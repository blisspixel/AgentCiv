"""Public HTTP checks against the actual local Cloudflare runtime and SQLite binding."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import time
from typing import Any
import unittest
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[2]
SERVICE = Path(__file__).resolve().parent
TOKENS = {f"agent:test-{index}": f"disposable-fixture-token-number-{index}" for index in range(10)}


class EdgeTests(unittest.TestCase):
    temporary: tempfile.TemporaryDirectory[str]
    workspace: Path
    process: subprocess.Popen[bytes]
    origin: str
    config: Path

    @classmethod
    def setUpClass(cls) -> None:
        cls.temporary = tempfile.TemporaryDirectory(prefix="agentciv-bulletin-fixtures-")
        cls.workspace = Path(cls.temporary.name).resolve()
        cls.config = cls.workspace / "wrangler.toml"
        config = (SERVICE / "wrangler.toml").read_text(encoding="utf-8")
        config = config.replace('main = "build/index.js"', f"main = {json.dumps((SERVICE / 'build/index.js').as_posix())}")
        config = config.replace('directory = "../../website/dist"', f"directory = {json.dumps((ROOT / 'website/dist').as_posix())}")
        config = config.replace('command = "worker-build --release --locked"', 'command = ""')
        cls.config.write_text(config, encoding="utf-8")
        grants = [{"principal": principal, "token_sha256": hashlib.sha256(token.encode()).hexdigest(), "moderator": principal == "agent:test-9"} for principal, token in TOKENS.items()]
        (cls.workspace / ".dev.vars").write_text(f"BOARD_GRANTS='{json.dumps(grants)}'\nREPORT_EMAIL=reports@example.invalid\n", encoding="utf-8")
        with socket.socket() as allocation:
            allocation.bind(("127.0.0.1", 0))
            port = allocation.getsockname()[1]
        cls.origin = f"http://127.0.0.1:{port}"
        try:
            cls.start(port)
        except BaseException:
            cls.stop()
            cls.cleanup()
            raise

    @classmethod
    def start(cls, port: int) -> None:
        executable = os.environ.get("AGENTCIV_WRANGLER") or shutil.which("wrangler")
        if executable is None:
            raise RuntimeError("Install wrangler 4.147.0 or set AGENTCIV_WRANGLER to its executable")
        # There are only disposable fixture tokens in this process, never production grants.
        with (cls.workspace / "runtime.log").open("ab") as log:
            cls.process = subprocess.Popen([executable, "dev", "--local", "--ip", "127.0.0.1", "--port", str(port), "--config", str(cls.config), "--persist-to", str(cls.workspace / "state")], cwd=SERVICE, stdout=log, stderr=subprocess.STDOUT)
        deadline = time.monotonic() + 45
        while time.monotonic() < deadline:
            try:
                with urlopen(cls.origin + "/api/board/info", timeout=1) as response:
                    if response.status == 200:
                        return
            except HTTPError as error:
                error.close()
                time.sleep(0.2)
            except (URLError, TimeoutError):
                if cls.process.poll() is not None:
                    break
                time.sleep(0.2)
        raise RuntimeError((cls.workspace / "runtime.log").read_text(encoding="utf-8", errors="replace"))

    @classmethod
    def stop(cls) -> None:
        if hasattr(cls, "process") and cls.process.poll() is None:
            if os.name == "nt":
                subprocess.run(["taskkill", "/PID", str(cls.process.pid), "/T", "/F"], check=False, capture_output=True)
            else:
                cls.process.terminate()
            cls.process.wait(timeout=15)

    @classmethod
    def tearDownClass(cls) -> None:
        cls.stop()
        cls.cleanup()

    @classmethod
    def cleanup(cls) -> None:
        # Check the exact explicitly-created fixture directory before recursive cleanup.
        if cls.workspace != Path(cls.temporary.name).resolve() or not cls.workspace.name.startswith("agentciv-bulletin-fixtures-"):
            raise RuntimeError("unexpected fixture cleanup target")
        for attempt in range(20):
            try:
                cls.temporary.cleanup()
                return
            except PermissionError:
                if attempt == 19:
                    raise
                time.sleep(0.25)

    def request(self, method: str, path: str, body: bytes | None = None, token: str | None = None, content_type: str = "application/json", accept: str = "application/json") -> tuple[int, dict[str, str], Any]:
        headers = {"Content-Type": content_type, "Accept": accept}
        if token is not None:
            headers["Authorization"] = "Bearer " + token
        request = Request(self.origin + path, data=body, headers=headers, method=method)
        try:
            response = urlopen(request, timeout=10)
        except HTTPError as error:
            response = error
        with response:
            data = response.read(1_000_000)
            result = json.loads(data) if "json" in response.headers.get("Content-Type", "") else data.decode()
            return response.status, dict(response.headers), result

    def submission(self, identifier: str, principal: str = "agent:test-0", reply: str | None = None) -> bytes:
        body: dict[str, Any] = {"subject": "Fixture topic", "text": "Actual runtime post"}
        if reply is not None:
            body["reply_to"] = reply
        return json.dumps({"publish": "public", "message": {"protocol_version": "0.1-draft", "type": "message", "id": identifier, "world": "civ:agentciv-board", "from": principal, "to": ["board:all"], "body": body, "unknown": {"source": "retained"}}}).encode()

    def test_access_and_failure_paths(self) -> None:
        raw = self.submission("access")
        for body, token, media, expected in [(raw, None, "application/json", 401), (raw, TOKENS["agent:test-1"], "application/json", 403), (raw, TOKENS["agent:test-0"], "text/plain", 415), (b"invalid", TOKENS["agent:test-0"], "application/json", 400), (b"x" * 9000, TOKENS["agent:test-0"], "application/json", 413)]:
            status, headers, _ = self.request("POST", "/api/board/posts", body, token, media)
            self.assertEqual(status, expected)
            self.assertEqual(headers.get("Cache-Control"), "no-store")
        for path in ["/api/board/posts?after=no", "/api/board/posts?after=0&before=1", "/api/board/posts?after=0&after=1"]:
            self.assertEqual(self.request("GET", path)[0], 400)
        self.assertEqual(self.request("GET", "/.well-known/agentciv")[0], 404)
        self.assertEqual(self.request("POST", "/api/board/info", b"{}")[0], 405)
        self.assertEqual(self.request("GET", "/api/other")[0], 404)

    def test_byte_retries_and_author_scoped_removal(self) -> None:
        raw = self.submission("retry")
        first = self.request("POST", "/api/board/posts", raw, TOKENS["agent:test-0"])
        self.assertEqual(first[0], 200)
        self.assertEqual(self.request("POST", "/api/board/posts", raw, TOKENS["agent:test-0"], "Application/JSON; charset=utf-8")[2], first[2])
        self.assertEqual(self.request("POST", "/api/board/posts", raw + b" ", TOKENS["agent:test-0"])[0], 409)
        number = first[2]["sequence"]
        public = self.request("GET", "/api/board/posts?after=0")[2]["posts"]
        entry = next(post for post in public if post["sequence"] == number)
        self.assertEqual(entry["original_submission"], raw.decode())
        self.assertEqual(entry["message"]["unknown"]["source"], "retained")
        self.assertEqual(self.request("DELETE", f"/api/board/posts/{number}", b"", TOKENS["agent:test-1"])[0], 403)
        self.assertEqual(self.request("DELETE", f"/api/board/posts/{number}", b"", TOKENS["agent:test-0"])[0], 200)
        self.assertEqual(self.request("POST", "/api/board/posts", raw, TOKENS["agent:test-0"])[2]["status"], "removed")
        self.assertNotIn("Actual runtime post", self.request("GET", f"/board/posts/{number}")[2])

    def test_pagination_concurrency_and_restart(self) -> None:
        from concurrent.futures import ThreadPoolExecutor
        raw = self.submission("concurrent", "agent:test-2")
        with ThreadPoolExecutor(max_workers=4) as clients:
            results = list(clients.map(lambda _: self.request("POST", "/api/board/posts", raw, TOKENS["agent:test-2"]), range(4)))
        self.assertTrue(all(result[0] == 200 for result in results))
        self.assertEqual(len({result[2]["sequence"] for result in results}), 1)
        for index in range(53):
            principal = f"agent:test-{3 + index // 18}"
            self.assertEqual(self.request("POST", "/api/board/posts", self.submission(f"page-{index}", principal), TOKENS[principal])[0], 200)
        first = self.request("GET", "/api/board/posts?after=0")[2]
        self.assertEqual(len(first["posts"]), 50)
        self.assertTrue(first["has_more"])
        second = self.request("GET", f"/api/board/posts?after={first['next_after']}")[2]
        self.assertTrue(all(post["sequence"] > first["next_after"] for post in second["posts"]))
        self.stop()
        self.start(int(self.origin.rsplit(":", 1)[1]))
        self.assertEqual(self.request("POST", "/api/board/posts", raw, TOKENS["agent:test-2"])[2], results[0][2])
        self.assertEqual(self.request("GET", "/api/board/posts?after=0")[2], first)

    def test_static_manifest_and_inspection_view(self) -> None:
        status, headers, root = self.request("GET", "/")
        self.assertEqual(status, 200)
        self.assertEqual(root["interfaces"]["directory"], "/directory.json")
        self.assertEqual(headers.get("Vary"), "Accept")
        self.assertEqual(self.request("GET", "/agent.json")[2]["interfaces"]["directory"], "/directory.json")
        self.assertEqual(self.request("GET", "/.well-known/agentciv-services")[2]["access"]["bulletin_read"], "public")
        self.assertEqual(self.request("GET", "/directory.json")[2]["schema_version"], 1)
        for path, text in [("/", "Connect a runtime"), ("/board", "Connect an agent"), ("/terms", "public publication"), ("/privacy", "Privacy notice"), ("/report", "reports@example.invalid")]:
            status, _, content = self.request("GET", path, accept="text/html")
            self.assertEqual(status, 200, path)
            self.assertIn(text.lower(), content.lower(), path)


if __name__ == "__main__":
    unittest.main()
