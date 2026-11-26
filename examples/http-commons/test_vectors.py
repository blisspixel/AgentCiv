"""Exercise language-neutral wire test vectors against the Python HTTP Commons host."""

from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from typing import Any
from urllib.error import HTTPError
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[2]
VECTORS_PATH = ROOT / "conformance" / "vectors" / "wire-vectors.json"
HOST_MODULE_PATH = ROOT / "implementations" / "http-commons-python" / "host.py"


def load_host_module() -> Any:
    spec = importlib.util.spec_from_file_location("agentciv_python_host_vectors", HOST_MODULE_PATH)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def http_request(
    url: str,
    *,
    method: str = "GET",
    token: str | None = None,
    body: bytes | None = None,
) -> tuple[int, dict[str, str], dict[str, Any]]:
    headers: dict[str, str] = {"Accept": "application/json"}
    if token is not None:
        headers["Authorization"] = f"Bearer {token}"
    if body is not None:
        headers["Content-Type"] = "application/json"
    req = Request(url, data=body, headers=headers, method=method)
    try:
        response = urlopen(req, timeout=5)
    except HTTPError as error:
        response = error
    with response:
        raw = response.read(1_000_000)
        parsed: dict[str, Any] = json.loads(raw.decode("utf-8"))
        lower_headers = {key.lower(): value for key, value in response.headers.items()}
        return response.status, lower_headers, parsed


class WireVectorTests(unittest.TestCase):
    def test_wire_vectors_against_python_host(self) -> None:
        vectors: dict[str, Any] = json.loads(VECTORS_PATH.read_text(encoding="utf-8"))
        self.assertEqual(vectors["format"], "agentciv-wire-vectors/0.1")
        self.assertEqual(vectors["profile"], "http-commons/0.1-draft")
        world_id: str = vectors["world"]
        principal: str = vectors["principal"]
        recipient: str = vectors["recipient"]

        host = load_host_module()
        with tempfile.TemporaryDirectory(prefix="agentciv-vectors-") as temp_dir:
            config = host.HostConfig(
                world_id=world_id,
                title="Wire Vectors",
                database_path=Path(temp_dir) / "world.sqlite",
                listen=("127.0.0.1", 0),
                visibility="members",
                retention_seconds=300,
                max_payload_bytes=8192,
                credentials=(
                    host.Credential(principal=principal, token="token-writer", read=True, write=True),
                    host.Credential(principal=recipient, token="token-reader", read=True, write=False),
                ),
            )
            server = host.start_server(config)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                origin = server.origin

                expected_by_record_id: dict[str, str] = {}
                recorded_event_ids: dict[str, tuple[str, int]] = {}

                for case in vectors["positive"]:
                    with self.subTest(positive=case["id"]):
                        record: dict[str, Any] = case["record"]
                        raw = json.dumps(record, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
                        status, headers, receipt = http_request(
                            f"{origin}/submit",
                            method="POST",
                            token="token-writer",
                            body=raw,
                        )
                        self.assertEqual(status, 200, receipt)
                        self.assertEqual(receipt["status"], "recorded")
                        self.assertEqual(receipt["record_id"], record["id"])
                        self.assertEqual(headers.get("cache-control"), "no-store")
                        for forbidden in case.get("forbidden_response_headers", []):
                            self.assertNotIn(forbidden.lower(), headers)

                        # Exact-byte idempotent retry returns identical event_id and sequence.
                        retry_status, _, retry_receipt = http_request(
                            f"{origin}/submit",
                            method="POST",
                            token="token-writer",
                            body=raw,
                        )
                        self.assertEqual(retry_status, 200)
                        self.assertEqual(retry_receipt["event_id"], receipt["event_id"])
                        self.assertEqual(retry_receipt["sequence"], receipt["sequence"])

                        # HTML-escaped re-encoding of the same record ID must fail with 409 id_conflict.
                        if case.get("check_escaped_conflict"):
                            escaped = raw.replace(b"<", b"\\u003c").replace(b">", b"\\u003e").replace(b"&", b"\\u0026")
                            self.assertNotEqual(escaped, raw)
                            conflict_status, _, conflict_body = http_request(
                                f"{origin}/submit",
                                method="POST",
                                token="token-writer",
                                body=escaped,
                            )
                            self.assertEqual(conflict_status, 409)
                            self.assertEqual(conflict_body["code"], "id_conflict")

                        expected_by_record_id[record["id"]] = case["expected_body_text"]
                        recorded_event_ids[record["id"]] = (receipt["event_id"], receipt["sequence"])

                for case in vectors["negative"]:
                    with self.subTest(negative=case["id"]):
                        endpoint: str = case["endpoint"]
                        raw = case["raw_json"].encode("utf-8")
                        status, headers, problem = http_request(
                            f"{origin}{endpoint}",
                            method="POST",
                            token="token-writer",
                            body=raw,
                        )
                        self.assertEqual(status, case["expected_status"], problem)
                        self.assertIn("application/problem+json", headers.get("content-type", ""))
                        self.assertEqual(problem["code"], case["expected_code"])

                # Read back events and verify strictly opaque round-trip for every positive vector.
                status, headers, page = http_request(
                    f"{origin}/events",
                    method="GET",
                    token="token-reader",
                )
                self.assertEqual(status, 200)
                self.assertEqual(headers.get("cache-control"), "no-store")
                self.assertNotIn("x-injected-header", headers)
                self.assertNotIn("set-cookie", headers)
                events: list[dict[str, Any]] = page["events"]
                self.assertEqual(len(events), len(vectors["positive"]))
                for event in events:
                    self.assertEqual(event["kind"], "message.recorded")
                    message = event["body"]["message"]
                    record_id = message["id"]
                    self.assertIn(record_id, expected_by_record_id)
                    self.assertEqual(message["from"], principal)
                    self.assertEqual(message["world"], world_id)
                    self.assertEqual(message["body"]["text"], expected_by_record_id[record_id])
                    expected_event_id, expected_sequence = recorded_event_ids[record_id]
                    self.assertEqual(event["id"], expected_event_id)
                    self.assertEqual(event["sequence"], expected_sequence)
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=5)


if __name__ == "__main__":
    unittest.main()
