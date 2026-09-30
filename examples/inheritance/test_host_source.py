"""Actual raw HTTP fixture recording, restart, scoped newcomer reads and failures."""

from __future__ import annotations

import copy
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import host_source  # noqa: E402
import oracle  # noqa: E402
import collaboration as client  # noqa: E402
import local_participant as wire  # noqa: E402
import walk  # noqa: E402

HERE = Path(__file__).resolve().parent


def seeds() -> list[oracle.JsonObject]:
    return oracle.validate_events(oracle.load_json(HERE / "history.json"))


class LiveHostTests(unittest.TestCase):
    def test_separate_writer_grants_host_assignments_restart_and_read_only_newcomer(self) -> None:
        requests: list[tuple[str, str | None, bytes | None, int]] = []
        raw_exchange = wire.exchange

        def observe(method: str, url: str, *, token: str | None = None,
                    body: bytes | None = None) -> tuple[int, bytes]:
            result = raw_exchange(method, url, token=token, body=body)
            requests.append((method, token, body, result[0]))
            return result

        with tempfile.TemporaryDirectory(prefix="agentciv-source-test-") as temporary:
            directory = Path(temporary)
            with patch.object(wire, "exchange", side_effect=observe), \
                 patch.object(client, "OllamaDecision", side_effect=AssertionError("no model call")):
                events = host_source.create_history(directory)
            self.assertEqual(list(directory.iterdir()), [])
        self.assertEqual([event["kind"] for event in events], [event["kind"] for event in seeds()])
        self.assertTrue(all(event["id"] not in {seed["id"] for seed in seeds()} for event in events))
        posts = [(token, oracle.object_value(json.loads(body)))
                 for method, token, body, status in requests if method == "POST" and status == 200 and body is not None]
        self.assertEqual(len(posts), 6)
        author_tokens: dict[str, str | None] = {}
        for token, record in posts:
            author = str(record["from"])
            self.assertEqual(author_tokens.setdefault(author, token), token)
            if record["type"] == "artifact_revision":
                self.assertNotIn("revision", record)
        self.assertEqual(set(author_tokens), set(host_source.AUTHORS))
        self.assertEqual(len(set(author_tokens.values())), 3)
        self.assertEqual(sum(status == 401 for _, _, _, status in requests), 4)
        self.assertEqual(requests[-1][0], "POST")
        self.assertEqual(requests[-1][2:], (b"{}", 403))
        newcomer_token = requests[-1][1]
        self.assertNotIn(newcomer_token, author_tokens.values())
        encoded = json.dumps(events)
        for _, token, _, _ in requests:
            if token is not None:
                self.assertNotIn(token, encoded)
        revisions = [oracle.artifact(event) for event in events if event["kind"] == "artifact.recorded"]
        self.assertEqual([record["revision"] for record in revisions if record is not None], [1, 1, 2, 1])

    def test_scripted_after_plan_uses_actual_recorded_source_and_before_remains_defective(self) -> None:
        with tempfile.TemporaryDirectory(prefix="agentciv-source-test-") as temporary:
            events = host_source.create_history(Path(temporary))
        before = oracle.validate_plan(oracle.load_json(HERE / "before.json"))
        after = oracle.validate_plan(oracle.load_json(HERE / "after.json"))
        notes = next(event for event in events
                     if (record := oracle.artifact(event)) is not None
                     and record["from"] == "agent:b" and record["artifact_id"] == "artifact:interface-notes"
                     and record["revision"] == 2)
        claims = after["claims"]
        if not isinstance(claims, list):
            raise AssertionError("fixture lacks claims")
        remapped: list[oracle.JsonObject] = []
        for value in claims:
            claim = oracle.object_value(value)
            source = oracle.object_value(claim["source"])
            source["event_id"] = notes["id"]
            claim["source"] = source
            remapped.append(claim)
        after["claims"] = remapped
        self.assertFalse(oracle.evaluate(events, before)["useful_continuation"])
        self.assertTrue(oracle.evaluate(events, after)["useful_continuation"])
        original = oracle.artifact(events[0])
        if original is None:
            raise AssertionError("missing original")
        self.assertEqual(oracle.object_value(original["body"])["reader_plan"], before)

    def test_lost_receipt_evidence_never_retries_and_closes_host_and_private_state(self) -> None:
        raw_exchange = wire.exchange
        start = walk.start_host
        processes: list[walk.RunningHost] = []
        posts = 0

        def capture(argv: list[str]) -> walk.RunningHost:
            running = start(argv)
            processes.append(running)
            return running

        def changed(method: str, url: str, *, token: str | None = None,
                    body: bytes | None = None) -> tuple[int, bytes]:
            nonlocal posts
            status, payload = raw_exchange(method, url, token=token, body=body)
            if method == "POST":
                posts += 1
                receipt = oracle.object_value(json.loads(payload))
                receipt["event_id"] = "event:not-recorded"
                payload = json.dumps(receipt).encode("utf-8")
            return status, payload

        with tempfile.TemporaryDirectory(prefix="agentciv-source-test-") as temporary:
            directory = Path(temporary)
            with patch.object(walk, "start_host", side_effect=capture), \
                 patch.object(wire, "exchange", side_effect=changed):
                with self.assertRaisesRegex(host_source.HostSourceError, "seed_not_read_back"):
                    host_source.create_history(directory)
            self.assertEqual(list(directory.iterdir()), [])
        self.assertEqual(posts, 1)
        self.assertEqual(len(processes), 1)
        self.assertTrue(all(process.process.poll() is not None for process in processes))

    def test_changed_history_after_restart_fails_and_both_processes_are_stopped(self) -> None:
        read_history = client.history
        start = walk.start_host
        processes: list[walk.RunningHost] = []
        reads = 0

        def capture(argv: list[str]) -> walk.RunningHost:
            running = start(argv)
            processes.append(running)
            return running

        def changed(origin: str, token: str) -> list[oracle.JsonObject]:
            nonlocal reads
            reads += 1
            events = read_history(origin, token)
            if reads == 8:
                return events[:-1]
            return events

        with tempfile.TemporaryDirectory(prefix="agentciv-source-test-") as temporary:
            directory = Path(temporary)
            with patch.object(walk, "start_host", side_effect=capture), \
                 patch.object(client, "history", side_effect=changed):
                with self.assertRaisesRegex(host_source.HostSourceError, "restart_history_mismatch"):
                    host_source.create_history(directory)
            self.assertEqual(list(directory.iterdir()), [])
        self.assertEqual(len(processes), 2)
        self.assertTrue(all(process.process.poll() is not None for process in processes))


