"""Unit and dual-host loopback tests for the missing-grant mechanics packet.

Exercises only synthetic local mechanics via the documented local room wrapper
and both loopback hosts. No model calls or public endpoints are used.
"""
from __future__ import annotations

import io
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "http-commons"))
import encounter_private as private
import missing_grant_mechanics as mechanics
import reading_table_journey as journey
import walk

JsonObject = dict[str, object]


def _obj(value: object) -> JsonObject:
    if not isinstance(value, dict):
        raise AssertionError("expected JSON object")
    return {str(key): item for key, item in value.items()}


def _sample_note_observed(fixture: mechanics.MissingGrantFixture) -> journey.Observed:
    record = mechanics.collaboration_note_record(fixture)
    stored = dict(record)
    stored["revision"] = 1
    event: JsonObject = {
        "protocol_version": "0.1-draft",
        "type": "event",
        "id": "event:synthetic-note-1",
        "world": fixture.world,
        "sequence": 1,
        "timestamp": "2026-11-25T00:00:00Z",
        "kind": "artifact.recorded",
        "actor": fixture.author,
        "body": {"artifact_revision": stored},
    }
    return journey.observe_original(json.dumps(event, ensure_ascii=True))


class MissingGrantFixtureTests(unittest.TestCase):
    def test_default_fixture_loads_and_rejects_operator_grant_claim(self) -> None:
        fixture = mechanics.load_fixture(mechanics.DEFAULT_FIXTURE)
        self.assertEqual(fixture.world, "civ:missing-grant-mechanics")
        self.assertEqual(fixture.invitation.source_kind, "synthetic_fixture_content")
        self.assertFalse(fixture.invitation.operator_grant)
        self.assertEqual(len(fixture.note.open_items), 2)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            raw = json.loads(mechanics.DEFAULT_FIXTURE.read_text(encoding="utf-8"))
            raw["peer_invitation"]["operator_grant"] = True
            bad = root / "claimed-grant.json"
            bad.write_text(json.dumps(raw), encoding="utf-8")
            with self.assertRaises(mechanics.MechanicsError) as error:
                mechanics.load_fixture(bad)
            self.assertEqual(error.exception.code, "invitation_must_not_claim_operator_grant")

    def test_fixture_loader_rejects_missing_symlink_duplicate_keys_and_bad_fields(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.assertRaises(mechanics.MechanicsError) as missing:
                mechanics.load_fixture(root / "missing.json")
            self.assertEqual(missing.exception.code, "fixture_unavailable")

            link = root / "link.json"
            link.symlink_to(mechanics.DEFAULT_FIXTURE)
            with self.assertRaises(mechanics.MechanicsError) as linked:
                mechanics.load_fixture(link)
            self.assertEqual(linked.exception.code, "fixture_unavailable")

            oversized = root / "large.json"
            oversized.write_bytes(b" " * (mechanics.MAX_FIXTURE_BYTES + 1))
            with self.assertRaises(mechanics.MechanicsError) as too_large:
                mechanics.load_fixture(oversized)
            self.assertEqual(too_large.exception.code, "fixture_too_large")

            dup = root / "dup.json"
            dup.write_text('{"a": 1, "a": 2}\n', encoding="utf-8")
            with self.assertRaises(mechanics.MechanicsError) as duplicate:
                mechanics.load_fixture(dup)
            self.assertEqual(duplicate.exception.code, "invalid_fixture_json")

            raw = json.loads(mechanics.DEFAULT_FIXTURE.read_text(encoding="utf-8"))
            raw["principals"]["visitor"] = raw["principals"]["author"]
            dup_principals = root / "dup-principals.json"
            dup_principals.write_text(json.dumps(raw), encoding="utf-8")
            with self.assertRaises(mechanics.MechanicsError) as dup_error:
                mechanics.load_fixture(dup_principals)
            self.assertEqual(dup_error.exception.code, "duplicate_fixture_principals")

            raw = json.loads(mechanics.DEFAULT_FIXTURE.read_text(encoding="utf-8"))
            raw["world"] = "civ:bad\nworld"
            bad_id = root / "bad-id.json"
            bad_id.write_text(json.dumps(raw), encoding="utf-8")
            with self.assertRaises(mechanics.MechanicsError) as id_error:
                mechanics.load_fixture(bad_id)
            self.assertEqual(id_error.exception.code, "invalid_fixture_identifier")

    def test_record_builders_and_private_note_storage(self) -> None:
        fixture = mechanics.load_fixture(mechanics.DEFAULT_FIXTURE)
        note_seen = _sample_note_observed(fixture)
        invite = mechanics.peer_invitation_record(fixture, note_seen)
        invite_body = _obj(invite["body"])
        invite_meta = _obj(invite_body["peer_invitation"])
        self.assertEqual(invite_meta["source_kind"], "synthetic_fixture_content")
        self.assertFalse(invite_meta["operator_grant"])
        self.assertEqual(_obj(invite_meta["note_selector"])["original_sha256"], note_seen.sha256)

        followup = mechanics.visitor_followup_record(fixture, note_seen)
        self.assertEqual(followup["from"], fixture.visitor)
        self.assertEqual(followup["id"], fixture.followup_submission_id)

        non_artifact = journey.Observed(
            event_id="event:msg-1",
            sequence=1,
            kind="message.recorded",
            original_utf8="{}",
            sha256="0" * 64,
            author=fixture.author,
        )
        with self.assertRaises(mechanics.MechanicsError) as bad_invite:
            mechanics.peer_invitation_record(fixture, non_artifact)
        self.assertEqual(bad_invite.exception.code, "note_not_an_artifact")
        with self.assertRaises(mechanics.MechanicsError) as bad_followup:
            mechanics.visitor_followup_record(fixture, non_artifact)
        self.assertEqual(bad_followup.exception.code, "note_not_an_artifact")

        with tempfile.TemporaryDirectory() as temporary:
            notes = Path(temporary).resolve() / "notes"
            private.create_directory(notes)
            saved = mechanics.save_private_note(
                notes / "draft.json",
                kind="local_draft",
                world=fixture.world,
                principal=fixture.visitor,
                source_event_id=note_seen.event_id,
                source_sha256=note_seen.sha256,
                title=fixture.private_draft_title,
                text=fixture.private_draft_text,
            )
            loaded = _obj(json.loads(saved.read_text(encoding="ascii")))
            self.assertEqual(loaded["kind"], "local_draft")
            self.assertFalse(loaded["shared_history_mutated"])
            self.assertEqual(loaded["copying_permission"], "not_granted")
            with self.assertRaises(mechanics.MechanicsError) as bad_kind:
                mechanics.save_private_note(
                    notes / "other.json",
                    kind="shared_post",
                    world=fixture.world,
                    principal=fixture.visitor,
                    source_event_id=note_seen.event_id,
                    source_sha256=note_seen.sha256,
                    text="invalid",
                )
            self.assertEqual(bad_kind.exception.code, "invalid_private_note_kind")

    def test_failure_classification_separates_access_denial_and_transport(self) -> None:
        self.assertEqual(mechanics.classify_failure(private.EncounterError("forbidden")), "access_denied")
        self.assertEqual(mechanics.classify_failure(private.EncounterError("authentication_required")), "access_denied")
        self.assertEqual(mechanics.classify_failure(private.EncounterError("host_unreachable")), "transport_or_read_failure")
        self.assertEqual(mechanics.classify_failure(private.EncounterError("host_read_timeout")), "transport_or_read_failure")
        self.assertEqual(mechanics.classify_failure(private.EncounterError("private_cache_full")), "other_failure")

    def test_cli_reports_fixed_code_without_leaking_path(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            missing = Path(temporary) / "absent.json"
            stdout = io.StringIO()
            with patch.object(sys, "stdout", stdout):
                code = mechanics.main(["--fixture", str(missing), "--host", "python"])
            self.assertEqual(code, 1)
            payload = _obj(json.loads(stdout.getvalue()))
            self.assertEqual(payload, {"outcome": "failed", "code": "fixture_unavailable"})
            self.assertNotIn(temporary, stdout.getvalue())


class LiveMissingGrantMechanicsTests(unittest.TestCase):
    reader_binary: Path
    host_binary: Path

    @classmethod
    def setUpClass(cls) -> None:
        binaries = walk.build_rust_binaries("agentciv-reader", "agentciv-host")
        cls.reader_binary = binaries["agentciv-reader"]
        cls.host_binary = binaries["agentciv-host"]

    def test_missing_grant_mechanics_packet_on_both_hosts(self) -> None:
        report = mechanics.run_packet(
            mechanics.DEFAULT_FIXTURE,
            ("python", "rust"),
            reader_binary=self.reader_binary,
            host_binary=self.host_binary,
        )
        self.assertEqual(report["outcome"], "passed")
        self.assertEqual(report["evidence_scope"], "synthetic_local_mechanics_only")
        limits = _obj(report["limits"])
        self.assertEqual(limits["model_calls"], 0)
        self.assertFalse(limits["live_grant_upgrade"])
        self.assertFalse(limits["interoperability_claim"])
        self.assertFalse(limits["new_conformance_profile"])

        results = report["results"]
        self.assertIsInstance(results, list)
        if not isinstance(results, list):
            raise AssertionError("expected list of host results")
        self.assertEqual(len(results), 2)

        for entry in results:
            row = _obj(entry)
            host_name = str(row["host"])
            with self.subTest(host=host_name):
                ro = _obj(row["read_only_room"])
                self.assertTrue(ro["restarted_and_returned"])
                self.assertTrue(ro["exact_source_preserved"])
                self.assertTrue(ro["private_draft_saved"])
                self.assertTrue(ro["private_decline_saved"])
                self.assertEqual(ro["read_only_submit_outcome"], "rejected")
                self.assertEqual(ro["read_only_submit_code"], "forbidden")
                self.assertTrue(ro["observer_history_unchanged"])
                self.assertEqual(ro["observer_events_before"], ro["observer_events_after_refusal"])
                self.assertEqual(ro["read_access_denial_class"], "access_denied")
                self.assertEqual(ro["network_failure_code"], "host_unreachable")
                self.assertEqual(ro["network_failure_class"], "transport_or_read_failure")
                self.assertNotEqual(ro["read_access_denial_class"], ro["network_failure_class"])
                self.assertTrue(ro["failure_distinct_from_empty_history"])

                mw = _obj(row["matched_writer_room"])
                self.assertEqual(mw["room_kind"], "separately_initialized_matched_writer")
                self.assertFalse(mw["live_grant_upgrade"])
                self.assertEqual(mw["first_submit_outcome"], "recorded")
                self.assertEqual(mw["first_submit_actor"], "agent:visitor")
                self.assertEqual(mw["replay_submit_outcome"], "recorded")
                self.assertTrue(mw["replay_same_event_id"])
                self.assertTrue(mw["replay_same_sequence"])
                self.assertEqual(mw["observer_events_before"], 2)
                self.assertEqual(mw["observer_events_after_submit"], 3)
                self.assertEqual(mw["observer_events_after_replay"], 3)
                self.assertEqual(mw["replay_added_events"], 0)

    def test_cli_runs_on_python_host(self) -> None:
        stdout = io.StringIO()
        with patch.object(sys, "stdout", stdout):
            code = mechanics.main([
                "--fixture",
                str(mechanics.DEFAULT_FIXTURE),
                "--host",
                "python",
                "--reader-binary",
                str(self.reader_binary),
            ])
        self.assertEqual(code, 0, stdout.getvalue())
        parsed = _obj(json.loads(stdout.getvalue()))
        self.assertEqual(parsed["outcome"], "passed")
        self.assertEqual(parsed["hosts"], ["python"])


if __name__ == "__main__":
    unittest.main()
