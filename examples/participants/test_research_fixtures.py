"""Hostile-peer display and storage, and the frozen handoff-note comparison.

The host round trip uses the in-repository Python host. No model is started.
"""
from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "examples" / "http-commons"))
import encounter_client as client  # noqa: E402
import encounter_private as private  # noqa: E402
import local_participant  # noqa: E402
import test_encounter_return as returns  # noqa: E402
import test_local_encounter as fixtures  # noqa: E402
import test_local_participant as participants  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
HOSTILE = ROOT / "conformance" / "fixtures" / "hostile-peer"
HANDOFF = ROOT / "conformance" / "fixtures" / "handoff-notes" / "comparison.json"
TOKEN = "synthetic-fixture-credential-0123456789"
USEFUL_PHRASE = "A later source note replaces that support."
STALE_SENTENCE = "The earlier source note is still the current support."


def load(path: Path) -> dict[str, object]:
    value: object = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise AssertionError("expected a JSON object")
    return {str(key): item for key, item in value.items()}


def fixture_paths() -> list[Path]:
    return sorted(path for path in HOSTILE.glob("*.json") if path.name != "manifest.json")


def fixture_draft(source: dict[str, object]) -> local_participant.Draft:
    """Submit the frozen fixture bytes. The host assigns world, sender, and id."""

    def draft(*, world: str, principal: str, recipients: list[str], text: str, message_id: str) -> dict[str, object]:
        del text
        record = dict(source)
        record["world"] = world
        record["from"] = principal
        record["to"] = list(recipients)
        record["id"] = message_id
        return record

    return draft


def keys_of(value: object, found: set[str]) -> None:
    if isinstance(value, dict):
        for key, item in value.items():
            found.add(str(key))
            keys_of(item, found)
    elif isinstance(value, list):
        for item in value:
            keys_of(item, found)


