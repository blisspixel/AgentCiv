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
from unittest import mock
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Callable, Mapping
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import host  # noqa: E402


def config(
    directory: Path,
    *,
    world_id: str = "civ:local",
    title: str = "Local",
    listen: tuple[str, int] = ("127.0.0.1", 0),
    visibility: str = "members",
    retention_seconds: int = 60,
    max_payload_bytes: int = 4096,
    credentials: tuple[host.Credential, ...] | None = None,
) -> host.HostConfig:
    if credentials is None:
        credentials = (
            host.Credential("agent:abc123", "writer-token-value", True, True),
            host.Credential("agent:reader", "reader-token-value", True, False),
            host.Credential("agent:two", "second-writer-token", True, True),
            host.Credential("agent:writer-only", "writer-only-token", False, True),
        )
    return host.HostConfig(
        world_id=world_id,
        title=title,
        database_path=directory / "world.sqlite",
        listen=listen,
        visibility=visibility,
        retention_seconds=retention_seconds,
        max_payload_bytes=max_payload_bytes,
        credentials=credentials,
    )


def frozen_clock(moment: int) -> Callable[[], int]:
    def current() -> int:
        return moment

    return current


def message(
    message_id: str,
    sender: str,
    world: str = "civ:local",
    text: str = "hello",
    recipients: list[str] | None = None,
    extra: Mapping[str, object] | None = None,
) -> bytes:
    record: dict[str, object] = {
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


def as_dict(value: object) -> dict[str, object] | None:
    if not isinstance(value, dict):
        return None
    parsed: dict[str, object] = {}
    for key, item in value.items():
        if not isinstance(key, str):
            return None
        parsed[key] = item
    return parsed


def json_body(payload: bytes) -> dict[str, object]:
    try:
        value: object = json.loads(payload)
    except json.JSONDecodeError as error:
        raise AssertionError("response was not JSON") from error
    found = as_dict(value)
    if found is None:
        raise AssertionError("expected a JSON object")
    return found


def expect_dict(value: object) -> dict[str, object]:
    found = as_dict(value)
    if found is None:
        raise AssertionError("expected a JSON object")
    return found


def expect_list(value: object) -> list[object]:
    if not isinstance(value, list):
        raise AssertionError("expected a JSON array")
    return list(value)


def expect_str(value: object) -> str:
    if not isinstance(value, str):
        raise AssertionError("expected a string")
    return value


def at(value: object, *steps: str | int) -> object:
    current = value
    for step in steps:
        if isinstance(step, str):
            mapping = expect_dict(current)
            if step not in mapping:
                raise AssertionError(f"missing {step}")
            current = mapping[step]
            continue
        if isinstance(step, bool):
            raise AssertionError("path step was not a key or index")
        items = expect_list(current)
        if step < 0 or step >= len(items):
            raise AssertionError(f"missing index {step}")
        current = items[step]
    return current


def dicts(value: object) -> list[dict[str, object]]:
    return [expect_dict(item) for item in expect_list(value)]


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

    def test_bytes_listen_address_formats_as_an_origin(self) -> None:
        self.assertEqual(host.http_origin((b"127.0.0.1", 9)), "http://127.0.0.1:9")
        self.assertEqual(host.http_origin((bytearray(b"127.0.0.1"), 9)), "http://127.0.0.1:9")
        self.assertEqual(host.http_origin(("::1", 9)), "http://[::1]:9")
        with self.assertRaises(host.StorageFailure):
            host.http_origin((b"\xff", 9))
        with self.assertRaises(host.StorageFailure):
            host.http_origin(("127.0.0.1", True))
        with self.assertRaises(host.StorageFailure):
            host.http_origin(("127.0.0.1",))


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

    def test_collaboration_shape_checks_version_and_type_first(self) -> None:
        self.assertEqual(host.collaboration_error({"protocol_version": "9"}), "unsupported_version")
        self.assertEqual(
            host.collaboration_error({"protocol_version": "0.1-draft", "type": "message"}),
            "unsupported_record_type",
        )
        self.assertEqual(host.collaboration_error({"type": "withdrawal"}), "invalid_record")
        self.assertEqual(
            host.collaboration_error({"protocol_version": 1, "type": "objection"}),
            "invalid_record",
        )
        good: dict[str, object] = {
            "protocol_version": "0.1-draft",
            "type": "artifact_revision",
            "id": "submission:1",
            "artifact_id": "artifact:plan",
            "world": "civ:local",
            "from": "agent:abc123",
            "to": ["agent:peer"],
            "media_type": "text/plain",
            "body": {"text": "plan"},
        }
        self.assertIsNone(host.collaboration_error(good))
        chosen = dict(good)
        chosen["revision"] = 7
        self.assertEqual(host.collaboration_error(chosen), "invalid_record")
        missing_note = dict(good)
        missing_note["continuity_note"] = None
        self.assertEqual(host.collaboration_error(missing_note), "invalid_record")
        empty_aim = dict(good)
        empty_aim["continuity_note"] = {"aim": "", "resume_hint": "later"}
        self.assertEqual(host.collaboration_error(empty_aim), "invalid_record")


class StoreTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.directory = Path(self.temporary.name)
        self.config = config(self.directory, retention_seconds=10)
        self.store = host.Store(self.config)
        self.store.clock = frozen_clock(1_000)

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
        self.assertEqual(at(page, "events", 0, "body", "message", "id"), "message:kept")

    def test_retry_window_is_half_open_and_keeps_the_old_event(self) -> None:
        body = message("message:1", "agent:abc123", text="first")
        receipt = self.store.submit("agent:abc123", body)
        self.store.clock = frozen_clock(1_009)
        self.assertEqual(self.store.submit("agent:abc123", body)["event_id"], receipt["event_id"])
        with self.assertRaises(host.Conflict):
            self.store.submit("agent:abc123", message("message:1", "agent:abc123", text="other"))
        self.assertEqual(self.store.event_count(), 1)
        self.store.clock = frozen_clock(1_010)
        again = self.store.submit("agent:abc123", message("message:1", "agent:abc123", text="later"))
        self.assertNotEqual(again["event_id"], receipt["event_id"])
        self.assertEqual(self.store.event_count(), 2)
        page = self.store.read_page("agent:abc123", None, True)
        texts = [at(event, "body", "message", "body", "text") for event in dicts(page["events"])]
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
            [at(event, "body", "message", "id") for event in dicts(reader["events"])],
            ["message:to-reader"],
        )
        sender = addressed.read_page("agent:abc123", None, True)
        self.assertEqual(len(expect_list(sender["events"])), 2)
        stale = expect_str(reader["next_cursor"])
        sender_only = host.Store(config(self.directory, visibility="sender_only", retention_seconds=10))
        with self.assertRaises(host.CursorExpired):
            sender_only.read_page("agent:reader", stale, True)
        hidden = sender_only.read_page("agent:reader", None, True)
        self.assertEqual(hidden["events"], [])
        own = sender_only.read_page("agent:abc123", None, True)
        self.assertEqual(len(expect_list(own["events"])), 2)
        with self.assertRaises(host.ReadForbidden):
            sender_only.read_page("agent:writer-only", "not-a-cursor", False)
        with self.assertRaises(host.InvalidCursor):
            sender_only.read_page("agent:abc123", "not-a-cursor", True)
        with self.assertRaises(host.InvalidCursor):
            sender_only.read_page("agent:abc123", "", True)
        with self.assertRaises(host.ReadForbidden):
            sender_only.read_page("agent:reader", expect_str(sender["next_cursor"]), True)


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

    def start(
        self,
        *,
        world_id: str = "civ:local",
        visibility: str = "members",
        retention_seconds: int = 60,
        max_payload_bytes: int = 4096,
        credentials: tuple[host.Credential, ...] | None = None,
    ) -> host.CommonsServer:
        server = host.start_server(
            config(
                self.directory,
                world_id=world_id,
                visibility=visibility,
                retention_seconds=retention_seconds,
                max_payload_bytes=max_payload_bytes,
                credentials=credentials,
            )
        )
        self.servers.append(server)
        return server

    def test_startup_does_not_reverse_resolve_the_listen_address(self) -> None:
        with mock.patch("socket.getfqdn", side_effect=AssertionError("dns")):
            server = self.start()
        status, _, _ = request("GET", f"{server.origin}/.well-known/agentciv")
        self.assertEqual(status, 200)

    def test_discovery_auth_recording_and_retry(self) -> None:
        server = self.start()
        status, headers, payload = request("GET", f"{server.origin}/.well-known/agentciv")
        self.assertEqual(status, 200)
        world = json_body(payload)
        self.assertEqual(world["profile"], "http-commons/0.1-draft")
        self.assertEqual(at(world, "endpoints", "events"), f"{server.origin}/events")
        self.assertEqual(at(world, "endpoints", "submit"), f"{server.origin}/submit")
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
        cursor = expect_str(page["next_cursor"])
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
        self.assertEqual(len(expect_list(page["events"])), 1)
        event = expect_dict(at(page, "events", 0))
        self.assertEqual(event["id"], receipt["event_id"])
        self.assertEqual(event["sequence"], receipt["sequence"])
        self.assertEqual(event["kind"], "message.recorded")
        self.assertEqual(at(event, "body", "message", "conformance_probe"), {"preserve": True})
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
        cursor = expect_str(json_body(payload)["next_cursor"])
        status, _, payload = request(
            "GET",
            f"{events}?after={urllib.parse.quote(cursor, safe='')}",
            token=reader,
        )
        self.assertEqual(status, 403)
        self.assertEqual(json_body(payload)["code"], "forbidden")

        status, _, payload = request("GET", events, token=reader)
        seen = [
            at(event, "body", "message", "id")
            for event in dicts(json_body(payload)["events"])
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
        self.assertEqual(len(expect_list(first["events"])), 100)
        self.assertIs(first["has_more"], True)
        cursor = expect_str(first["next_cursor"])
        for event in dicts(first["events"]):
            self.assertNotEqual(cursor, str(event["sequence"]))
        _, _, payload = request(
            "GET",
            f"{server.origin}/events?after={urllib.parse.quote(cursor, safe='')}",
            token="writer-token-value",
        )
        second = json_body(payload)
        self.assertEqual(len(expect_list(second["events"])), 1)
        self.assertIs(second["has_more"], False)
        first_ids = {expect_str(event["id"]) for event in dicts(first["events"])}
        self.assertNotIn(expect_str(at(second, "events", 0, "id")), first_ids)

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
        cursor = expect_str(json_body(payload)["next_cursor"])
        server.shutdown()
        server.server_close()
        self.servers.remove(server)

        restarted = self.start(visibility="members", retention_seconds=3600)
        _, _, payload = request("GET", f"{restarted.origin}/events", token="reader-token-value")
        texts = [
            at(event, "body", "message", "body", "text")
            for event in dicts(json_body(payload)["events"])
        ]
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
        results: list[tuple[int, dict[str, object]]] = []
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
        self.assertEqual(len(expect_list(json_body(payload)["events"])), 1)

        run_pair("message:differ", "left", "right")
        self.assertEqual(sorted(status for status, _ in results), [200, 409])

    def test_collaboration_records_survive_restart(self) -> None:
        credentials = (
            host.Credential("agent:abc123", "writer-token-value", True, True),
            host.Credential("agent:reader", "reader-token-value", True, False),
            host.Credential("agent:peer", "token-peer", True, True),
            host.Credential("agent:outsider", "token-outsider", True, True),
        )
        server = self.start(
            visibility="addressed",
            credentials=credentials,
            retention_seconds=3600,
        )
        origin = server.origin
        status, _, payload = request("GET", f"{origin}/.well-known/agentciv")
        world = json_body(payload)
        self.assertEqual(status, 200)
        self.assertIn("collaboration.submit", expect_list(world["capabilities"]))
        self.assertEqual(at(world, "endpoints", "collaborate"), f"{origin}/collaborate")

        def post(
            token: str, record: Mapping[str, object], content_type: str = "application/json"
        ) -> tuple[int, dict[str, object]]:
            raw = json.dumps(record, separators=(",", ":")).encode("utf-8")
            posted, _, body = request(
                "POST",
                f"{origin}/collaborate",
                token=token,
                body=raw,
                content_type=content_type,
            )
            return posted, json_body(body)

        denied, denied_body = post(
            "reader-token-value",
            artifact_revision("submission:denied", "agent:reader", ["agent:peer"]),
        )
        self.assertEqual(denied, 403)
        self.assertEqual(denied_body["code"], "forbidden")

        first = artifact_revision("submission:plan", "agent:abc123", ["agent:peer"])
        raw = json.dumps(first, separators=(",", ":")).encode("utf-8")
        created, headers, payload = request(
            "POST", f"{origin}/collaborate", token="writer-token-value", body=raw
        )
        self.assertEqual(created, 200)
        self.assertIn("no-store", headers["cache-control"])
        receipt = json_body(payload)
        self.assertEqual(receipt["status"], "recorded")
        self.assertEqual(receipt["artifact_id"], "artifact:plan")
        self.assertEqual(receipt["revision"], 1)
        self.assertNotIn("aim", receipt)
        self.assertNotIn("continuity_note", receipt)
        event_id = receipt["event_id"]

        retried, _, payload = request(
            "POST",
            f"{origin}/collaborate",
            token="writer-token-value",
            body=raw,
            content_type="application/json; charset=utf-8",
        )
        self.assertEqual(retried, 200)
        self.assertEqual(json_body(payload)["event_id"], event_id)

        cites_missing = dict(first)
        cites_missing["derived_from"] = {
            "from": "agent:abc123",
            "artifact_id": "artifact:plan",
            "revision": 9,
        }
        missing_retry, missing_body = post("writer-token-value", cites_missing)
        self.assertEqual(missing_retry, 422)
        self.assertEqual(missing_body["code"], "unknown_target")

        changed = dict(first)
        changed["body"] = {"text": "different bytes"}
        conflict, conflict_body = post("writer-token-value", changed)
        self.assertEqual(conflict, 409)
        self.assertEqual(conflict_body["code"], "id_conflict")

        chosen = dict(first)
        chosen["id"] = "submission:chosen-revision"
        chosen["revision"] = 7
        rejected, rejected_body = post("writer-token-value", chosen)
        self.assertEqual(rejected, 422)
        self.assertEqual(rejected_body["code"], "invalid_record")

        second = artifact_revision("submission:plan-2", "agent:abc123", ["agent:peer"])
        second["body"] = {"text": "A second revision."}
        del second["continuity_note"]
        again, again_body = post("writer-token-value", second)
        self.assertEqual(again, 200)
        self.assertEqual(again_body["revision"], 2)

        own, own_body = post(
            "token-outsider",
            artifact_revision("submission:other-chain", "agent:outsider", ["agent:outsider"]),
        )
        self.assertEqual(own, 200)
        self.assertEqual(own_body["revision"], 1)
        self.assertNotEqual(own_body["event_id"], event_id)

        hidden, hidden_body = post(
            "token-outsider",
            {
                "protocol_version": "0.1-draft",
                "type": "artifact_revision",
                "id": "submission:hidden-cite",
                "artifact_id": "artifact:fork",
                "world": "civ:local",
                "from": "agent:outsider",
                "to": ["agent:outsider"],
                "media_type": "text/plain",
                "body": {"text": "cite"},
                "derived_from": {
                    "from": "agent:abc123",
                    "artifact_id": "artifact:plan",
                    "revision": 1,
                },
            },
        )
        self.assertEqual(hidden, 422)
        self.assertEqual(hidden_body["code"], "unknown_target")

        hidden_withdrawal, hidden_withdrawal_body = post(
            "token-outsider",
            {
                "protocol_version": "0.1-draft",
                "type": "withdrawal",
                "id": "submission:hidden-withdraw",
                "world": "civ:local",
                "from": "agent:outsider",
                "artifact_id": "artifact:plan",
                "target_from": "agent:abc123",
                "revision": 1,
            },
        )
        self.assertEqual(hidden_withdrawal, 422)
        self.assertEqual(hidden_withdrawal_body["code"], "unknown_target")

        missing, missing_body = post(
            "token-peer",
            {
                "protocol_version": "0.1-draft",
                "type": "objection",
                "id": "submission:missing",
                "world": "civ:local",
                "from": "agent:peer",
                "to": ["agent:abc123"],
                "artifact_id": "artifact:plan",
                "target_from": "agent:abc123",
                "revision": 9,
                "body": {"text": "No such revision."},
            },
        )
        self.assertEqual(missing, 422)
        self.assertEqual(missing_body["code"], "unknown_target")

        objection, objection_body = post(
            "token-peer",
            {
                "protocol_version": "0.1-draft",
                "type": "objection",
                "id": "submission:objection",
                "world": "civ:local",
                "from": "agent:peer",
                "to": ["agent:abc123"],
                "artifact_id": "artifact:plan",
                "target_from": "agent:abc123",
                "revision": 1,
                "body": {"text": "The plan still treats a summary as the source."},
            },
        )
        self.assertEqual(objection, 200)
        self.assertEqual(objection_body["status"], "recorded")

        decline, _decline_body = post(
            "token-peer",
            {
                "protocol_version": "0.1-draft",
                "type": "decline",
                "id": "submission:decline",
                "world": "civ:local",
                "from": "agent:peer",
                "to": ["agent:abc123"],
                "artifact_id": "artifact:plan",
                "target_from": "agent:abc123",
                "revision": 1,
                "body": {"text": "I will not take up this plan."},
            },
        )
        self.assertEqual(decline, 200)

        _, _, payload = request("GET", f"{origin}/events", token="token-peer")
        before = dicts(json_body(payload)["events"])
        self.assertTrue(
            any(
                at(event, "kind") == "artifact.recorded"
                and at(event, "body", "artifact_revision", "revision") == 1
                and at(event, "body", "artifact_revision", "continuity_note", "aim")
                == "Leave a plan a later participant can resume or reject."
                and at(event, "body", "artifact_revision", "note", "keep") is True
                for event in before
            )
        )

        stolen, stolen_body = post(
            "token-peer",
            {
                "protocol_version": "0.1-draft",
                "type": "withdrawal",
                "id": "submission:steal",
                "world": "civ:local",
                "from": "agent:peer",
                "artifact_id": "artifact:plan",
                "target_from": "agent:abc123",
                "revision": 1,
            },
        )
        self.assertEqual(stolen, 403)
        self.assertEqual(stolen_body["code"], "forbidden")

        withdrawn, withdrawn_body = post(
            "writer-token-value",
            {
                "protocol_version": "0.1-draft",
                "type": "withdrawal",
                "id": "submission:withdraw",
                "world": "civ:local",
                "from": "agent:abc123",
                "artifact_id": "artifact:plan",
                "target_from": "agent:abc123",
                "revision": 1,
            },
        )
        self.assertEqual(withdrawn, 200)
        self.assertEqual(withdrawn_body["event_id"], event_id)

        cited, _cited_body = post(
            "token-peer",
            {
                "protocol_version": "0.1-draft",
                "type": "objection",
                "id": "submission:after-withdrawal",
                "world": "civ:local",
                "from": "agent:peer",
                "to": ["agent:abc123"],
                "artifact_id": "artifact:plan",
                "target_from": "agent:abc123",
                "revision": 1,
                "body": {"text": "The withdrawal leaves the objection standing."},
            },
        )
        self.assertEqual(cited, 200)

        on_submit, _, payload = request(
            "POST",
            f"{origin}/submit",
            token="writer-token-value",
            body=json.dumps(
                artifact_revision("submission:wrong-door", "agent:abc123", ["agent:peer"]),
                separators=(",", ":"),
            ).encode("utf-8"),
        )
        self.assertEqual(on_submit, 422)
        self.assertEqual(json_body(payload)["code"], "unsupported_record_type")

        message_record: dict[str, object] = {
            "protocol_version": "0.1-draft",
            "type": "message",
            "id": "message:wrong-door",
            "world": "civ:local",
            "from": "agent:abc123",
            "to": ["agent:peer"],
            "body": {"text": "This door is not message submit."},
        }
        wrong_door, wrong_door_body = post("writer-token-value", message_record)
        self.assertEqual(wrong_door, 422)
        self.assertEqual(wrong_door_body["code"], "unsupported_record_type")

        def survived(base: str) -> None:
            _, _, page = request("GET", f"{base}/events", token="token-peer")
            events = dicts(json_body(page)["events"])
            self.assertTrue(
                any(
                    at(event, "kind") == "artifact.withdrawn"
                    and at(event, "body") == {}
                    and at(event, "actor") == "agent:abc123"
                    for event in events
                )
            )
            self.assertTrue(
                any(
                    at(event, "kind") == "objection.recorded"
                    and at(event, "body", "objection", "body", "text")
                    == "The plan still treats a summary as the source."
                    for event in events
                )
            )
            self.assertTrue(any(at(event, "kind") == "decline.recorded" for event in events))
            self.assertTrue(
                any(
                    at(event, "kind") == "artifact.recorded"
                    and at(event, "body", "artifact_revision", "revision") == 2
                    and at(event, "actor") == "agent:abc123"
                    for event in events
                )
            )
            self.assertTrue(
                any(
                    at(event, "kind") == "objection.recorded"
                    and at(event, "body", "objection", "body", "text")
                    == "The withdrawal leaves the objection standing."
                    for event in events
                )
            )

        survived(origin)
        server.shutdown()
        server.server_close()
        self.servers.remove(server)
        restarted = self.start(
            visibility="addressed",
            credentials=credentials,
            retention_seconds=3600,
        )
        survived(restarted.origin)


def artifact_revision(record_id: str, sender: str, recipients: list[str]) -> dict[str, object]:
    record: dict[str, object] = {
        "protocol_version": "0.1-draft",
        "type": "artifact_revision",
        "id": record_id,
        "artifact_id": "artifact:plan",
        "world": "civ:local",
        "from": sender,
        "to": recipients,
        "media_type": "application/json",
        "body": {"text": "Keep the source pages addressable."},
        "continuity_note": {
            "aim": "Leave a plan a later participant can resume or reject.",
            "resume_hint": "Read the objection before choosing a design.",
        },
        "note": {"keep": True},
    }
    return record


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
                env["AGENTCIV_CONFORMANCE_PEER_TOKEN"] = "second-writer-token"
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
        loaded: object = json.loads(completed.stdout)
        report = expect_dict(loaded)
        self.assertEqual(report["runner_scope"], "credentialed-extended")
        summary = expect_dict(report["summary"])
        self.assertEqual(summary["failed"], 0)
        self.assertEqual(summary["skipped"], 0)
        self.assertEqual(summary["passed"], 47)
        cases = dicts(report["cases"])
        ids = {expect_str(case["id"]) for case in cases}
        self.assertEqual(len(ids), 47)
        self.assertIn("submit.json_charset", ids)
        self.assertIn("events.empty_cursor", ids)
        for case_id in (
            "collaborate.client_revision",
            "collaborate.forbidden",
            "collaborate.unauthenticated",
            "collaborate.payload_too_large",
            "collaborate.unsupported_media_type",
            "collaborate.malformed_json",
            "collaborate.unsupported_version",
            "collaborate.unsupported_record_type",
            "collaborate.partial_note",
            "collaborate.wrong_world",
            "collaborate.from_mismatch",
            "collaborate.revision",
            "collaborate.retry",
            "collaborate.json_charset",
            "collaborate.conflict",
            "collaborate.objection",
            "collaborate.decline",
            "collaborate.absent_revision",
            "collaborate.withdrawal_forbidden",
            "collaborate.withdrawal",
            "collaborate.withdrawn_citation",
            "collaborate.unknown_target",
            "collaborate.other_chain",
        ):
            found = [case for case in cases if case["id"] == case_id]
            self.assertEqual(len(found), 1, case_id)
            self.assertEqual(found[0]["status"], "passed")
            self.assertIs(found[0]["required"], True)


def runner_command(root: Path, discovery: str) -> list[str]:
    arguments = [
        "--discovery",
        discovery,
        "--principal",
        "agent:abc123",
        "--reader",
        "agent:reader",
        "--peer",
        "agent:two",
    ]
    for name in ("agentciv-conformance.exe", "agentciv-conformance"):
        candidate = root / "target" / "debug" / name
        if candidate.exists():
            return [str(candidate), *arguments]
    return ["cargo", "run", "--locked", "-p", "agentciv-conformance", "--", *arguments]


if __name__ == "__main__":
    unittest.main()
