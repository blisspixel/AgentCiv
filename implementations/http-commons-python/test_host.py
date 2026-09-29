"""Behavior tests for the Python HTTP Commons host.

These tests speak HTTP or call the store directly. They do not import the
Rust host. The optional public-runner test at the bottom uses the existing
conformance command as a black box.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import threading
import unittest
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import host  # noqa: E402


def config(directory: Path, **overrides: object) -> host.HostConfig:
    values: dict[str, object] = {
        "world_id": "civ:local",
        "title": "Local",
        "database_path": directory / "world.sqlite",
        "listen": ("127.0.0.1", 0),
        "visibility": "members",
        "retention_seconds": 60,
        "max_payload_bytes": 4096,
        "credentials": (
            host.Credential("agent:abc123", "writer-token-value", True, True),
            host.Credential("agent:reader", "reader-token-value", True, False),
            host.Credential("agent:two", "second-writer-token", True, True),
            host.Credential("agent:writer-only", "writer-only-token", False, True),
        ),
    }
    values.update(overrides)
    return host.HostConfig(**values)  # type: ignore[arg-type]


def message(
    message_id: str,
    sender: str,
    world: str = "civ:local",
    text: str = "hello",
    recipients: list[str] | None = None,
    extra: dict | None = None,
) -> bytes:
    record = {
        "protocol_version": "0.1-draft",
        "type": "message",
        "id": message_id,
        "world": world,
        "from": sender,
        "to": recipients or [sender],
        "body": {"text": text},
    }
    if extra:
        record.update(extra)
    return json.dumps(record).encode("utf-8")


def request(
    method: str,
    url: str,
    token: str | None = None,
    body: bytes | None = None,
    content_type: str | None = "application/json",
) -> tuple[int, dict[str, str], bytes]:
    req = urllib.request.Request(url, data=body, method=method)
    req.add_header("Accept", "application/json")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    if body is not None and content_type is not None:
        req.add_header("Content-Type", content_type)
    try:
        with urllib.request.urlopen(req, timeout=5) as response:
            status = response.status
            headers = {key.lower(): value for key, value in response.headers.items()}
            payload = response.read()
    except urllib.error.HTTPError as error:
        try:
            status = error.code
            headers = {key.lower(): value for key, value in error.headers.items()}
            payload = error.read()
        finally:
            error.close()
    return status, headers, payload


def json_body(payload: bytes) -> dict:
    value = json.loads(payload)
    if not isinstance(value, dict):
        raise AssertionError("expected a JSON object")
    return value


class ConfigTests(unittest.TestCase):
    def test_file_round_trip_and_rejections(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            database = directory / "world.sqlite"
            path = directory / "host.json"
            path.write_text(
                json.dumps(
                    {
                        "world_id": "civ:local",
                        "title": "Local",
                        "database_path": str(database),
                        "listen": "127.0.0.1:0",
                        "visibility": "addressed",
                        "retention_seconds": 60,
                        "max_payload_bytes": 1024,
                        "credentials": [
                            {
                                "principal": "agent:abc123",
                                "token": "writer-token-value",
                                "read": True,
                                "write": True,
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )
            loaded = host.load_config(path)
            self.assertEqual(loaded.visibility, "addressed")
            self.assertEqual(loaded.listen, ("127.0.0.1", 0))
            path.write_text(path.read_text(encoding="utf-8").replace("127.0.0.1", "0.0.0.0"), encoding="utf-8")
            with self.assertRaises(host.ConfigError):
                host.load_config(path)

    def test_checked_config_rejects_weak_setup(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            good = config(directory)
            host.checked_config(good)
            with self.assertRaises(host.ConfigError):
                host.checked_config(config(directory, listen=("0.0.0.0", 9)))
            with self.assertRaises(host.ConfigError):
                host.checked_config(config(directory, retention_seconds=0))
            with self.assertRaises(host.ConfigError):
                host.checked_config(config(directory, max_payload_bytes=1023))
            with self.assertRaises(host.ConfigError):
                host.checked_config(config(directory, visibility="public"))
            with self.assertRaises(host.ConfigError):
                host.checked_config(config(directory, credentials=()))
            duplicated = (
                host.Credential("agent:abc123", "one", True, True),
                host.Credential("agent:abc123", "two", True, True),
            )
            with self.assertRaises(host.ConfigError):
                host.checked_config(config(directory, credentials=duplicated))


class RecordTests(unittest.TestCase):
    def test_version_and_type_precede_other_schema_failures(self) -> None:
        self.assertEqual(host.message_error({"protocol_version": "9"}), "unsupported_version")
        self.assertEqual(
            host.message_error({"protocol_version": "0.1-draft", "type": "action"}),
            "unsupported_record_type",
        )
        self.assertEqual(host.message_error({"type": "message"}), "invalid_record")
        self.assertIsNone(
            host.message_error(json.loads(message("message:1", "agent:abc123", extra={"note": True})))
        )


class StoreTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.directory = Path(self.temporary.name)
        self.config = config(self.directory, retention_seconds=10)
        self.store = host.Store(self.config)
        self.store.clock = lambda: 1_000

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def test_database_keeps_the_world_it_was_opened_for(self) -> None:
        body = message("message:kept", "agent:abc123", text="stay")
        self.store.submit("agent:abc123", body)
        self.assertEqual(self.store.event_count(), 1)
        with self.assertRaises(host.ConfigError) as caught:
            host.Store(config(self.directory, world_id="civ:other", retention_seconds=10))
        self.assertIn("different world", str(caught.exception))
        reopened = host.Store(self.config)
        self.assertEqual(reopened.event_count(), 1)
        page = reopened.read_page("agent:abc123", None, True)
        self.assertEqual(page["events"][0]["body"]["message"]["id"], "message:kept")

    def test_retry_window_is_half_open_and_keeps_the_old_event(self) -> None:
        body = message("message:1", "agent:abc123", text="first")
        receipt = self.store.submit("agent:abc123", body)
        self.store.clock = lambda: 1_009
        self.assertEqual(self.store.submit("agent:abc123", body)["event_id"], receipt["event_id"])
        with self.assertRaises(host.Conflict):
            self.store.submit("agent:abc123", message("message:1", "agent:abc123", text="other"))
        self.assertEqual(self.store.event_count(), 1)
        self.store.clock = lambda: 1_010
        again = self.store.submit("agent:abc123", message("message:1", "agent:abc123", text="later"))
        self.assertNotEqual(again["event_id"], receipt["event_id"])
        self.assertEqual(self.store.event_count(), 2)
        page = self.store.read_page("agent:abc123", None, True)
        texts = [event["body"]["message"]["body"]["text"] for event in page["events"]]
        self.assertEqual(texts, ["first", "later"])

    def test_retry_scope_is_per_principal(self) -> None:
        body = message("message:same", "agent:abc123")
        other = message("message:same", "agent:two")
        self.store.submit("agent:abc123", body)
        self.store.submit("agent:two", other)
        self.assertEqual(self.store.event_count(), 2)

    def test_visibility_and_cursor_revision(self) -> None:
        self.store.submit(
            "agent:abc123",
            message("message:to-reader", "agent:abc123", recipients=["agent:reader"], text="seen"),
        )
        self.store.submit(
            "agent:abc123",
            message("message:private", "agent:abc123", recipients=["agent:abc123"], text="hidden"),
        )
        addressed = host.Store(config(self.directory, visibility="addressed", retention_seconds=10))
        reader = addressed.read_page("agent:reader", None, True)
        self.assertEqual(
            [event["body"]["message"]["id"] for event in reader["events"]],
            ["message:to-reader"],
        )
        sender = addressed.read_page("agent:abc123", None, True)
        self.assertEqual(len(sender["events"]), 2)
        stale = reader["next_cursor"]
        sender_only = host.Store(config(self.directory, visibility="sender_only", retention_seconds=10))
        with self.assertRaises(host.CursorExpired):
            sender_only.read_page("agent:reader", stale, True)
        hidden = sender_only.read_page("agent:reader", None, True)
        self.assertEqual(hidden["events"], [])
        own = sender_only.read_page("agent:abc123", None, True)
        self.assertEqual(len(own["events"]), 2)
        with self.assertRaises(host.ReadForbidden):
            sender_only.read_page("agent:writer-only", "not-a-cursor", False)
        with self.assertRaises(host.InvalidCursor):
            sender_only.read_page("agent:abc123", "not-a-cursor", True)
        with self.assertRaises(host.InvalidCursor):
            sender_only.read_page("agent:abc123", "", True)
        with self.assertRaises(host.ReadForbidden):
            sender_only.read_page("agent:reader", sender["next_cursor"], True)


class HttpTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.directory = Path(self.temporary.name)
        self.servers: list[host.CommonsServer] = []

    def tearDown(self) -> None:
        for server in self.servers:
            server.shutdown()
            server.server_close()
        self.temporary.cleanup()

    def start(self, **overrides: object) -> host.CommonsServer:
        server = host.start_server(config(self.directory, **overrides))
        self.servers.append(server)
        return server

    def test_discovery_auth_recording_and_retry(self) -> None:
        server = self.start()
        status, headers, payload = request("GET", f"{server.origin}/.well-known/agentciv")
        self.assertEqual(status, 200)
        world = json_body(payload)
        self.assertEqual(world["profile"], "http-commons/0.1-draft")
        self.assertEqual(world["endpoints"]["events"], f"{server.origin}/events")
        self.assertEqual(world["endpoints"]["submit"], f"{server.origin}/submit")
        self.assertEqual(headers["content-type"], "application/json")

        status, headers, payload = request("GET", f"{server.origin}/events")
        self.assertEqual(status, 401)
        self.assertEqual(headers["content-type"], "application/problem+json")
        self.assertIn("no-store", headers["cache-control"])
        self.assertTrue(headers["www-authenticate"].lower().startswith("bearer"))
        self.assertEqual(json_body(payload)["code"], "authentication_required")

        status, _, payload = request(
            "POST",
            f"{server.origin}/submit",
            body=message("message:nope", "agent:abc123"),
        )
        self.assertEqual(status, 401)
        self.assertEqual(json_body(payload)["status"], 401)

        status, headers, payload = request("GET", f"{server.origin}/events", token="reader-token-value")
        self.assertEqual(status, 200)
        self.assertIn("no-store", headers["cache-control"])
        self.assertEqual(json_body(payload)["events"], [])

        status, _, payload = request("GET", f"{server.origin}/events", token="writer-token-value")
        self.assertEqual(status, 200)
        page = json_body(payload)
        self.assertEqual(page["events"], [])
        cursor = page["next_cursor"]
        self.assertTrue(cursor)
        self.assertFalse(cursor.isdigit())

        body = message(
            "message:conformance-roundtrip",
            "agent:abc123",
            text="conformance roundtrip",
            extra={"conformance_probe": {"preserve": True}},
        )
        status, headers, payload = request(
            "POST", f"{server.origin}/submit", token="writer-token-value", body=body
        )
        self.assertEqual(status, 200)
        self.assertIn("no-store", headers["cache-control"])
        receipt = json_body(payload)
        self.assertEqual(receipt["status"], "recorded")
        self.assertEqual(receipt["record_id"], "message:conformance-roundtrip")

        status, _, payload = request(
            "POST", f"{server.origin}/submit", token="writer-token-value", body=body
        )
        self.assertEqual(json_body(payload), receipt)

        changed = message("message:conformance-roundtrip", "agent:abc123", text="different bytes")
        status, _, payload = request(
            "POST", f"{server.origin}/submit", token="writer-token-value", body=changed
        )
        self.assertEqual(status, 409)
        self.assertEqual(json_body(payload)["code"], "id_conflict")

        quoted = urllib.parse.quote(cursor, safe="")
        status, _, payload = request(
            "GET", f"{server.origin}/events?after={quoted}", token="writer-token-value"
        )
        page = json_body(payload)
        self.assertEqual(status, 200)
        self.assertEqual(len(page["events"]), 1)
        event = page["events"][0]
        self.assertEqual(event["id"], receipt["event_id"])
        self.assertEqual(event["sequence"], receipt["sequence"])
        self.assertEqual(event["kind"], "message.recorded")
        self.assertEqual(event["body"]["message"]["conformance_probe"], {"preserve": True})
        self.assertNotEqual(page["next_cursor"], str(event["sequence"]))

    def test_refusal_record_errors_and_cursors(self) -> None:
        server = self.start(max_payload_bytes=1024)
        submit = f"{server.origin}/submit"
        events = f"{server.origin}/events"
        writer = "writer-token-value"
        reader = "reader-token-value"

        status, _, payload = request(
            "POST",
            submit,
            token=reader,
            body=message("message:denied", "agent:reader"),
        )
        self.assertEqual(status, 403)
        self.assertEqual(json_body(payload)["code"], "forbidden")

        status, _, payload = request(
            "POST",
            submit,
            token=writer,
            body=message("message:version", "agent:abc123").replace(
                b'"0.1-draft"', b'"9"', 1
            ),
        )
        self.assertEqual(status, 422)
        self.assertEqual(json_body(payload)["code"], "unsupported_version")

        status, _, payload = request("POST", submit, token=writer, body=b"{")
        self.assertEqual(status, 400)
        self.assertEqual(json_body(payload)["code"], "malformed_json")

        status, _, payload = request(
            "POST",
            submit,
            token=writer,
            body=message("message:elsewhere", "agent:abc123", world="civ:elsewhere"),
        )
        self.assertEqual(status, 422)
        self.assertEqual(json_body(payload)["code"], "wrong_world")

        status, _, payload = request(
            "POST",
            submit,
            token=writer,
            body=message("message:mismatch", "agent:reader"),
        )
        self.assertEqual(status, 403)
        self.assertEqual(json_body(payload)["code"], "forbidden")

        action = json.loads(message("message:action", "agent:abc123"))
        action["type"] = "action"
        status, _, payload = request(
            "POST", submit, token=writer, body=json.dumps(action).encode("utf-8")
        )
        self.assertEqual(status, 422)
        self.assertEqual(json_body(payload)["code"], "unsupported_record_type")

        status, _, payload = request(
            "POST", submit, token=writer, body=b"hello", content_type="text/plain"
        )
        self.assertEqual(status, 415)
        self.assertEqual(json_body(payload)["code"], "unsupported_media_type")

        status, _, payload = request("POST", submit, token=writer, body=b"x" * 1025)
        self.assertEqual(status, 413)
        self.assertEqual(json_body(payload)["code"], "payload_too_large")

        status, _, payload = request("GET", f"{events}?after=not-a-cursor", token=writer)
        self.assertEqual(status, 400)
        self.assertEqual(json_body(payload)["code"], "invalid_cursor")

        status, _, payload = request(
            "GET", f"{events}?after=not-a-cursor", token="writer-only-token"
        )
        self.assertEqual(status, 403)
        self.assertEqual(json_body(payload)["code"], "forbidden")

        request(
            "POST",
            submit,
            token=writer,
            body=message("message:conformance-roundtrip", "agent:abc123", text="visible"),
        )
        status, _, payload = request("GET", events, token=writer)
        cursor = json_body(payload)["next_cursor"]
        status, _, payload = request(
            "GET",
            f"{events}?after={urllib.parse.quote(cursor, safe='')}",
            token=reader,
        )
        self.assertEqual(status, 403)
        self.assertEqual(json_body(payload)["code"], "forbidden")

        status, _, payload = request("GET", events, token=reader)
        seen = [
            event["body"]["message"]["id"]
            for event in json_body(payload)["events"]
        ]
        self.assertIn("message:conformance-roundtrip", seen)

    def test_sender_only_hides_a_message_from_another_principal(self) -> None:
        server = self.start(visibility="sender_only")
        request(
            "POST",
            f"{server.origin}/submit",
            token="writer-token-value",
            body=message("message:conformance-roundtrip", "agent:abc123"),
        )
        _, _, payload = request("GET", f"{server.origin}/events", token="reader-token-value")
        self.assertEqual(json_body(payload)["events"], [])

    def test_pagination_does_not_reveal_sequence_in_the_cursor(self) -> None:
        server = self.start()
        for index in range(101):
            status, _, _ = request(
                "POST",
                f"{server.origin}/submit",
                token="writer-token-value",
                body=message(f"message:page-{index}", "agent:abc123", text="page"),
            )
            self.assertEqual(status, 200)
        _, _, payload = request("GET", f"{server.origin}/events", token="writer-token-value")
        first = json_body(payload)
        self.assertEqual(len(first["events"]), 100)
        self.assertIs(first["has_more"], True)
        cursor = first["next_cursor"]
        for event in first["events"]:
            self.assertNotEqual(cursor, str(event["sequence"]))
        _, _, payload = request(
            "GET",
            f"{server.origin}/events?after={urllib.parse.quote(cursor, safe='')}",
            token="writer-token-value",
        )
        second = json_body(payload)
        self.assertEqual(len(second["events"]), 1)
        self.assertIs(second["has_more"], False)
        first_ids = {event["id"] for event in first["events"]}
        self.assertNotIn(second["events"][0]["id"], first_ids)

    def test_restart_handoff_and_cursor_expiry(self) -> None:
        server = self.start(visibility="members", retention_seconds=3600)
        for principal, token, text in (
            ("agent:abc123", "writer-token-value", "from the first writer"),
            ("agent:two", "second-writer-token", "from the second writer"),
        ):
            status, _, _ = request(
                "POST",
                f"{server.origin}/submit",
                token=token,
                body=message(f"message:{principal}", principal, text=text),
            )
            self.assertEqual(status, 200)
        _, _, payload = request("GET", f"{server.origin}/events", token="reader-token-value")
        cursor = json_body(payload)["next_cursor"]
        server.shutdown()
        server.server_close()
        self.servers.remove(server)

        restarted = self.start(visibility="members", retention_seconds=3600)
        _, _, payload = request("GET", f"{restarted.origin}/events", token="reader-token-value")
        texts = [event["body"]["message"]["body"]["text"] for event in json_body(payload)["events"]]
        self.assertEqual(texts, ["from the first writer", "from the second writer"])

        changed = self.start(visibility="sender_only", retention_seconds=3600)
        status, _, payload = request(
            "GET",
            f"{changed.origin}/events?after={urllib.parse.quote(cursor, safe='')}",
            token="reader-token-value",
        )
        self.assertEqual(status, 410)
        self.assertEqual(json_body(payload)["code"], "cursor_expired")

    def test_concurrent_submissions_keep_one_receipt_per_exact_body(self) -> None:
        server = self.start()
        results: list[tuple[int, dict]] = []
        lock = threading.Lock()

        def race(message_id: str, text: str, barrier: threading.Barrier) -> None:
            barrier.wait()
            status, _, payload = request(
                "POST",
                f"{server.origin}/submit",
                token="writer-token-value",
                body=message(message_id, "agent:abc123", text=text),
            )
            with lock:
                results.append((status, json_body(payload)))

        def run_pair(message_id: str, left: str, right: str) -> None:
            results.clear()
            barrier = threading.Barrier(2)
            threads = [
                threading.Thread(target=race, args=(message_id, left, barrier)),
                threading.Thread(target=race, args=(message_id, right, barrier)),
            ]
            for thread in threads:
                thread.start()
            for thread in threads:
                thread.join()

        run_pair("message:exact", "same", "same")
        self.assertEqual(sorted(status for status, _ in results), [200, 200])
        self.assertEqual(results[0][1], results[1][1])
        _, _, payload = request("GET", f"{server.origin}/events", token="writer-token-value")
        self.assertEqual(len(json_body(payload)["events"]), 1)

        run_pair("message:differ", "left", "right")
        self.assertEqual(sorted(status for status, _ in results), [200, 409])


class PublicRunnerTest(unittest.TestCase):
    def test_extended_public_report(self) -> None:
        if os.environ.get("AGENTCIV_SKIP_RUNNER") == "1":
            self.skipTest("public runner skipped")
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            server = host.start_server(
                config(
                    directory,
                    world_id="civ:python-runner",
                    visibility="members",
                    retention_seconds=86400,
                    max_payload_bytes=4096,
                )
            )
            try:
                root = Path(__file__).resolve().parents[2]
                command = runner_command(root, f"{server.origin}/.well-known/agentciv")
                env = os.environ.copy()
                env["AGENTCIV_CONFORMANCE_TOKEN"] = "writer-token-value"
                env["AGENTCIV_CONFORMANCE_READER_TOKEN"] = "reader-token-value"
                completed = subprocess.run(
                    command,
                    cwd=root,
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=180,
                    check=False,
                )
            finally:
                server.shutdown()
                server.server_close()
        self.assertEqual(
            completed.returncode,
            0,
            completed.stdout + "\n" + completed.stderr,
        )
        report = json.loads(completed.stdout)
        self.assertEqual(report["runner_scope"], "credentialed-extended")
        self.assertEqual(report["summary"]["failed"], 0)
        self.assertEqual(report["summary"]["skipped"], 0)
        self.assertGreaterEqual(report["summary"]["passed"], 17)


def runner_command(root: Path, discovery: str) -> list[str]:
    arguments = [
        "--discovery",
        discovery,
        "--principal",
        "agent:abc123",
        "--reader",
        "agent:reader",
    ]
    for name in ("agentciv-conformance.exe", "agentciv-conformance"):
        candidate = root / "target" / "debug" / name
        if candidate.exists():
            return [str(candidate), *arguments]
    return ["cargo", "run", "--locked", "-p", "agentciv-conformance", "--", *arguments]


if __name__ == "__main__":
    unittest.main()
