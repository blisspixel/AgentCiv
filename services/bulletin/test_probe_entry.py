"""Real stdlib HTTP fixtures for public entry access and refusal retention."""
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

from services.bulletin import probe_entry


class ProbeTests(unittest.TestCase):
    def setUp(self) -> None:
        self.requests: list[tuple[str, str, str | None]] = []
        self.responses: dict[str, tuple[int, bytes, dict[str, str]]] = {
            "/agent.json": (200, b'{"service":"agentciv.io/0.1-experimental"}', {}),
            "/llms.txt": (200, b"# AgentCiv service instructions\nWelcome.\n", {}),
            "/api/board/info": (200, b'{"service":"web-bulletin/0.1-experimental"}', {}),
        }
        self.block_default = False
        self.malformed_response: bytes | None = None
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self) -> None:
                agent = self.headers.get("User-Agent", "")
                owner.requests.append((self.path, agent, self.headers.get("Authorization")))
                if owner.malformed_response is not None:
                    self.connection.sendall(owner.malformed_response)
                    return
                if owner.block_default and agent.startswith("Python-urllib/"):
                    status, body, headers = 403, b"error code: 1010", {"Server": "cloudflare", "CF-Ray": "fixture"}
                else:
                    status, body, headers = owner.responses[self.path]
                self.send_response_only(status)
                for key, value in headers.items():
                    self.send_header(key, value)
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, format: str, *args: object) -> None:
                pass

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = Thread(target=lambda: self.server.serve_forever(poll_interval=0.01), daemon=True)
        self.thread.start()
        self.origin = f"http://127.0.0.1:{self.server.server_port}"
        self.addCleanup(self.server.server_close)
        self.addCleanup(self.thread.join)
        self.addCleanup(self.server.shutdown)

    def test_default_and_truthful_clients_send_six_unauthenticated_gets(self) -> None:
        results = probe_entry.probe(self.origin + "/")
        self.assertEqual(len(results), 6)
        self.assertTrue(all(result.passed for result in results))
        self.assertEqual([request[0] for request in self.requests], list(probe_entry.PATHS) * 2)
        default_agent = dict(build_opener().addheaders)["User-agent"]
        self.assertTrue(default_agent.startswith("Python-urllib/"))
        self.assertEqual([request[1] for request in self.requests], [default_agent] * 3 + [probe_entry.CONTROL_AGENT] * 3)
        self.assertTrue(all(request[2] is None for request in self.requests))
        self.assertEqual([result.user_agent for result in results], [request[1] for request in self.requests])

    def test_1010_default_refusal_is_preserved_even_when_control_passes(self) -> None:
        self.block_default = True
        results = probe_entry.probe(self.origin)
        self.assertEqual([result.status for result in results], [403] * 3 + [200] * 3)
        self.assertEqual([result.passed for result in results], [False] * 3 + [True] * 3)
        for result in results[:3]:
            self.assertEqual(result.body_text, "error code: 1010")
            self.assertEqual(result.headers, {"Server": "cloudflare", "CF-Ray": "fixture"})
            self.assertIsNotNone(result.body_sha256)

    def test_redirect_is_retained_without_following_external_location(self) -> None:
        self.responses["/agent.json"] = (302, b"moved", {"Location": "https://example.invalid/secret"})
        results = probe_entry.probe(self.origin)
        self.assertEqual([result.status for result in results[::3]], [302, 302])
        self.assertFalse(results[0].passed)
        self.assertEqual(len(self.requests), 6)
        self.assertNotIn("secret", json.dumps([result.headers for result in results]))

    def test_overlong_body_is_capped_and_fails(self) -> None:
        self.responses["/llms.txt"] = (200, b"x" * (probe_entry.MAX_BYTES + 20), {})
        results = probe_entry.probe(self.origin)
        for result in (results[1], results[4]):
            self.assertEqual(result.bytes_read, probe_entry.MAX_BYTES + 1)
            self.assertTrue(result.truncated)
            self.assertFalse(result.passed)

    def test_wrong_documents_invalid_utf8_and_html_do_not_pass(self) -> None:
        self.responses["/agent.json"] = (200, b'{"service":"wrong"}', {})
        self.responses["/llms.txt"] = (200, b"\xff", {})
        self.responses["/api/board/info"] = (200, b"<html>challenge</html>", {})
        self.assertFalse(any(result.passed for result in probe_entry.probe(self.origin)))
        self.assertTrue(probe_entry.expected_body("/llms.txt", b"# AgentCiv service instructions\r\n"))
        self.assertFalse(probe_entry.expected_body("/agent.json", b"[]"))

    def test_transport_failure_retained_and_never_retried(self) -> None:
        with patch("urllib.request.OpenerDirector.open", side_effect=TimeoutError("fixture")) as opening:
            results = probe_entry.probe(self.origin)
        self.assertEqual(opening.call_count, 6)
        self.assertTrue(all(result.status is None and result.error == "TimeoutError" and not result.passed for result in results))

    def test_malformed_http_retains_all_six_failures_in_complete_output(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            for wire, diagnostic in ((b"invalid-http-status\r\n\r\n", "BadStatusLine"),
                                     (b"HTTP/1.0 " + b"x" * 65_537 + b"\r\n\r\n", "LineTooLong")):
                with self.subTest(diagnostic=diagnostic):
                    self.requests.clear()
                    self.malformed_response = wire
                    output = Path(temporary) / (diagnostic + ".json")
                    self.assertEqual(probe_entry.main(["--origin", self.origin, "--output", str(output)]), 1)
                    document = json.loads(output.read_bytes())
                    self.assertFalse(document["passed"])
                    self.assertEqual(len(self.requests), 6)
                    self.assertEqual(len(document["observations"]), 6)
                    for observation in document["observations"]:
                        self.assertEqual(observation["error"], diagnostic)
                        self.assertIsNone(observation["status"])
                        self.assertFalse(observation["passed"])

    def test_cli_fresh_output_success_and_refusal_exit_status(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "probe.json"
            self.assertEqual(probe_entry.main(["--origin", self.origin, "--output", str(output)]), 0)
            document = json.loads(output.read_bytes())
            self.assertTrue(document["passed"])
            self.assertEqual(len(document["observations"]), 6)
            with self.assertRaises(FileExistsError):
                probe_entry.main(["--origin", self.origin, "--output", str(output)])
            self.assertEqual(len(self.requests), 6)
            self.block_default = True
            output = Path(temporary) / "refused.json"
            self.assertEqual(probe_entry.main(["--origin", self.origin, "--output", str(output)]), 1)
            self.assertFalse(json.loads(output.read_bytes())["passed"])

    def test_invalid_origins_fail_before_network_or_output(self) -> None:
        for value in ("http://agentciv.io", "https://a.invalid/path", "https://a.invalid?x=y", "https://a.invalid/#x", "https://u:p@a.invalid", "https://a.invalid:99999", "https://", "ftp://a.invalid"):
            with self.subTest(value=value), self.assertRaises(ValueError):
                probe_entry.origin_value(value)
        with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            probe_entry.main([])


if __name__ == "__main__":
    unittest.main()
