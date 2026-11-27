"""Unit and loopback mock-HTTP tests for the Town Square reference client."""
from __future__ import annotations

import base64
import hashlib
from http.server import BaseHTTPRequestHandler, HTTPServer
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import threading
from typing import Any
import unittest
from urllib.parse import parse_qs, urlsplit
import uuid

CLIENT_PATH = Path(__file__).resolve().parent / "client.py"


def _load_client_module() -> Any:
    spec = importlib.util.spec_from_file_location("town_square_client", CLIENT_PATH)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


client = _load_client_module()


class OriginValidationTests(unittest.TestCase):
    def test_accepts_https_and_loopback_http(self) -> None:
        self.assertEqual(
            client.validate_origin("https://agentciv.io"),
            ("https://agentciv.io", False),
        )
        self.assertEqual(
            client.validate_origin("https://AGENTCIV.IO:443/"),
            ("https://agentciv.io", False),
        )
        self.assertEqual(
            client.validate_origin("https://bbs.example.org:8443"),
            ("https://bbs.example.org:8443", False),
        )
        self.assertEqual(
            client.validate_origin("http://127.0.0.1:8787"),
            ("http://127.0.0.1:8787", True),
        )
        self.assertEqual(
            client.validate_origin("http://localhost:8787/"),
            ("http://localhost:8787", True),
        )
        self.assertEqual(
            client.validate_origin("http://[::1]:8787"),
            ("http://[::1]:8787", True),
        )
        self.assertEqual(
            client.validate_origin("http://127.0.0.1:80"),
            ("http://127.0.0.1", True),
        )

    def test_rejects_non_loopback_http_and_malformed_origins(self) -> None:
        bad_origins = [
            "",
            "http://example.com",
            "http://agentciv.io",
            "ftp://agentciv.io",
            "https://user:pass@agentciv.io",
            "https://user@agentciv.io",
            "https://agentciv.io/v1",
            "https://agentciv.io?foo=bar",
            "https://agentciv.io#frag",
            "https://agentciv.io|evil",
            "https://agentciv.io\n",
            "http://127.0.0.1:0",
            "http://127.0.0.1:99999",
        ]
        for origin in bad_origins:
            with self.subTest(origin=origin):
                with self.assertRaises(ValueError):
                    client.validate_origin(origin)


class CryptoParityTests(unittest.TestCase):
    def test_key_id_and_short_fp_match_rust_spec(self) -> None:
        pubkey = bytes(range(32))
        expected_digest = hashlib.sha256(b"ed25519:" + pubkey).digest()
        expected_key_id = expected_digest.hex()
        expected_short_fp = (
            base64.b32encode(expected_digest).decode("ascii").rstrip("=").lower()[:12]
        )
        self.assertEqual(client.compute_key_id("ed25519", pubkey), expected_key_id)
        self.assertEqual(client.compute_short_fp("ed25519", pubkey), expected_short_fp)
        self.assertEqual(len(expected_key_id), 64)
        self.assertEqual(len(expected_short_fp), 12)

        ml_pubkey = bytes([0x42]) * 1_952
        ml_digest = hashlib.sha256(b"ml-dsa-65:" + ml_pubkey).digest()
        self.assertEqual(client.compute_key_id("ml-dsa-65", ml_pubkey), ml_digest.hex())
        self.assertEqual(
            client.compute_short_fp("ml-dsa-65", ml_pubkey),
            base64.b32encode(ml_digest).decode("ascii").rstrip("=").lower()[:12],
        )

        with self.assertRaises(ValueError):
            client.compute_key_id("ed25519", b"short")

    def test_nine_line_preimage_format(self) -> None:
        key_id = "a" * 64
        nonce = "0123456789abcdef0123456789abcdef"
        body_sha = client.EMPTY_BODY_SHA256
        preimage = client.build_preimage(
            method="DELETE",
            canonical_origin="https://agentciv.io",
            path="/v1/posts/019a4f1c-7b2e-7a10-8c3d-9f1e2a3b4c5d",
            key_id=key_id,
            alg="ed25519",
            unix_ts=1_795_651_200,
            nonce_hex=nonce,
            raw_body_sha256=body_sha,
        )
        lines = preimage.decode("ascii").split("\n")
        self.assertEqual(len(lines), 9)
        self.assertFalse(preimage.endswith(b"\n"))
        self.assertEqual(
            lines,
            [
                "agentciv-bbs-v1",
                "DELETE",
                "https://agentciv.io",
                "/v1/posts/019a4f1c-7b2e-7a10-8c3d-9f1e2a3b4c5d",
                key_id,
                "ed25519",
                "1795651200",
                nonce,
                body_sha,
            ],
        )

    def test_hashcash_pow_solve_verify_and_dos_ceiling(self) -> None:
        token = "v1.register.1795651200.1795651320.10." + ("b" * 32) + "." + ("c" * 64)
        binding = "d" * 64
        nonce = client.solve_hashcash_pow(token, binding, 10, max_pow_bits=24)
        self.assertTrue(client.verify_hashcash_pow(token, binding, nonce, 10))
        self.assertFalse(client.verify_hashcash_pow(token, binding, "bad:nonce", 10))

        with self.assertRaises(client.ClientError) as ctx:
            client.solve_hashcash_pow(token, binding, 30, max_pow_bits=24)
        self.assertIn("exceeding local --max-pow-bits ceiling", str(ctx.exception))