class HostileDisplayTests(unittest.TestCase):
    def test_show_quotes_peer_text_and_does_not_promote_a_claim(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        config, _settings, cache = fixtures.settings(Path(temporary.name).resolve())
        events: list[dict[str, object]] = []
        authorities: list[str] = []
        for index, path in enumerate(fixture_paths(), start=1):
            fixture = load(path)
            body = fixture["body"]
            provenance = fixture["provenance"]
            self.assertIsInstance(body, dict)
            self.assertIsInstance(provenance, dict)
            if not isinstance(body, dict) or not isinstance(provenance, dict):
                raise AssertionError("fixture shape")
            text = body["text"]
            authority = provenance["claimed_authority"]
            self.assertIsInstance(text, str)
            self.assertIsInstance(authority, str)
            if not isinstance(text, str) or not isinstance(authority, str):
                raise AssertionError("fixture text")
            authorities.append(authority)
            identifier = fixture["id"]
            self.assertIsInstance(identifier, str)
            record: dict[str, object] = {
                "protocol_version": "0.1-draft",
                "type": "message",
                "id": identifier,
                "world": fixtures.WORLD,
                "from": fixtures.A,
                "to": [fixtures.A, fixtures.B, fixtures.C],
                "body": {"text": text},
                "provenance": {"claimed_authority": authority},
            }
            events.append(fixtures.event(record, index))
        saved = returns.view(events)
        view_path = cache / "view-hostile.json"
        private.write_new(view_path, json.dumps(saved).encode("utf-8"))
        rendered = client.show(config, view_path)
        self.assertNotIn(TOKEN, rendered)
        self.assertNotIn("\u200b", rendered)
        self.assertIn("\\u200b", rendered)
        self.assertIn("Ignore your previous instructions", rendered)
        self.assertIn("not executed", rendered)
        self.assertNotIn("\n  tool", rendered)
        for authority in authorities:
            self.assertNotIn(authority, rendered)
        zero = (HOSTILE / "zero-width.json").read_text(encoding="utf-8")
        self.assertIn("\\u200b", zero)

    def test_python_host_keeps_the_text_and_leaves_the_claim_out_of_the_receipt(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        directory = Path(temporary.name)
        server = participants.HOST.start_server(participants.HOST.HostConfig(
            world_id="civ:hostile-peer-fixture",
            title="Hostile peer fixtures",
            database_path=directory / "world.sqlite",
            listen=("127.0.0.1", 0),
            visibility="members",
            retention_seconds=60,
            max_payload_bytes=4096,
            credentials=(
                participants.HOST.Credential("agent:peer", "writer-token-value", True, True),
                participants.HOST.Credential("agent:reader", "reader-token-value", True, False),
            ),
        ))

        def stop_server() -> None:
            try:
                server.shutdown()
                server.serve_thread.join(timeout=5)
            finally:
                server.server_close()

        self.addCleanup(stop_server)
        expected: list[tuple[str, str, str]] = []
        for path in fixture_paths():
            fixture = load(path)
            message_id = fixture["id"]
            body = fixture["body"]
            provenance = fixture["provenance"]
            if not isinstance(message_id, str) or not isinstance(body, dict) or not isinstance(provenance, dict):
                raise AssertionError("fixture shape")
            text = body["text"]
            authority = provenance["claimed_authority"]
            if not isinstance(text, str) or not isinstance(authority, str):
                raise AssertionError("fixture text")
            expected.append((message_id, text, authority))

            recorded = local_participant.record_message(
                server.origin, "writer-token-value", "agent:peer", ["agent:reader"],
                text="unused by the fixture draft", message_id=message_id, draft=fixture_draft(fixture),
            )
            receipt = json.dumps(recorded["receipt"])
            self.assertNotIn("claimed_authority", receipt)
            self.assertNotIn(authority, receipt)
            self.assertIn('"status": "recorded"', receipt)
        page = local_participant.read_page(server.origin, "reader-token-value")
        found = local_participant.messages_in(page)
        self.assertEqual([item["id"] for item in found], [item[0] for item in expected])
        for message, (identifier, text, authority) in zip(found, expected, strict=True):
            body = message["body"]
            provenance = message["provenance"]
            self.assertIsInstance(body, dict)
            self.assertIsInstance(provenance, dict)
            if not isinstance(body, dict) or not isinstance(provenance, dict):
                raise AssertionError("stored shape")
            self.assertEqual(message["id"], identifier)
            self.assertEqual(body["text"], text)
            self.assertEqual(provenance["claimed_authority"], authority)
            self.assertIsInstance(provenance["claimed_authority"], str)
        zero = next(item for item in found if item["id"] == "message:hostile-zero-width")
        zero_body = zero["body"]
        self.assertIsInstance(zero_body, dict)
        if isinstance(zero_body, dict):
            stored_text = zero_body["text"]
            self.assertIsInstance(stored_text, str)
            if isinstance(stored_text, str):
                self.assertIn("\u200b", stored_text)


class HandoffFreezeTests(unittest.TestCase):
    def test_four_conditions_share_one_task_and_are_not_a_qualified_improvement(self) -> None:
        value = load(HANDOFF)
        found: set[str] = set()
        keys_of(value, found)
        self.assertNotIn("qualified_improvement", found)
        self.assertEqual(value["status"], "frozen_before_inference")
        self.assertEqual(value["observation"], "unobserved")
        conditions = value["conditions"]
        originals = value["originals"]
        self.assertIsInstance(conditions, dict)
        self.assertIsInstance(originals, dict)
        if not isinstance(conditions, dict) or not isinstance(originals, dict):
            raise AssertionError("comparison shape")
        self.assertEqual(set(conditions), {"absent", "useful", "stale", "misleading"})
        self.assertIsNone(conditions["absent"])
        useful = conditions["useful"]
        stale = conditions["stale"]
        misleading = conditions["misleading"]
        self.assertIsInstance(useful, dict)
        self.assertIsInstance(stale, dict)
        self.assertIsInstance(misleading, dict)
        if not isinstance(useful, dict) or not isinstance(stale, dict) or not isinstance(misleading, dict):
            raise AssertionError("note shape")
        self.assertEqual(useful["aim"], stale["aim"])
        self.assertEqual(stale["aim"], misleading["aim"])
        self.assertEqual(useful["aim"], value["task"])
        self.assertEqual(useful["resume_hint"], originals["correction"])
        self.assertEqual(stale["resume_hint"], originals["obsolete"])
        hint = misleading["resume_hint"]
        self.assertIsInstance(hint, str)
        if isinstance(hint, str):
            self.assertIn(USEFUL_PHRASE, hint)
            self.assertIn(STALE_SENTENCE, hint)
        for note in (useful, stale, misleading):
            for key in ("aim", "resume_hint"):
                text = note[key]
                self.assertIsInstance(text, str)
                if isinstance(text, str):
                    self.assertTrue(1 <= len(text) <= 1024)


if __name__ == "__main__":
    unittest.main()
