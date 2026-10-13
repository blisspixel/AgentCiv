"""Offline journal inspection: evidence distinctions, exact data and fixed failures."""
from __future__ import annotations

import copy
import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from decimal import Decimal
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "http-commons"))
import encounter_client as client
import encounter_private as private
import encounter_request as requests
import encounter_return as returns
import local_encounter as operator
import local_participant as wire
import test_encounter_return as views
import test_local_encounter as fixtures
import walk

JsonObject = dict[str, object]


def mutable(value: object) -> JsonObject:
    if not isinstance(value, dict):
        raise AssertionError("expected mutable fixture object")
    return value


def journal(record: JsonObject, *, raw: bytes | None = None, state: str = "uncertain") -> JsonObject:
    exact = client._json(record) if raw is None else raw
    value: JsonObject = {"format": client.JOURNAL_FORMAT, "world": fixtures.WORLD, "principal": fixtures.A,
        "record_id": record["id"], "record_type": record["type"], "request_utf8": exact.decode("utf-8"),
        "request_sha256": hashlib.sha256(exact).hexdigest(), "endpoint_kind": "messages.submit" if record["type"] == "message" else "collaboration.submit",
        "state": state, "copying_permission": "not_granted"}
    if state == "uncertain":
        value["failure_code"] = "publication_outcome_unverified"
    if state == "rejected":
        value["failure_code"] = "forbidden"
    return value


def receipt(record: JsonObject, event: JsonObject) -> JsonObject:
    value: JsonObject = {"protocol_version": "0.1-draft", "type": "receipt", "status": "recorded",
        "world": fixtures.WORLD, "record_id": record["id"], "event_id": event["id"], "sequence": event["sequence"]}
    if record["type"] == "artifact_revision":
        value.update(artifact_id=record["artifact_id"], revision=1)
    return value


