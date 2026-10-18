"""Manifest-derived stdlib reads, refusal evidence, and strict probe bounds."""
from __future__ import annotations

from contextlib import redirect_stderr
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import json
from pathlib import Path
import tempfile
from threading import Thread
import unittest
from unittest.mock import patch
from urllib.request import build_opener

from services.bulletin import probe_entry as probe


class ProbeTests(unittest.TestCase):
    def setUp(self) -> None:
        self.requests: list[tuple[str, str, str, str | None]] = []
        self.manifest = {"service": "agentciv.io/0.1-experimental", "interfaces": {
            "instructions": "/llms.txt", "info": "/api/board/info", "new_catalog": "/catalog.json"}}
        manifest = json.dumps(self.manifest).encode()
        self.responses: dict[str, tuple[int, bytes, dict[str, str]]] = {
            "/agent.json": (200, manifest, {}), "/": (200, manifest, {}),
            "/.well-known/agentciv-services": (200, manifest, {}),
            "/llms.txt": (200, b"# AgentCiv service instructions\nWelcome.\n", {}),
            "/api/board/info": (200, b'{"service":"web-bulletin/0.1-experimental"}', {}),
            "/robots.txt": (200, b"User-agent: *\nAllow: /\n", {}),
            "/catalog.json": (200, b'{"newly_advertised":true}', {}),
            probe.MISSING_PATH: (404, b"missing", {}),
        }
        self.block_default = False
        self.malformed_response: bytes | None = None
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def respond(self, head: bool) -> None:
                agent = self.headers.get("User-Agent", "")
                owner.requests.append((self.command, self.path, agent, self.headers.get("Authorization")))
                if owner.malformed_response is not None:
                    self.connection.sendall(owner.malformed_response)
                    return
                if owner.block_default and agent.startswith("Python-urllib/"):
                    status, body, headers = 403, b"error code: 1010", {"Server": "cloudflare", "CF-Ray": "fixture"}
                else:
                    status, body, headers = owner.responses.get(self.path, (404, b"missing", {}))
                    accept = self.headers.get("Accept")
                    if self.path == "/" and accept in ("text/markdown", "text/plain"):
                        status, body, headers = 200, b"# AgentCiv service instructions\nWelcome.\n", {"Content-Type": accept + "; charset=utf-8", "Vary": "Accept"}
                self.send_response_only(status)
                for key, value in headers.items():
                    self.send_header(key, value)
                self.end_headers()
                if not head:
                    self.wfile.write(body)

            def do_GET(self) -> None:
                self.respond(False)

            def do_HEAD(self) -> None:
                self.respond(True)

            def log_message(self, format: str, *args: object) -> None:
                pass

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = Thread(target=lambda: self.server.serve_forever(poll_interval=0.01), daemon=True)
        self.thread.start()
        self.origin = f"http://127.0.0.1:{self.server.server_port}"
        self.addCleanup(self.server.server_close)
        self.addCleanup(self.thread.join)
        self.addCleanup(self.server.shutdown)

    def test_newly_advertised_reads_both_identities_head_and_no_credentials(self) -> None:
        results = probe.probe(self.origin + "/")
        self.assertTrue(all(item.passed for item in results), results)
        self.assertEqual(len(results), len(self.requests))
        self.assertLessEqual(len(results), probe.MAX_REQUESTS)
        for client in ("python-default", "truthful-control"):
            rows = [item for item in results if item.client == client]
            self.assertEqual(len([item for item in rows if item.path == "/catalog.json"]), 1)
            self.assertEqual(len([item for item in rows if item.method == "HEAD"]), 3)
            self.assertEqual(len([item for item in rows if item.accept is not None]), 4)
            self.assertIn(probe.MISSING_PATH, [item.path for item in rows])
        default_agent = dict(build_opener().addheaders)["User-agent"]
        self.assertTrue(default_agent.startswith("Python-urllib/"))
        self.assertEqual({item.user_agent for item in results}, {default_agent, probe.CONTROL_AGENT})
        self.assertTrue(all(request[3] is None for request in self.requests))
        self.assertEqual({request[0] for request in self.requests}, {"GET", "HEAD"})

    def test_default_refusal_preserved_control_manifest_still_discovers_child(self) -> None:
        self.block_default = True
        results = probe.probe(self.origin)
        default = [item for item in results if item.client == "python-default"]
        self.assertTrue(all(item.status == 403 and not item.passed for item in default))
        self.assertIn("/catalog.json", [item.path for item in default])
        self.assertTrue(all(item.passed for item in results if item.client == "truthful-control"))
        self.assertEqual(default[0].body_text, "error code: 1010")
        self.assertEqual(default[0].headers, {"Server": "cloudflare", "CF-Ray": "fixture"})
        self.assertIsNotNone(default[0].body_sha256)

    def test_refused_child_fails_even_with_successful_manifest(self) -> None:
        self.responses["/catalog.json"] = (403, b"error code: 1010", {})
        children = [item for item in probe.probe(self.origin) if item.path == "/catalog.json"]
        self.assertEqual(len(children), 2)
        self.assertTrue(all(not item.passed and item.status == 403 for item in children))

    def test_redirect_never_followed_and_location_not_exported(self) -> None:
        self.responses["/catalog.json"] = (302, b"moved", {"Location": "https://example.invalid/secret"})
        results = probe.probe(self.origin)
        children = [item for item in results if item.path == "/catalog.json"]
        self.assertTrue(all(item.status == 302 and not item.passed for item in children))
        self.assertFalse(any("secret" in request[1] for request in self.requests))
        self.assertNotIn("secret", json.dumps([item.headers for item in results]))

    def test_malformed_external_and_oversized_descriptors_never_add_requests(self) -> None:
        cases = [b"not-json", b"[]", b'{"service":"wrong"}', b"x" * (probe.MAX_BYTES + 20)]
        for target in ("https://outside.invalid/x", "//outside.invalid/x", "/x#fragment", "/board/publish", "/theme?set=dark", "/%2foutside", "/x\\other", "/x\n"):
            cases.append(json.dumps({**self.manifest, "interfaces": {"bad": target}}).encode())
        cases.append(json.dumps({**self.manifest, "interfaces": {str(i): f"/item-{i}" for i in range(probe.MAX_INTERFACES + 1)}}).encode())
        cases.append(b'{"service":"agentciv.io/0.1-experimental","interfaces":{"a":"/a","a":"/b"}}')
        for raw in cases:
            with self.subTest(raw=raw[:80]):
                self.requests.clear()
                self.responses["/agent.json"] = (200, raw, {})
                results = probe.probe(self.origin)
                self.assertFalse(all(item.passed for item in results))
                self.assertTrue(all(request[1] in {*probe.PUBLIC_PATHS, "/agent.json"} for request in self.requests))
                self.assertLessEqual(len(self.requests), probe.MAX_REQUESTS)

    def test_body_limits_wrong_representation_and_unknown_path_block_are_failures(self) -> None:
        self.responses["/llms.txt"] = (200, b"x" * (probe.MAX_BYTES + 20), {})
        self.responses["/catalog.json"] = (200, b"<html>challenge</html>", {})
        self.responses[probe.MISSING_PATH] = (403, b"error code: 1010", {})
        results = probe.probe(self.origin)
        for item in results:
            if item.path == "/llms.txt":
                self.assertTrue(item.truncated)
                self.assertEqual(item.bytes_read, probe.MAX_BYTES + 1)
            if item.path in ("/llms.txt", "/catalog.json", probe.MISSING_PATH):
                self.assertFalse(item.passed)
        self.assertFalse(probe.expected_body("/agent.json", b"[]"))
        self.assertFalse(probe.expected_body("/catalog.json", b"\xff"))
        self.assertTrue(probe.expected_body("/llms.txt", b"# AgentCiv service instructions\r\n"))

    def test_transport_deadline_and_malformed_http_fail_without_retry(self) -> None:
        with patch("urllib.request.OpenerDirector.open", side_effect=TimeoutError("fixture")) as opening:
            results = probe.probe(self.origin)
        self.assertEqual(opening.call_count, len(results))
        self.assertTrue(all(item.error == "TimeoutError" and not item.passed for item in results))
        with patch("services.bulletin.probe_entry.time.monotonic", side_effect=[0.0] + [121.0] * 64), \
            patch("urllib.request.OpenerDirector.open") as opening:
            results = probe.probe(self.origin)
        opening.assert_not_called()
        self.assertTrue(all(item.error == "probe_budget_exceeded" for item in results))
        self.malformed_response = b"invalid-http-status\r\n\r\n"
        results = probe.probe(self.origin)
        self.assertTrue(all(item.error == "BadStatusLine" and not item.passed for item in results))

    def test_cli_output_reserved_before_requests_and_failures_retained(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "probe.json"
            self.assertEqual(probe.main(["--origin", self.origin, "--output", str(path)]), 0)
            document = json.loads(path.read_bytes())
            self.assertTrue(document["passed"])
            count = len(self.requests)
            with self.assertRaises(FileExistsError):
                probe.main(["--origin", self.origin, "--output", str(path)])
            self.assertEqual(len(self.requests), count)
            self.block_default = True
            refused = Path(temporary) / "refused.json"
            self.assertEqual(probe.main(["--origin", self.origin, "--output", str(refused)]), 1)
            self.assertFalse(json.loads(refused.read_bytes())["passed"])

    def test_invalid_origins_fail_before_network_or_output(self) -> None:
        for value in ("http://agentciv.io", "https://a.invalid/path", "https://a.invalid?x=y", "https://a.invalid/#x", "https://u:p@a.invalid", "https://a.invalid:99999", "https://", "ftp://a.invalid"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                probe.origin_value(value)
        with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            probe.main([])


if __name__ == "__main__":
    unittest.main()