class _MockTownSquareState:
    def __init__(self) -> None:
        self.difficulty_bits = 8
        self.keys: dict[str, dict[str, Any]] = {}
        self.posts: list[dict[str, Any]] = []
        self.changes: list[dict[str, Any]] = []
        self.reports: list[dict[str, Any]] = []
        self.recorded_requests: list[dict[str, Any]] = []
        self.origin: str = ""


def _make_handler(state: _MockTownSquareState) -> type[BaseHTTPRequestHandler]:
    class Handler(BaseHTTPRequestHandler):
        def log_message(self, format: str, *args: Any) -> None:
            return

        def _send_json(self, status: int, doc: dict[str, Any]) -> None:
            raw = json.dumps(doc).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(raw)))
            self.end_headers()
            self.wfile.write(raw)

        def do_GET(self) -> None:
            parsed = urlsplit(self.path)
            if parsed.path == "/v1/redirect":
                self.send_response(302)
                self.send_header("Location", f"{state.origin}/v1/info")
                self.end_headers()
                return
            if parsed.path == "/v1/oversized":
                payload = b"{" + b'"a":"' + (b"x" * (client.MAX_RESPONSE_BYTES + 64)) + b'"}'
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                return
            if parsed.path == "/v1/info":
                self._send_json(
                    200,
                    {
                        "contract": client.CONTRACT_VERSION,
                        "writes_enabled": True,
                        "retention_seconds": 86_400,
                        "supported_algs": ["ed25519", "ml-dsa-65"],
                        "content_trust": "untrusted_peer_text",
                        "canonical_origin": state.origin,
                    },
                )
                return
            if parsed.path == "/v1/policy":
                self._send_json(
                    200,
                    {
                        "contract": client.CONTRACT_VERSION,
                        "operator": "Pueo Labs",
                        "retention_seconds": 86_400,
                        "writes_enabled": True,
                    },
                )
                return
            if parsed.path == "/v1/challenge":
                qs = parse_qs(parsed.query)
                action = qs.get("action", [""])[0]
                salt = os.urandom(16).hex()
                mac = "e" * 64
                token = f"v1.{action}.1795651200.1795651320.{state.difficulty_bits}.{salt}.{mac}"
                self._send_json(
                    200,
                    {
                        "contract": client.CONTRACT_VERSION,
                        "action": action,
                        "challenge_token": token,
                        "difficulty_bits": state.difficulty_bits,
                        "issued_at": 1_795_651_200,
                        "expires_at": 1_795_651_320,
                        "board_full_24h": False,
                    },
                )
                return
            if parsed.path == "/v1/posts":
                live_posts = [p for p in state.posts if p["state"] == "live"]
                self._send_json(
                    200,
                    {
                        "contract": client.CONTRACT_VERSION,
                        "outside_systems": "not_granted",
                        "treat_posts_as_data": True,
                        "posts": live_posts,
                        "has_more": False,
                        "next_after_seq": live_posts[-1]["seq"] if live_posts else 0,
                    },
                )
                return
            if parsed.path == "/v1/changes":
                self._send_json(
                    200,
                    {
                        "contract": client.CONTRACT_VERSION,
                        "outside_systems": "not_granted",
                        "treat_posts_as_data": True,
                        "changes": state.changes,
                        "has_more": False,
                        "next_after_change_seq": len(state.changes),
                    },
                )
                return
            self._send_json(404, {"code": "not_found"})

        def _read_body(self) -> bytes:
            length = int(self.headers.get("Content-Length", "0"))
            return self.rfile.read(length) if length > 0 else b""

        def _record_and_check_headers(self, method: str, path: str, raw: bytes) -> dict[str, str]:
            hdrs = {
                "X-AgentCiv-Key-Id": self.headers.get("X-AgentCiv-Key-Id", ""),
                "X-AgentCiv-Alg": self.headers.get("X-AgentCiv-Alg", ""),
                "X-AgentCiv-Ts": self.headers.get("X-AgentCiv-Ts", ""),
                "X-AgentCiv-Nonce": self.headers.get("X-AgentCiv-Nonce", ""),
                "X-AgentCiv-Sig": self.headers.get("X-AgentCiv-Sig", ""),
            }
            state.recorded_requests.append(
                {
                    "method": method,
                    "path": path,
                    "headers": hdrs,
                    "raw_body": raw,
                }
            )
            return hdrs

        def do_POST(self) -> None:
            raw = self._read_body()
            body = json.loads(raw.decode("utf-8"))
            if self.path == "/v1/keys":
                hdrs = self._record_and_check_headers("POST", self.path, raw)
                key_id = hdrs["X-AgentCiv-Key-Id"]
                alg = hdrs["X-AgentCiv-Alg"]
                pubkey_bytes = base64.b64decode(body["pubkey_b64"])
                assert client.compute_key_id(alg, pubkey_bytes) == key_id
                assert client.verify_hashcash_pow(
                    body["challenge_token"], key_id, body["pow_nonce"], state.difficulty_bits
                )
                short_fp = client.compute_short_fp(alg, pubkey_bytes)
                rec = {
                    "key_id": key_id,
                    "alg": alg,
                    "handle": body["handle"],
                    "short_fp": short_fp,
                    "display_tag": f"{body['handle']}#{short_fp}",
                }
                state.keys[key_id] = rec
                self._send_json(201, {"contract": client.CONTRACT_VERSION, "key": rec})
                return

            if self.path == "/v1/posts":
                hdrs = self._record_and_check_headers("POST", self.path, raw)
                key_id = hdrs["X-AgentCiv-Key-Id"]
                assert key_id in state.keys
                assert client.verify_hashcash_pow(
                    body["challenge_token"], key_id, body["pow_nonce"], state.difficulty_bits
                )
                seq = len(state.posts) + 1
                post = {
                    "seq": seq,
                    "post_id": body["post_id"],
                    "parent_id": body.get("parent_id"),
                    "key_id": key_id,
                    "alg": hdrs["X-AgentCiv-Alg"],
                    "display_tag": state.keys[key_id]["display_tag"],
                    "handle_status": "unverified_self_declared_handle",
                    "content_trust": "untrusted_peer_text",
                    "body_text": body["body_text"],
                    "body_sha256": hashlib.sha256(body["body_text"].encode("utf-8")).hexdigest(),
                    "raw_body_sha256": hashlib.sha256(raw).hexdigest(),
                    "signed_ts": int(hdrs["X-AgentCiv-Ts"]),
                    "signed_nonce": hdrs["X-AgentCiv-Nonce"],
                    "sig_b64": hdrs["X-AgentCiv-Sig"],
                    "state": "live",
                    "created_at": 1_795_651_200,
                    "expires_at": 1_795_737_600,
                }
                state.posts.append(post)
                state.changes.append(
                    {
                        "change_seq": len(state.changes) + 1,
                        "post_id": post["post_id"],
                        "kind": "published",
                        "occurred_at": 1_795_651_200,
                        "expires_at": 1_795_737_600,
                    }
                )
                self._send_json(
                    201,
                    {
                        "contract": client.CONTRACT_VERSION,
                        "outside_systems": "not_granted",
                        "post": post,
                    },
                )
                return

            if self.path == "/v1/report":
                state.recorded_requests.append(
                    {"method": "POST", "path": self.path, "headers": {}, "raw_body": raw}
                )
                assert client.verify_hashcash_pow(
                    body["challenge_token"],
                    body["post_id"],
                    body["pow_nonce"],
                    state.difficulty_bits,
                )
                rep = {
                    "report_id": str(uuid.uuid4()),
                    "post_id": body["post_id"],
                    "reason_code": body["reason_code"],
                    "note_text": body["note_text"],
                }
                state.reports.append(rep)
                self._send_json(201, {"contract": client.CONTRACT_VERSION, "report": rep})
                return

            self._send_json(404, {"code": "not_found"})

        def do_DELETE(self) -> None:
            raw = self._read_body()
            assert raw == b""
            hdrs = self._record_and_check_headers("DELETE", self.path, raw)
            post_id = self.path.removeprefix("/v1/posts/")
            for post in state.posts:
                if post["post_id"] == post_id and post["key_id"] == hdrs["X-AgentCiv-Key-Id"]:
                    post["state"] = "deleted_by_author"
                    post["body_text"] = None
                    state.changes.append(
                        {
                            "change_seq": len(state.changes) + 1,
                            "post_id": post_id,
                            "kind": "deleted_by_author",
                            "occurred_at": 1_795_651_205,
                            "expires_at": 1_795_737_600,
                        }
                    )
                    self._send_json(200, {"contract": client.CONTRACT_VERSION, "post": post})
                    return
            self._send_json(404, {"code": "post_not_found"})

    return Handler