class RequestInspectionTests(unittest.TestCase):
    def setUp(self) -> None:
        # These units test matching/control flow; private ACL policy retains its
        # existing independent tests. Real process tests below do not mock ACLs.
        self.acl = patch.object(private, "_windows_acl")
        self.acl.start(); self.addCleanup(self.acl.stop)
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.config, self.settings, self.cache = fixtures.settings(Path(self.temporary.name).resolve())
        self.journal_path, self.view_path = self.cache / "journal.json", self.cache / "view.json"
        self.record = fixtures.authored("message", "message:uncertain", fixtures.A)
        self.event = fixtures.event(self.record, 1)
        self.save(journal(self.record), [self.event])

    def save(self, value: JsonObject, events: list[JsonObject], *, raw_view: bytes | None = None) -> None:
        for path, raw in ((self.journal_path, client._json(value)), (self.view_path, client._json(views.view(events)) if raw_view is None else raw_view)):
            if path.exists():
                path.unlink()
            private.write_new(path, raw)

    def inspect(self) -> JsonObject:
        before = {path: path.read_bytes() for path in (self.config, self.journal_path, self.view_path)}
        entries = set(self.cache.iterdir())
        parents = set(self.cache.parent.iterdir())
        try:
            with patch.object(client, "_binary", return_value=Path("native-reader")), \
                patch.object(requests, "_syntax"), patch.object(client, "_project", return_value={}), \
                patch.object(wire, "exchange", side_effect=AssertionError("offline must not send")), \
                patch.object(wire, "discover", side_effect=AssertionError("offline must not discover")), \
                patch.object(client, "_capture", side_effect=AssertionError("offline must not capture")), \
                patch.object(client, "_publish", side_effect=AssertionError("offline must not publish")):
                return requests.inspect(self.config, self.journal_path, self.view_path)
        finally:
            self.assertEqual(before, {path: path.read_bytes() for path in before})
            self.assertEqual(entries, set(self.cache.iterdir()))
            self.assertEqual(parents, set(self.cache.parent.iterdir()))

    def test_no_receipt_is_only_compatible_even_one_or_many_reused_ids(self) -> None:
        for events in ([self.event], [self.event, fixtures.event(self.record, 9)], []):
            self.save(journal(self.record), events)
            result = self.inspect()
            self.assertEqual(result["evidence_status"], "compatible_candidates" if events else "no_match_in_observation")
            self.assertEqual(len(fixtures.objects(result["compatible_candidates"])), len(events))
            self.assertFalse(result["receipt_retained"])
            self.assertFalse(result["journal_state_changed"])
            self.assertEqual(result["retry_permission"], "not_established")
            self.assertNotIn("unfinished question", json.dumps(result))

    def test_retained_receipt_selects_only_its_event_and_never_falls_back(self) -> None:
        value = journal(self.record)
        value["receipt"] = receipt(self.record, self.event)
        for fault, expected in (("none", "receipt_correlated_match"), ("absent", "receipt_event_unavailable"),
            ("sequence", "receipt_event_changed"), ("actor", "receipt_event_changed"), ("body", "receipt_event_changed")):
            event = copy.deepcopy(self.event)
            if fault == "sequence": event["sequence"] = 2
            if fault == "actor": event["actor"] = fixtures.B
            if fault == "body": mutable(mutable(event["body"])["message"])["body"] = {"text": "changed"}
            self.save(value, ([] if fault == "absent" else [event]) + [fixtures.event(self.record, 8)])
            result = self.inspect()
            self.assertEqual(result["evidence_status"], expected)
            self.assertEqual(result["compatible_candidates"], [])

    def test_all_nonwithdrawal_types_preserve_host_revision_and_unknown_fields(self) -> None:
        for kind in ("message", "artifact_revision", "objection", "decline"):
            record = fixtures.authored(kind, "submission:typed", fixtures.A)
            record["extension"] = {"nested": [False, None, {"unknown": "original"}]}
            event = fixtures.event(record, 1)
            self.save(journal(record), [event])
            self.assertEqual(self.inspect()["evidence_status"], "compatible_candidates")
            value = journal(record); value["receipt"] = receipt(record, event)
            self.save(value, [event])
            self.assertEqual(self.inspect()["evidence_status"], "receipt_correlated_match")
            if kind == "artifact_revision":
                mutable(value["receipt"])["revision"] = 2
                self.save(value, [event])
                self.assertEqual(self.inspect()["evidence_status"], "receipt_event_changed")

    def test_decimal_precision_bool_missing_null_and_list_order_are_not_erased(self) -> None:
        raw = client._json(self.record).replace(b'"body": {', b'"body": {"number": 0.123456789012345678901,')
        record = client._decode(raw)
        self.assertIsInstance(mutable(record["body"])["number"], Decimal)
        event = fixtures.event(self.record, 1)
        raw_event = client._json(event).replace(b'"body": {\n      "text"', b'"body": {\n      "text"')
        # Store exact numeric originals without serializing a Decimal through json.
        event_raw = client._json(event).replace(b'"body": {\n        "text"', b'"body": {\n        "number": 0.123456789012345678901,\n        "text"')
        self.assertNotEqual(raw_event, event_raw)
        view = views.view([]); mutable(view["snapshot"])["records"] = [event_raw.decode()]
        mutable(view["transport"]).update(events=1, response_bytes=len(event_raw) + 100)
        self.save(journal(self.record, raw=raw), [], raw_view=client._json(view))
        self.assertEqual(self.inspect()["evidence_status"], "compatible_candidates")
        for replacement in (b"0.123456789012345678902", b"true", b"null"):
            changed = client._json(view).replace(b"0.123456789012345678901", replacement)
            self.save(journal(self.record, raw=raw), [], raw_view=changed)
            self.assertEqual(self.inspect()["evidence_status"], "no_match_in_observation")
        for left, right in (({"x": False}, {"x": 0}), ({}, {"x": None}), ([1, 2], [2, 1]), ({"x": 1}, {"x": "1"})):
            self.assertFalse(client._json_equal(left, right))
        self.assertTrue(client._json_equal({"x": Decimal("1.0")}, {"x": 1}))

    def test_local_rejection_and_recorded_history_remain_separate_from_current_view(self) -> None:
        self.save(journal(self.record, state="rejected"), [self.event])
        result = self.inspect()
        self.assertEqual(result["evidence_status"], "compatible_candidates")
        self.assertEqual(result["local_rejection_claim"], {"code": "forbidden", "original_response_retained": False})
        value = journal(self.record, state="rejected")
        value["failure_title"] = "Invalid record: check /body"
        self.save(value, [self.event])
        self.assertEqual(self.inspect()["local_rejection_claim"], {"code": "forbidden",
            "title": "Invalid record: check /body", "original_response_retained": False})
        artifact = fixtures.authored("artifact_revision", "submission:earlier", fixtures.A)
        original = fixtures.event(artifact, 1)
        value = journal(artifact, state="recorded")
        value.update(receipt=receipt(artifact, original), event_record_utf8=client._json(original).decode())
        tombstone = {**original, "kind": "artifact.withdrawn", "body": {}}
        self.save(value, [tombstone])
        self.assertEqual(self.inspect()["evidence_status"], "receipt_event_changed")

    def test_pending_receipt_pending_and_recorded_journal_shapes_remain_supported(self) -> None:
        for state in ("attempt_pending_outcome_unknown", "receipt_received_readback_pending", "recorded"):
            value = journal(self.record, state=state)
            if state != "attempt_pending_outcome_unknown": value["receipt"] = receipt(self.record, self.event)
            if state == "recorded": value["event_record_utf8"] = client._json(self.event).decode()
            self.save(value, [self.event])
            self.assertEqual(self.inspect()["evidence_status"], "compatible_candidates" if state == "attempt_pending_outcome_unknown" else "receipt_correlated_match")
        value["event_record_utf8"] = client._json({**self.event, "actor": fixtures.B}).decode()
        self.save(value, [self.event])
        with self.assertRaises(private.EncounterError): self.inspect()

    def test_failure_claim_combinations_must_be_possible_for_the_existing_writer(self) -> None:
        value = journal(self.record, state="rejected"); value["failure_code"] = "unknown_target"
        self.save(value, [])
        with self.assertRaises(private.EncounterError): self.inspect()
        for code in client.PREFIX_READ_FAILURES:
            value = journal(self.record); value["failure_code"] = code
            self.save(value, [self.event])
            with self.assertRaises(private.EncounterError): self.inspect()
            value["receipt"] = receipt(self.record, self.event)
            self.save(value, [self.event])
            self.assertEqual(self.inspect()["evidence_status"], "receipt_correlated_match")
        record = fixtures.authored("objection", "submission:unknown", fixtures.A)
        value = journal(record, state="rejected"); value["failure_code"] = "unknown_target"
        self.save(value, [])
        self.assertEqual(self.inspect()["local_rejection_claim"], {"code": "unknown_target", "original_response_retained": False})

    def test_withdrawal_never_reconstructs_a_prior_target(self) -> None:
        record = fixtures.authored("withdrawal", "submission:withdrawal", fixtures.A)
        tombstone = {**self.event, "kind": "artifact.withdrawn", "body": {}}
        for retained in (False, True):
            value = journal(record)
            if retained: value["receipt"] = receipt(record, tombstone)
            self.save(value, [tombstone])
            result = self.inspect()
            self.assertEqual(result["evidence_status"], "withdrawal_target_not_checkable")
            self.assertEqual(result["compatible_candidates"], [])
            self.assertEqual(result["withdrawal_target"], "not_checkable_without_prior_view")

    def test_edited_journal_bindings_states_hashes_and_receipts_are_invalid(self) -> None:
        original = journal(self.record)
        faults = {"format": "other", "world": "civ:else", "principal": fixtures.B,
            "record_id": "other", "record_type": "decline", "endpoint_kind": "collaboration.submit",
            "request_sha256": "0" * 64, "state": "invented", "copying_permission": "granted",
            "failure_code": "invented", "extra": True}
        for key, replacement in faults.items():
            value = {**original, key: replacement}
            self.save(value, [self.event])
            with self.assertRaises(private.EncounterError): self.inspect()
        for fault in ("extra", "wrong_record", "bool_sequence", "missing", "rejected_receipt", "pending_failure", "recorded_no_original"):
            value = journal(self.record); value["receipt"] = receipt(self.record, self.event)
            if fault == "extra": mutable(value["receipt"])["revision"] = 1
            if fault == "wrong_record": mutable(value["receipt"])["record_id"] = "other"
            if fault == "bool_sequence": mutable(value["receipt"])["sequence"] = True
            if fault == "missing": mutable(value["receipt"]).pop("event_id")
            if fault == "rejected_receipt": value.update(state="rejected", failure_code="forbidden")
            if fault == "pending_failure": value["state"] = "attempt_pending_outcome_unknown"
            if fault == "recorded_no_original": value["state"] = "recorded"; value.pop("failure_code")
            self.save(value, [self.event])
            with self.assertRaises(private.EncounterError): self.inspect()

    def test_complete_view_intervals_budgets_reflection_and_output_bound(self) -> None:
        for fault in ("partial", "wrong_caller", "budget", "interval", "duplicate", "secret"):
            value = views.view([self.event])
            if fault == "partial": mutable(value["transport"])["reached_end"] = False
            if fault == "wrong_caller": value["principal"] = fixtures.B
            if fault == "budget": value["configured_budgets"] = {**client.BUDGETS, "max_events": 0}
            if fault == "interval": value["retrieval_finished"] = "bad"
            if fault == "duplicate": mutable(value["snapshot"])["records"] = [json.dumps(self.event)] * 2; mutable(value["transport"])["events"] = 2
            if fault == "secret": value["extension"] = self.settings["token"]
            self.save(journal(self.record), [], raw_view=client._json(value))
            with self.assertRaises(private.EncounterError): self.inspect()
        self.save(journal(self.record), [self.event])
        with patch.object(requests, "MAX_RESULT_BYTES", 1), self.assertRaises(private.EncounterError) as caught:
            self.inspect()
        self.assertEqual(caught.exception.code, "request_inspection_too_large")

    def test_native_failure_timeout_and_cleanup_leave_no_retained_state(self) -> None:
        before = set(self.cache.iterdir())
        for failure in (private.EncounterError("reader_failed"), subprocess.TimeoutExpired("native", 40)):
            with patch.object(client, "_binary", return_value=Path("native-reader")), \
                patch.object(subprocess, "run", side_effect=failure), self.assertRaises(private.EncounterError):
                requests.inspect(self.config, self.journal_path, self.view_path)
            self.assertEqual(before, set(self.cache.iterdir()))
        with patch.object(client, "_binary", return_value=Path("native-reader")), \
            patch.object(requests, "_syntax"), patch.object(client, "_project", side_effect=private.EncounterError("reader_failed")), \
            self.assertRaises(private.EncounterError):
            requests.inspect(self.config, self.journal_path, self.view_path)
        self.assertEqual(before, set(self.cache.iterdir()))

    def test_syntax_helper_requires_fixed_success_and_names_old_or_refusing_tools(self) -> None:
        responses = [
            (subprocess.CompletedProcess([], 0, b'{"outcome":"valid","scope":"request_syntax","acceptance":"not_established"}', b""), None),
            (subprocess.CompletedProcess([], 1, b"", b'{"outcome":"failed","code":"invalid_request"}'), "request_syntax_unavailable"),
            (subprocess.CompletedProcess([], 1, b"", b'{"outcome":"failed","code":"invalid_configuration"}'), "request_validator_unavailable"),
            (subprocess.CompletedProcess([], 0, b'{"outcome":"valid","extra":"content"}', b""), "request_validator_unavailable"),
            (subprocess.CompletedProcess([], 0, b"x" * 1025, b""), "request_validator_unavailable"),
        ]
        before = set(self.cache.iterdir())
        for response, error in responses:
            with requests._staging(self.settings) as settings, patch.object(subprocess, "run", return_value=response) as run:
                if error is None:
                    requests._syntax(client._json(self.record), settings, Path("native-reader"))
                else:
                    with self.assertRaises(private.EncounterError) as caught:
                        requests._syntax(client._json(self.record), settings, Path("native-reader"))
                    self.assertEqual(caught.exception.code, error)
                self.assertEqual(run.call_args.args[0][1], "check-request")
                self.assertEqual(run.call_args.kwargs["timeout"], 40)
            self.assertEqual(before, set(self.cache.iterdir()))

    def test_duplicate_journal_escaped_secret_and_private_input_bounds_fail_without_result(self) -> None:
        exact = self.journal_path.read_bytes()
        malformed = exact[:-1] + b',"state":"uncertain"}'
        self.journal_path.unlink(); private.write_new(self.journal_path, malformed)
        with self.assertRaises(private.EncounterError): self.inspect()
        secret = str(self.settings["token"])
        raw = client._json({**self.record, "body": {"text": secret}}).replace(secret.encode(), ("\\u0073" + secret[1:]).encode())
        self.save(journal(self.record, raw=raw), [])
        with self.assertRaises(private.EncounterError): self.inspect()
        raw = client._json({**self.record, "body": {"text": "x" * 16384}})
        self.save(journal(self.record, raw=raw), [])
        with self.assertRaises(private.EncounterError) as caught: self.inspect()
        self.assertEqual(caught.exception.code, "authored_record_too_large")
        self.journal_path.unlink(); private.write_new(self.journal_path, b"x")
        self.journal_path.write_bytes(b"x" * (client.MAX_OUTPUT + 1))
        with self.assertRaises(private.EncounterError): self.inspect()

    def test_cleanup_failure_refuses_success_without_deleting_unrelated_inputs(self) -> None:
        before = {path: path.read_bytes() for path in (self.config, self.journal_path, self.view_path)}
        with patch.object(client, "_binary", return_value=Path("native-reader")), patch.object(requests, "_syntax"), \
            patch.object(client, "_project", return_value={}), patch.object(Path, "rmdir", side_effect=OSError("synthetic_cleanup_failure")), \
            self.assertRaises(private.EncounterError):
            requests.inspect(self.config, self.journal_path, self.view_path)
        self.assertEqual(before, {path: path.read_bytes() for path in before})
        owned = [path for path in self.cache.iterdir() if path.name.startswith(".inspect-request-")]
        self.assertEqual(len(owned), 1)
        self.assertEqual(owned[0].resolve().parent, self.cache.resolve())
        private.check_directory(owned[0])
        self.assertEqual(list(owned[0].iterdir()), [])
        owned[0].rmdir()

    def test_cli_escapes_terminal_controls_and_routes_without_body_output(self) -> None:
        self.record["id"] = "message:control-\x1b"
        event = fixtures.event(self.record, 1); event["id"] = "event:control-\x1b"
        self.save(journal(self.record), [event])
        # _authored rejects control-bearing IDs; permitted body controls remain
        # private and never enter selectors or terminal output.
        with self.assertRaises(private.EncounterError): self.inspect()
        result = {"outcome": "inspected", "selected_event": {"event_id": "event:control-\x1b"}}
        with patch.object(sys, "argv", ["local_encounter.py", "inspect-request", str(self.journal_path), "--view", str(self.view_path), "--config", str(self.config)]), \
            patch.object(requests, "inspect", return_value=result), patch("builtins.print") as printed:
            self.assertEqual(operator.main(), 0)
        self.assertNotIn("\x1b", str(printed.call_args.args[0]))
        self.assertIn("\\u001b", str(printed.call_args.args[0]))


