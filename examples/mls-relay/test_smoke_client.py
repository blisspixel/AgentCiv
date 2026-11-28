#!/usr/bin/env python3
"""Unit and live loopback integration tests for `examples/mls-relay/smoke_client.py`."""
from __future__ import annotations

import contextlib
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
from typing import Any
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import smoke_client  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[2]


def _find_agentciv_host_binary() -> Path | None:
    env_bin = os.environ.get("AGENTCIV_HOST_BIN")
    if env_bin:
        candidate = Path(env_bin)
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return candidate

    target_dir = Path(os.environ.get("CARGO_TARGET_DIR", str(REPO_ROOT / "target")))
    for profile in ("debug", "release"):
        candidate = target_dir / profile / "agentciv-host"
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return candidate
    return None


class _MockServer:
    def __init__(self, handler_cls: type[BaseHTTPRequestHandler]) -> None:
        self._server = ThreadingHTTPServer(("127.0.0.1", 0), handler_cls)
        self.origin = f"http://127.0.0.1:{self._server.server_port}"
        self._thread = threading.Thread(target=self._server.serve_forever, daemon=True)

    def __enter__(self) -> _MockServer:
        self._thread.start()
        return self

    def __exit__(self, *args: object) -> None:
        self._server.shutdown()
        self._server.server_close()
        self._thread.join(timeout=5.0)


class OriginValidationTests(unittest.TestCase):
    def test_accepts_loopback_origins(self) -> None:
        self.assertEqual(
            smoke_client.validate_loopback_origin("http://127.0.0.1:8788"),
            "http://127.0.0.1:8788",
        )
        self.assertEqual(
            smoke_client.validate_loopback_origin("http://localhost:8788/"),
            "http://localhost:8788",
        )
        self.assertEqual(
            smoke_client.validate_loopback_origin("http://[::1]:8788"),
            "http://[::1]:8788",
        )
        self.assertEqual(
            smoke_client.validate_loopback_origin("http://127.0.0.1:80"),
            "http://127.0.0.1",
        )

    def test_rejects_non_loopback_and_malformed_origins(self) -> None:
        bad_origins = [
            "",
            "https://127.0.0.1:8788",
            "https://agentciv.io",
            "http://example.com:8788",
            "http://0.0.0.0:8788",
            "http://user:pass@127.0.0.1:8788",
            "http://127.0.0.1:8788/mls/v1/info",
            "http://127.0.0.1:8788?foo=bar",
            "http://127.0.0.1:8788#frag",
            "http://127.0.0.1:0",
            "http://127.0.0.1:8788\n",
            "http://127.0.0.1:8788|extra",
            "http://\u00e9:8788",
            "http://:8788",
        ]
        for origin in bad_origins:
            with self.subTest(origin=origin):
                with self.assertRaises(ValueError):
                    smoke_client.validate_loopback_origin(origin)


class MerkleVerificationTests(unittest.TestCase):
    def test_merkle_root_and_consistency_proof_edges(self) -> None:
        empty = smoke_client.merkle_root_hash([])
        self.assertEqual(empty, smoke_client.EMPTY_ROOT_HASH)
        self.assertTrue(
            smoke_client.verify_consistency_proof(0, 0, empty, empty, [])
        )
        self.assertFalse(
            smoke_client.verify_consistency_proof(0, 0, b"\x00" * 32, empty, [])
        )
        self.assertFalse(
            smoke_client.verify_consistency_proof(2, 1, empty, empty, [])
        )

        leaves = [
            smoke_client.ciphertext_leaf_hash(hashlib.sha256(f"msg-{i}".encode()).digest())
            for i in range(4)
        ]
        root1 = smoke_client.merkle_root_hash(leaves[:1])
        root2 = smoke_client.merkle_root_hash(leaves[:2])
        root3 = smoke_client.merkle_root_hash(leaves[:3])
        root4 = smoke_client.merkle_root_hash(leaves[:4])

        self.assertTrue(smoke_client.verify_consistency_proof(0, 3, empty, root3, []))
        self.assertTrue(smoke_client.verify_consistency_proof(2, 2, root2, root2, []))
        self.assertFalse(smoke_client.verify_consistency_proof(1, 3, root1, root3, []))

        # Proof from 1 -> 3: sibling leaves[1], then leaves[2].
        self.assertTrue(
            smoke_client.verify_consistency_proof(1, 3, root1, root3, [leaves[1], leaves[2]])
        )
        # Tampered root fails.
        tampered = bytes([root3[0] ^ 0xFF]) + root3[1:]
        self.assertFalse(
            smoke_client.verify_consistency_proof(1, 3, root1, tampered, [leaves[1], leaves[2]])
        )
        # Proof from 3 -> 4 (non-power-of-two first_size): [leaves[2], leaves[3], root2].
        self.assertTrue(
            smoke_client.verify_consistency_proof(
                3, 4, root3, root4, [leaves[2], leaves[3], root2]
            )
        )
        # Extra proof element fails sn == 0 check.
        self.assertFalse(
            smoke_client.verify_consistency_proof(
                1, 3, root1, root3, [leaves[1], leaves[2], leaves[3]]
            )
        )

    def test_leaf_hash_requires_32_byte_digest(self) -> None:
        with self.assertRaises(ValueError):
            smoke_client.ciphertext_leaf_hash(b"short")