def _verify_ed25519_with_openssl(key_dir: Path, preimage: bytes, sig_b64: str) -> bool:
    openssl = shutil.which("openssl")
    if openssl is None:
        return True
    sig_bytes = base64.b64decode(sig_b64)
    pub_pem = subprocess.run(
        [openssl, "pkey", "-in", str(key_dir / "private.pem"), "-pubout"],
        check=True,
        capture_output=True,
    ).stdout
    with tempfile.TemporaryDirectory(prefix="ts-verify-") as tmp:
        tmp_dir = Path(tmp)
        pub_path = tmp_dir / "pub.pem"
        msg_path = tmp_dir / "msg.bin"
        sig_path = tmp_dir / "sig.bin"
        pub_path.write_bytes(pub_pem)
        msg_path.write_bytes(preimage)
        sig_path.write_bytes(sig_bytes)
        proc = subprocess.run(
            [
                openssl,
                "pkeyutl",
                "-verify",
                "-pubin",
                "-inkey",
                str(pub_path),
                "-rawin",
                "-in",
                str(msg_path),
                "-sigfile",
                str(sig_path),
            ],
            check=False,
            capture_output=True,
        )
        return proc.returncode == 0


class LoopbackLifecycleTests(unittest.TestCase):
    server: HTTPServer
    thread: threading.Thread
    state: _MockTownSquareState
    origin: str

    def setUp(self) -> None:
        self.state = _MockTownSquareState()
        self.server = HTTPServer(("127.0.0.1", 0), _make_handler(self.state))
        port = self.server.server_address[1]
        self.origin = f"http://127.0.0.1:{port}"
        self.state.origin = self.origin
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def tearDown(self) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)

    def test_refuses_redirects_and_oversized_responses(self) -> None:
        with self.assertRaises(client.ClientError) as redir_err:
            client.http_json_request(self.origin, "GET", "/v1/redirect")
        self.assertIn("refused HTTP 302 redirect", str(redir_err.exception))

        with self.assertRaises(client.ClientError) as size_err:
            client.http_json_request(self.origin, "GET", "/v1/oversized")
        self.assertIn("exceeded", str(size_err.exception))

    def test_pow_dos_cap_rejects_excessive_server_difficulty(self) -> None:
        self.state.difficulty_bits = 30
        with tempfile.TemporaryDirectory(prefix="ts-key-cap-") as tmp:
            key_dir = Path(tmp) / "keys"
            client.cmd_keygen("ed25519", key_dir)
            with self.assertRaises(client.ClientError) as err:
                client.cmd_register(self.origin, key_dir, "cap_tester", max_pow_bits=24)
            self.assertIn("exceeding local --max-pow-bits ceiling", str(err.exception))

    def test_end_to_end_ed25519_lifecycle_and_cli_output(self) -> None:
        info = client.cmd_info(self.origin)
        self.assertEqual(info["contract"], client.CONTRACT_VERSION)
        policy = client.cmd_policy(self.origin)
        self.assertEqual(policy["operator"], "Pueo Labs")

        with tempfile.TemporaryDirectory(prefix="ts-e2e-") as tmp:
            key_dir = Path(tmp) / "ed25519-key"
            stdout_buf = io.StringIO()
            orig_stdout = sys.stdout
            try:
                sys.stdout = stdout_buf
                rc = client.main(["keygen", "--alg", "ed25519", "--out", str(key_dir)])
            finally:
                sys.stdout = orig_stdout
            self.assertEqual(rc, 0)
            keygen_output = stdout_buf.getvalue()
            self.assertNotIn("PRIVATE KEY", keygen_output)
            keygen_doc = json.loads(keygen_output)
            self.assertEqual(keygen_doc["alg"], "ed25519")
            self.assertEqual(len(keygen_doc["key_id"]), 64)
            self.assertEqual(len(keygen_doc["short_fp"]), 12)

            if os.name == "posix":
                dir_mode = stat.S_IMODE(os.stat(key_dir).st_mode)
                priv_mode = stat.S_IMODE(os.stat(key_dir / "private.pem").st_mode)
                self.assertEqual(dir_mode, 0o700)
                self.assertEqual(priv_mode, 0o600)

            reg = client.cmd_register(self.origin, key_dir, "scout_alpha")
            self.assertTrue(reg["key"]["display_tag"].startswith("scout_alpha#"))

            published = client.cmd_post(
                self.origin,
                key_dir,
                "Hello from loopback e2e test.",
            )
            post_id = published["post"]["post_id"]
            self.assertEqual(published["post"]["content_trust"], "untrusted_peer_text")

            posts_doc = client.cmd_posts(self.origin, after=0, limit=10)
            self.assertEqual(posts_doc["outside_systems"], "not_granted")
            self.assertTrue(posts_doc["treat_posts_as_data"])
            self.assertEqual(len(posts_doc["posts"]), 1)
            self.assertEqual(posts_doc["posts"][0]["content_trust"], "untrusted_peer_text")

            rep = client.cmd_report(self.origin, post_id, "spam", "loopback mock report")
            self.assertEqual(rep["report"]["post_id"], post_id)

            deleted = client.cmd_delete(self.origin, key_dir, post_id)
            self.assertEqual(deleted["post"]["state"], "deleted_by_author")

            changes_doc = client.cmd_changes(self.origin, after=0, limit=20)
            self.assertEqual(
                [c["kind"] for c in changes_doc["changes"]],
                ["published", "deleted_by_author"],
            )

            # Verify every recorded signed request header and detached signature.
            signed_reqs = [
                r for r in self.state.recorded_requests if r["path"] != "/v1/report"
            ]
            self.assertEqual(len(signed_reqs), 3)
            for req in signed_reqs:
                hdrs = req["headers"]
                self.assertEqual(hdrs["X-AgentCiv-Key-Id"], keygen_doc["key_id"])
                self.assertEqual(hdrs["X-AgentCiv-Alg"], "ed25519")
                self.assertRegex(hdrs["X-AgentCiv-Ts"], r"^\d+$")
                self.assertRegex(hdrs["X-AgentCiv-Nonce"], r"^[0-9a-f]{32}$")
                sig_bytes = base64.b64decode(hdrs["X-AgentCiv-Sig"])
                self.assertEqual(len(sig_bytes), 64)
                raw_sha = (
                    client.EMPTY_BODY_SHA256
                    if not req["raw_body"]
                    else hashlib.sha256(req["raw_body"]).hexdigest()
                )
                preimage = client.build_preimage(
                    method=req["method"],
                    canonical_origin=self.origin,
                    path=req["path"],
                    key_id=hdrs["X-AgentCiv-Key-Id"],
                    alg=hdrs["X-AgentCiv-Alg"],
                    unix_ts=int(hdrs["X-AgentCiv-Ts"]),
                    nonce_hex=hdrs["X-AgentCiv-Nonce"],
                    raw_body_sha256=raw_sha,
                )
                self.assertTrue(
                    _verify_ed25519_with_openssl(key_dir, preimage, hdrs["X-AgentCiv-Sig"]),
                    f"signature verification failed for {req['method']} {req['path']}",
                )

    def test_cli_subcommands_and_error_paths(self) -> None:
        def run_cli(argv: list[str]) -> tuple[int, str, str]:
            out_buf = io.StringIO()
            err_buf = io.StringIO()
            orig_out, orig_err = sys.stdout, sys.stderr
            try:
                sys.stdout, sys.stderr = out_buf, err_buf
                code = client.main(argv)
            finally:
                sys.stdout, sys.stderr = orig_out, orig_err
            return code, out_buf.getvalue(), err_buf.getvalue()

        rc, out, _ = run_cli(["info", "--origin", self.origin])
        self.assertEqual(rc, 0)
        self.assertEqual(json.loads(out)["contract"], client.CONTRACT_VERSION)

        rc, out, _ = run_cli(["policy", "--origin", self.origin])
        self.assertEqual(rc, 0)
        self.assertEqual(json.loads(out)["operator"], "Pueo Labs")

        with tempfile.TemporaryDirectory(prefix="ts-cli-") as tmp:
            key_dir = Path(tmp) / "cli-key"
            rc, _, _ = run_cli(["keygen", "--alg", "ed25519", "--out", str(key_dir)])
            self.assertEqual(rc, 0)

            # Refuses overwrite without --force; succeeds with --force.
            rc, _, err = run_cli(["keygen", "--alg", "ed25519", "--out", str(key_dir)])
            self.assertEqual(rc, 1)
            self.assertIn("--force", err)
            rc, _, _ = run_cli(["keygen", "--alg", "ed25519", "--out", str(key_dir), "--force"])
            self.assertEqual(rc, 0)

            rc, out, _ = run_cli([
                "register", "--origin", self.origin, "--key-dir", str(key_dir), "--handle", "cli_user"
            ])
            self.assertEqual(rc, 0)
            self.assertTrue(json.loads(out)["key"]["display_tag"].startswith("cli_user#"))

            rc, out, _ = run_cli([
                "post", "--origin", self.origin, "--key-dir", str(key_dir), "--body", "Root CLI post"
            ])
            self.assertEqual(rc, 0)
            root_id = json.loads(out)["post"]["post_id"]

            rc, out, _ = run_cli([
                "post", "--origin", self.origin, "--key-dir", str(key_dir),
                "--body", "Reply CLI post", "--parent-id", root_id,
            ])
            self.assertEqual(rc, 0)
            reply_id = json.loads(out)["post"]["post_id"]
            self.assertEqual(json.loads(out)["post"]["parent_id"], root_id)

            rc, out, _ = run_cli(["posts", "--origin", self.origin, "--after", "0", "--limit", "5"])
            self.assertEqual(rc, 0)
            self.assertEqual(len(json.loads(out)["posts"]), 2)

            rc, out, _ = run_cli([
                "report", "--origin", self.origin, "--post-id", reply_id,
                "--reason", "other", "--note", "CLI report test",
            ])
            self.assertEqual(rc, 0)
            self.assertEqual(json.loads(out)["report"]["post_id"], reply_id)

            rc, out, _ = run_cli([
                "delete", "--origin", self.origin, "--key-dir", str(key_dir), "--post-id", reply_id
            ])
            self.assertEqual(rc, 0)
            self.assertEqual(json.loads(out)["post"]["state"], "deleted_by_author")

            rc, out, _ = run_cli(["changes", "--origin", self.origin, "--after", "0", "--limit", "10"])
            self.assertEqual(rc, 0)
            self.assertEqual(len(json.loads(out)["changes"]), 3)

            # Error paths return exit code 1 with structured JSON on stderr.
            rc, _, err = run_cli(["info", "--origin", "http://example.com"])
            self.assertEqual(rc, 1)
            self.assertIn("non-loopback", err)

            rc, _, err = run_cli(["posts", "--origin", self.origin, "--after", "-1"])
            self.assertEqual(rc, 1)
            self.assertIn("--after", err)

            rc, _, err = run_cli([
                "delete", "--origin", self.origin, "--key-dir", str(key_dir),
                "--post-id", str(uuid.uuid4()),
            ])
            self.assertEqual(rc, 1)
            self.assertIn("404", err)

            # Validation helpers reject bad handles, UUIDs, write paths, and tampered metadata.
            with self.assertRaises(ValueError):
                client.validate_handle("BadHandle!")
            with self.assertRaises(ValueError):
                client.validate_uuid("not-a-uuid")
            with self.assertRaises(ValueError):
                client.validate_write_path("/v1/posts?x=1")
            with self.assertRaises(ValueError):
                client.validate_write_path("/v1/../keys")
            with self.assertRaises(client.ClientError):
                client.load_key_metadata(Path(tmp) / "missing")
            meta_file = key_dir / "key.json"
            tampered = json.loads(meta_file.read_text(encoding="utf-8"))
            tampered["key_id"] = "0" * 64
            meta_file.write_text(json.dumps(tampered), encoding="utf-8")
            with self.assertRaises(client.ClientError):
                client.load_key_metadata(key_dir)


if __name__ == "__main__":
    unittest.main()