class FailureTests(unittest.TestCase):
    def test_unsafe_or_missing_private_directory_is_rejected_before_process_start(self) -> None:
        with patch.object(walk, "start_host", side_effect=AssertionError("no process")):
            for directory in (host_source.ROOT, HERE, HERE / "not-created"):
                with self.subTest(path=directory), self.assertRaisesRegex(host_source.HostSourceError, "private_directory_required"):
                    host_source.create_history(directory)

    def test_seed_identity_and_kinds_do_not_expand_fixture_scope(self) -> None:
        event = seeds()[0]
        self.assertNotIn("revision", host_source.seed_record(event))
        for kind in ("artifact.withdrawn", "message.recorded", "unknown"):
            with self.subTest(kind=kind), self.assertRaises(host_source.HostSourceError):
                host_source.seed_record({**event, "kind": kind})
        for key, value in (("world", "outside:world"), ("from", "agent:not-authorized")):
            changed = copy.deepcopy(event)
            record = oracle.object_value(oracle.object_value(changed["body"])["artifact_revision"])
            record[key] = value
            changed["body"] = {"artifact_revision": record}
            with self.assertRaisesRegex(host_source.HostSourceError, "invalid_seed_identity"):
                host_source.seed_record(changed)

    def test_discovery_must_advertise_same_origin_collaboration_in_expected_world(self) -> None:
        valid: oracle.JsonObject = {"id": host_source.WORLD, "capabilities": ["collaboration.submit"],
                                   "endpoints": {"collaborate": "http://127.0.0.1:1/collaborate"}}
        with patch.object(wire, "discover", return_value=valid):
            self.assertEqual(host_source.endpoint("http://127.0.0.1:1"), "http://127.0.0.1:1/collaborate")
        for changed in ({**valid, "id": "different"}, {**valid, "capabilities": []},
                        {**valid, "endpoints": {"collaborate": 1}},
                        {**valid, "endpoints": {"collaborate": "http://127.0.0.1:2/collaborate"}}):
            with patch.object(wire, "discover", return_value=changed), self.assertRaises(host_source.HostSourceError):
                host_source.endpoint("http://127.0.0.1:1")

    def test_invalid_receipts_and_changed_stored_author_or_content_are_not_accepted(self) -> None:
        event = seeds()[0]
        receipt: oracle.JsonObject = {"status": "recorded", "record_id": "submission:reader-1",
                                     "world": host_source.WORLD, "event_id": event["id"], "sequence": 1}
        for key, value in (("status", "accepted"), ("record_id", "different"), ("world", "different"),
                           ("event_id", 1), ("sequence", True), ("code", "secret-token")):
            changed = {**receipt, key: value}
            with patch.object(host_source, "endpoint", return_value="http://127.0.0.1:1/collaborate"), \
                 patch.object(wire, "exchange", return_value=(200, json.dumps(changed).encode("utf-8"))), \
                 self.subTest(field=key), self.assertRaises(host_source.HostSourceError):
                host_source.submit_seed("http://127.0.0.1:1", event, "secret-token")
        for changed_key, changed_value in (("actor", "agent:impostor"), ("sequence", 2), ("kind", "message.recorded"),
                           ("world", "different"), ("body", {"artifact_revision": {"text": "changed"}})):
            changed = {**event, changed_key: changed_value}
            with patch.object(host_source, "endpoint", return_value="http://127.0.0.1:1/collaborate"), \
                 patch.object(wire, "exchange", return_value=(200, json.dumps(receipt).encode("utf-8"))), \
                 patch.object(client, "history", return_value=[changed]), \
                 self.subTest(field=changed_key), self.assertRaisesRegex(host_source.HostSourceError, "seed_readback_mismatch"):
                host_source.submit_seed("http://127.0.0.1:1", event, "secret-token")
        with patch.object(host_source, "endpoint", return_value="http://127.0.0.1:1/collaborate"), \
             patch.object(wire, "exchange", return_value=(403, b'{"code":"secret-token"}')), \
             self.assertRaisesRegex(host_source.HostSourceError, "seed_publication_rejected"):
            host_source.submit_seed("http://127.0.0.1:1", event, "secret-token")

    def test_transport_failure_cleans_private_state_and_does_not_echo_exception(self) -> None:
        with tempfile.TemporaryDirectory(prefix="agentciv-source-test-") as temporary:
            directory = Path(temporary)
            with patch.object(wire, "exchange", side_effect=OSError("private-token-in-exception")):
                with self.assertRaises(host_source.HostSourceError) as caught:
                    host_source.create_history(directory)
            self.assertEqual(str(caught.exception), "host_source_failed")
            self.assertEqual(list(directory.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