class TransportSecurityTests(unittest.TestCase):
    def test_refuses_redirects(self) -> None:
        class RedirectHandler(BaseHTTPRequestHandler):
            def do_GET(self) -> None:
                self.send_response(302)
                self.send_header("Location", "http://127.0.0.1:9/elsewhere")
                self.send_header("Content-Type", "application/json")
                body = json.dumps({"redirect": True}).encode("utf-8")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, fmt: str, *args: object) -> None:
                return

        with _MockServer(RedirectHandler) as srv:
            res = smoke_client.http_json_request(srv.origin, "GET", "/mls/v1/info")
            self.assertEqual(res.status, 302)

    def test_caps_oversized_response_body(self) -> None:
        class OversizedHandler(BaseHTTPRequestHandler):
            def do_GET(self) -> None:
                body = b'{"data":"' + (b"x" * 2048) + b'"}'
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, fmt: str, *args: object) -> None:
                return

        with _MockServer(OversizedHandler) as srv:
            with self.assertRaises(smoke_client.SmokeClientError) as ctx:
                smoke_client.http_json_request(
                    srv.origin,
                    "GET",
                    "/mls/v1/info",
                    max_response_bytes=512,
                )
            self.assertIn("byte cap", str(ctx.exception))

    def test_rejects_invalid_json_and_handles_http_error(self) -> None:
        class ErrorHandler(BaseHTTPRequestHandler):
            def do_GET(self) -> None:
                if self.path == "/bad-json":
                    body = b"not-json"
                    self.send_response(200)
                else:
                    body = json.dumps(
                        {"code": "writes_disabled", "status": 503}
                    ).encode("utf-8")
                    self.send_response(503)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, fmt: str, *args: object) -> None:
                return

        with _MockServer(ErrorHandler) as srv:
            err_res = smoke_client.http_json_request(srv.origin, "GET", "/mls/v1/info")
            self.assertEqual(err_res.status, 503)
            self.assertEqual(err_res.body["code"], "writes_disabled")

            with self.assertRaises(smoke_client.SmokeClientError):
                smoke_client.http_json_request(srv.origin, "GET", "/bad-json")

    def test_cli_fails_cleanly_when_writes_disabled(self) -> None:
        class ClosedRelayHandler(BaseHTTPRequestHandler):
            def do_GET(self) -> None:
                body = json.dumps(
                    {
                        "profile": smoke_client.PROFILE_ID,
                        "writes_enabled": False,
                        "supported_ciphersuites": [smoke_client.PQ_CIPHERSUITE],
                    }
                ).encode("utf-8")
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Cache-Control", "no-store")
                self.send_header("X-Content-Type-Options", "nosniff")
                self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, fmt: str, *args: object) -> None:
                return

        with _MockServer(ClosedRelayHandler) as srv:
            stderr_buf = io.StringIO()
            with contextlib.redirect_stderr(stderr_buf):
                rc = smoke_client.main(["--origin", srv.origin])
            self.assertEqual(rc, 1)
            payload = json.loads(stderr_buf.getvalue())
            self.assertEqual(payload["outcome"], "error")
            self.assertIn("writes_enabled", payload["detail"])


class LiveLoopbackRelayTests(unittest.TestCase):
    def test_full_lifecycle_against_real_agentciv_host_binary(self) -> None:
        binary = _find_agentciv_host_binary()
        if binary is None:
            self.skipTest("compiled agentciv-host binary not found in target/")

        with tempfile.TemporaryDirectory(prefix="agentciv-mls-smoke-") as tmpdir:
            db_path = Path(tmpdir) / "relay.sqlite"
            proc = subprocess.Popen(
                [
                    str(binary),
                    "mls-relay",
                    "--db",
                    str(db_path),
                    "--host",
                    "127.0.0.1",
                    "--port",
                    "0",
                    "--retention-seconds",
                    "3600",
                    "--writes-enabled",
                ],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            try:
                assert proc.stdout is not None
                ready_line = proc.stdout.readline().strip()
                self.assertTrue(ready_line, "expected JSON readiness line from agentciv-host")
                ready: dict[str, Any] = json.loads(ready_line)
                self.assertEqual(ready["outcome"], "serving")
                self.assertEqual(ready["profile"], smoke_client.PROFILE_ID)
                self.assertTrue(ready["writes_enabled"])
                self.assertEqual(ready["retention_seconds"], 3600)
                origin = str(ready["origin"])

                stdout_buf = io.StringIO()
                with contextlib.redirect_stdout(stdout_buf):
                    rc = smoke_client.main(["--origin", origin, "--prefix", "pylive"])
                self.assertEqual(rc, 0)
                summary = json.loads(stdout_buf.getvalue())
                self.assertEqual(summary["outcome"], "ok")
                self.assertEqual(summary["profile"], smoke_client.PROFILE_ID)
                self.assertEqual(summary["tree_size"], 3)
                self.assertTrue(summary["consistency_verified"])
                self.assertEqual(summary["checks_passed"], 11)
            finally:
                proc.terminate()
                try:
                    proc.wait(timeout=5.0)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait(timeout=5.0)
                if proc.stdout is not None:
                    proc.stdout.close()
                if proc.stderr is not None:
                    proc.stderr.close()


if __name__ == "__main__":
    unittest.main()