class NativeRequestInspectionTests(unittest.TestCase):
    reader: Path
    @classmethod
    def setUpClass(cls) -> None:
        cls.reader = walk.build_rust_binaries("agentciv-reader")["agentciv-reader"]

    def test_real_native_validation_cannot_be_bypassed_by_recomputed_hash(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            config, settings, cache = fixtures.settings(Path(temporary).resolve())
            journal_path, view_path = cache / "journal.json", cache / "view.json"
            record = fixtures.authored("message", "message:native", fixtures.A)
            private.write_new(view_path, client._json(views.view([])))
            for raw in (client._json(record), client._json({**record, "to": []}),
                client._json(record).replace(b'"body": {', b'"body": {"x":1,"\\u0078":2,')):
                if journal_path.exists(): journal_path.unlink()
                private.write_new(journal_path, client._json(journal(record, raw=raw)))
                inputs = {path: path.read_bytes() for path in (config, journal_path, view_path)}
                with patch.object(wire, "exchange", side_effect=AssertionError("offline")), patch.object(wire, "discover", side_effect=AssertionError("offline")):
                    if raw == client._json(record):
                        self.assertEqual(requests.inspect(config, journal_path, view_path, reader_binary=self.reader)["evidence_status"], "no_match_in_observation")
                    else:
                        with self.assertRaises(private.EncounterError): requests.inspect(config, journal_path, view_path, reader_binary=self.reader)
                self.assertEqual(inputs, {path: path.read_bytes() for path in inputs})
                self.assertEqual(set(path.name for path in cache.iterdir()), {"journal.json", "view.json"})


if __name__ == "__main__":
    unittest.main()
