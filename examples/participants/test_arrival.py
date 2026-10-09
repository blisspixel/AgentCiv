"""Bounded arrival choices, exact readback, and a separately credentialed return."""

from __future__ import annotations

import copy
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import arrival  # noqa: E402
import collaboration as client  # noqa: E402
import discovery  # noqa: E402
import local_participant as wire  # noqa: E402
import mock_collaboration as mock  # noqa: E402
import walk  # noqa: E402

JsonObject = dict[str, object]


def obj(value: object) -> JsonObject:
    return client.object_value(value)


def objects(value: object) -> list[JsonObject]:
    if not isinstance(value, list):
        raise AssertionError("expected object list")
    return [obj(item) for item in value]


def spans(path: Path) -> list[str]:
    return mock.list_strings(client.decode(path.read_bytes())["records"])


def event(record: JsonObject, sequence: int) -> JsonObject:
    kind = {"artifact_revision": "artifact.recorded", "objection": "objection.recorded",
            "decline": "decline.recorded", "message": "message.recorded"}[str(record["type"])]
    return {"protocol_version": "0.1-draft", "type": "event", "id": f"event:arrival-fixture-{sequence}",
        "world": discovery.WORLD, "sequence": sequence, "timestamp": "2026-10-08T12:00:00Z",
        "kind": kind, "actor": record["from"], "body": {str(record["type"]): record}}


def fixture() -> tuple[JsonObject, list[str]]:
    """Small exact-original fixture, independent of model or native processes."""
    old = {**discovery.artifact(discovery.SOURCE, "notes", "Old claim: one page is complete."), "revision": 1}
    correction = {**discovery.artifact(discovery.SOURCE, "notes", "Correction: inspect all permitted pages.", revision=2), "revision": 2}
    parent = {**discovery.artifact(discovery.GUIDE, "parent", "An unfinished reader plan; objections remain open."), "revision": 1}
    objection: JsonObject = {"protocol_version": "0.1-draft", "type": "objection", "world": discovery.WORLD,
        "id": "submission:reader-objection", "from": discovery.GUIDE, "to": [discovery.NEW, arrival.RETURN],
        "target_from": discovery.GUIDE, "artifact_id": "artifact:parent", "revision": 1,
        "body": {"text": "A first-page reader omits later originals. This objection remains unresolved."}}
    records = [event(record, index + 1) for index, record in enumerate((old, correction, parent, objection))]
    refs = [{"event_id": records[index]["id"], "from": record["from"], "artifact_id": record["artifact_id"],
             "revision": record["revision"]} for index, record in ((1, correction), (0, old), (2, parent))]
    offer = {**discovery.artifact(discovery.GUIDE, "reader-offer", "Optional reader repair invitation."), "revision": 1}
    offered: JsonObject = {"format": "agentciv-activity-offer/0.1-example", "title": "Inspect an unfinished reader",
        "purpose": "Choose whether to inspect and repair a reader using permitted originals.",
        "offered_scope": "Read-only inspection; participation is optional.",
        "limitations": "Unresolved objection; no assignment, access grant or acceptance is implied.",
        "copying_conditions": "Operator permits synthetic fixture export only.", "source_refs": refs}
    offer["body"] = {**obj(offer["body"]), "activity_offer": offered}
    records.append(event(offer, 5))
    originals = [json.dumps(record, ensure_ascii=False) for record in records]
    selected: JsonObject = {"world": discovery.WORLD, "event_id": records[4]["id"], "from": discovery.GUIDE,
        "artifact_id": "artifact:reader-offer", "revision": 1, "sequence": 5, "offer": offered,
        "record_utf8": originals[4], "sources": [{**ref, "status": "available", "record_utf8": originals[index]}
            for ref, index in zip(refs, (1, 0, 2), strict=True)], "earlier_revisions": [], "derivation": None,
        "related_records": [{"event_id": records[3]["id"], "record_utf8": originals[3]}]}
    view: JsonObject = {"format": "agentciv-offer-view/0.1-example", "world": discovery.WORLD,
        "query": "", "offers": [selected], "tombstones": [],
        "observation": {"copying_condition": "operator_authorized_synthetic_fixture_export_only"},
        "report": {"pages": 1, "events": 5, "reached_end": True, "scope": "current_caller_view",
            "copying_permission": "not_granted", "truncated": False, "shortened_fields": []}}
    return view, originals


def contribution() -> tuple[JsonObject, JsonObject, list[str]]:
    record: JsonObject = {"protocol_version": "0.1-draft", "type": "artifact_revision", "world": discovery.WORLD,
        "id": "submission:arrival-revise", "from": discovery.NEW, "to": [arrival.RETURN, discovery.NEW],
        "artifact_id": "artifact:arrival-reader", "media_type": "application/json",
        "body": {"text": "An inspectable unfinished continuation.", "source_event_ids": ["event:arrival-fixture-3"]},
        "derived_from": {"from": discovery.GUIDE, "artifact_id": "artifact:parent", "revision": 1}}
    stored = event({**record, "revision": 1}, 6)
    receipt: JsonObject = {"protocol_version": "0.1-draft", "type": "receipt", "status": "recorded",
        "world": discovery.WORLD, "record_id": record["id"], "event_id": stored["id"], "sequence": 6,
        "artifact_id": record["artifact_id"], "revision": 1}
    return record, receipt, [json.dumps(stored)]


class ArrivalUnitTests(unittest.TestCase):
    def assert_fixed(self, error: Exception) -> None:
        if isinstance(error, arrival.ArrivalError):
            self.assertRegex(error.stage, r"^[a-z_]+$")
            self.assertRegex(error.code, r"^[a-z_]+$")
        self.assertNotIn("private", str(error))

    def test_inspection_requires_current_exact_parent_and_sources(self) -> None:
        view, originals = fixture()
        basis = arrival.inspect_basis(view, originals)
        self.assertIsInstance(basis, dict)
        for case in ("missing_parent", "changed_parent"):
            with self.subTest(case=case):
                changed, raw = copy.deepcopy(view), list(originals)
                if case == "missing_parent":
                    raw.pop(2)
                elif case == "changed_parent":
                    parent = client.decode(raw[2].encode())
                    record = obj(obj(parent["body"])["artifact_revision"])
                    record["body"] = {"text": "Changed parent bytes."}
                    parent["body"] = {"artifact_revision": record}
                    raw[2] = json.dumps(parent)
                with self.assertRaises(ValueError) as caught:
                    arrival.inspect_basis(changed, raw)
                self.assert_fixed(caught.exception)

    def test_capture_rejects_partial_history_and_hidden_material(self) -> None:
        for case in ("partial_history", "hidden_sentinel", "credential_reflection"):
            view, originals = fixture()
            if case == "partial_history":
                view["report"] = {**obj(view["report"]), "reached_end": False}
            else:
                view["untrusted"] = "Restricted fixture sentinel" if case == "hidden_sentinel" else "private-token"
            with self.subTest(case=case), tempfile.TemporaryDirectory() as temporary:
                output = Path(temporary)

                def private_view(binary: Path, origin: str, token: str, staging: Path, label: str,
                                 query: str = "reader") -> JsonObject:
                    mock.save(staging / f"{label}-originals.json", {"world": discovery.WORLD, "records": originals})
                    mock.save(staging / f"{label}-view.json", view)
                    return view

                with patch.object(discovery, "view", side_effect=private_view), self.assertRaises(ValueError) as caught:
                    arrival.capture(Path("unused"), "http://127.0.0.1:1", "private-token", output, "case")
                self.assert_fixed(caught.exception)
                self.assertFalse((output / "case-view.json").exists())
                self.assertFalse((output / "case-originals.json").exists())
                self.assertEqual(list(output.iterdir()), [])

    def test_receipt_is_not_success_without_exact_correlated_readback(self) -> None:
        record, receipt, originals = contribution()
        self.assertEqual(arrival.verify_readback(record, receipt, originals), originals[0])
        for field, wrong in (("status", "delivered"), ("event_id", "event:other"),
                             ("record_id", "submission:other"), ("sequence", True), ("revision", 2)):
            with self.subTest(field=field), self.assertRaises(ValueError) as caught:
                arrival.verify_readback(record, {**receipt, field: wrong}, originals)
            self.assert_fixed(caught.exception)
        changed = client.decode(originals[0].encode())
        changed["body"] = {"artifact_revision": {**record, "revision": 1, "body": {"text": "Different result."}}}
        for raw in ([], ["private-not-json"], [json.dumps(changed)], originals * 2):
            with self.subTest(raw_count=len(raw)), self.assertRaises(ValueError) as caught:
                arrival.verify_readback(record, receipt, raw)
            self.assert_fixed(caught.exception)

    def test_a_denied_write_does_not_become_a_decline_or_success(self) -> None:
        basis = arrival.inspect_basis(*fixture())
        origin = "http://127.0.0.1:1"
        descriptor: JsonObject = {"id": discovery.WORLD, "profile": "http-commons/0.1-draft",
            "capabilities": ["messages.submit", "events.read", "collaboration.submit"], "endpoints": {
                "submit": origin + "/submit", "events": origin + "/events", "collaborate": origin + "/collaborate"}}
        problem = json.dumps({"status": 403, "code": "forbidden"}).encode()
        for choice in ("message", "revise", "object", "decline"):
            with self.subTest(choice=choice), patch.object(wire, "discover", return_value=descriptor), \
                patch.object(wire, "exchange", return_value=(403, problem)) as requested, self.assertRaises(arrival.ArrivalError) as caught:
                arrival.publish_choice(origin, "private-token", choice, basis)
            requested.assert_called_once()
            self.assertEqual((caught.exception.stage, caught.exception.code), ("publication", "publication_failed"))
            self.assert_fixed(caught.exception)
        with patch.object(wire, "exchange") as called:
            self.assertIsNone(arrival.publish_choice(origin, "private-token", "inspect", basis))
            called.assert_not_called()

    def test_return_discovery_searches_current_author_without_reusing_supplied_event_id(self) -> None:
        _, _, originals = contribution()
        self.assertEqual(arrival.find_returned("revise", originals), originals[0])
        self.assertIsNone(arrival.find_returned("inspect", []))
        with self.assertRaises(ValueError) as caught:
            arrival.find_returned("revise", [])
        self.assert_fixed(caught.exception)

    def test_returned_original_bytes_must_equal_the_verified_original(self) -> None:
        view, originals = fixture()
        record, receipt, published = contribution()
        after = {**view, "report": {**obj(view["report"]), "events": len(originals) + 1}}
        captures = [(view, originals), (view, originals), (after, originals + published),
                    (after, originals + [" " + published[0]])]
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "failed"
            with patch.object(arrival, "capture", side_effect=captures), \
                patch.object(arrival, "publish_choice", return_value=(record, receipt)), \
                patch.object(discovery, "seed"), patch.object(walk, "start_host"), \
                patch.object(walk, "wait_until_ready", return_value="http://127.0.0.1:1"), \
                patch.object(walk, "stop_host"), self.assertRaises(arrival.ArrivalError) as caught:
                arrival.run(output, choice="revise", reader_binary=Path(__file__), host_binary=Path(__file__))
            self.assertEqual((caught.exception.stage, caught.exception.code),
                ("restart_and_return", "arrival_stage_failed"))
            report = client.decode((output / "report.json").read_bytes())
            self.assertEqual(report["outcome"], "failed")
            self.assertNotIn("later_interpretation", report)

    def test_uncertain_submission_retains_attempt_without_claiming_recording(self) -> None:
        view, originals = fixture()
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "failed"
            with patch.object(arrival, "capture", side_effect=[(view, originals), (view, originals)]), \
                patch.object(arrival, "publish_choice", side_effect=arrival.ArrivalError("publication", "publication_failed")), \
                patch.object(discovery, "seed"), patch.object(walk, "start_host"), \
                patch.object(walk, "wait_until_ready", return_value="http://127.0.0.1:1"), \
                patch.object(walk, "stop_host"), self.assertRaises(arrival.ArrivalError) as caught:
                arrival.run(output, choice="revise", reader_binary=Path(__file__), host_binary=Path(__file__))
            self.assertEqual(caught.exception.stage, "publication")
            report = client.decode((output / "report.json").read_bytes())
            action = obj(report["participant_action"])
            self.assertEqual(report["outcome"], "failed")
            self.assertEqual(action["publication_state"], "attempt_pending_outcome_unknown")
            self.assertNotIn("receipt", action)
            self.assertNotIn("event_record_utf8", action)
            self.assertNotIn("participant_report", report)
            self.assertNotIn("later_interpretation", report)

    def test_operator_grants_and_fixed_runtime_failure_do_not_imply_acceptance(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            tokens = {principal: "synthetic-fixture-token-" + str(index)
                for index, principal in enumerate((discovery.GUIDE, discovery.SOURCE, discovery.NEW, discovery.HIDDEN, arrival.RETURN))}
            for choice in ("inspect", "revise"):
                config_path = directory / "config.json"
                arrival.configure(config_path, tokens, choice)
                credentials = objects(client.decode(config_path.read_bytes())["credentials"])
                self.assertIs(next(item["write"] for item in credentials if item["principal"] == arrival.RETURN), False)
                self.assertIs(next(item["write"] for item in credentials if item["principal"] == discovery.NEW), choice != "inspect")
            output = directory / "failed"
            with patch.object(walk, "start_host", side_effect=walk.WalkFailure("private-process-trace")), \
                self.assertRaises(arrival.ArrivalError) as caught:
                arrival.run(output, reader_binary=Path(__file__), host_binary=Path(__file__))
            self.assert_fixed(caught.exception)
            report = client.decode((output / "report.json").read_bytes())
            self.assertEqual(report["outcome"], "failed")
            self.assertEqual(report["failure_stage"], "initial_history")
            self.assertNotIn("private-process-trace", json.dumps(report))
            self.assertNotIn("participant_action", report)

    def test_configuration_and_cli_failures_have_fixed_diagnostics(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for host, choice in (("unknown", "inspect"), ("python", "unknown")):
                with self.subTest(host=host, choice=choice), self.assertRaises(arrival.ArrivalError) as caught:
                    arrival.run(directory / "unused", host=host, choice=choice)
                self.assertEqual(caught.exception.code, "invalid_configuration")
            with self.assertRaises(arrival.ArrivalError) as caught:
                arrival.run(directory)
            self.assertEqual(caught.exception.code, "output_exists")
        for failure in (arrival.ArrivalError("publication", "arrival_stage_failed"), ValueError("private-trace")):
            with patch.object(sys, "argv", ["arrival.py", "--output", "unused"]), \
                patch.object(arrival, "run", side_effect=failure), patch("builtins.print") as printed:
                self.assertEqual(arrival.main(), 1)
                self.assertNotIn("private-trace", str(printed.call_args))
        with patch.object(sys, "argv", ["arrival.py", "--output", "unused"]), \
            patch.object(arrival, "run", return_value={"outcome": "passed"}), patch("builtins.print"):
            self.assertEqual(arrival.main(), 0)


class ArrivalProcessTests(unittest.TestCase):
    reader_binary: Path
    host_binary: Path

    @classmethod
    def setUpClass(cls) -> None:
        binaries = walk.build_rust_binaries("agentciv-reader", "agentciv-host")
        cls.reader_binary = binaries["agentciv-reader"]
        cls.host_binary = binaries["agentciv-host"]

    def test_choices_readback_and_fresh_return_use_actual_http(self) -> None:
        conditions = [("python", choice) for choice in arrival.CHOICES] + [("rust", "inspect"), ("rust", "revise")]
        for host, choice in conditions:
            with self.subTest(host=host, choice=choice), tempfile.TemporaryDirectory(prefix="agentciv-arrival-test-") as temporary:
                output = Path(temporary) / "run"
                report = arrival.run(output, host=host, choice=choice,
                    reader_binary=self.reader_binary, host_binary=self.host_binary)
                self.assertEqual(report["outcome"], "passed", report)
                self.assertTrue(report["source_unchanged"])
                action = obj(report["participant_action"])
                interpreted = obj(report["later_interpretation"])
                self.assertEqual(action["choice"], choice)
                self.assertFalse(interpreted["meaningful_repair"])
                self.assertTrue(interpreted["objection_retained"])
                initial, returned = spans(output / "initial-originals.json"), spans(output / "return-originals.json")
                self.assertGreater(len(initial), 100)
                self.assertEqual(obj(obj(report["initial"])["report"])["pages"], 2)
                self.assertEqual(obj(obj(report["return"])["report"])["pages"], 2)
                self.assertTrue(all("Restricted fixture sentinel" not in text for text in initial + returned))
                before = [client.decode(text.encode()) for text in initial]
                after = [client.decode(text.encode()) for text in returned]
                old_objection = next(event for event in before if event["kind"] == "objection.recorded")
                self.assertIn(old_objection, after)
                if choice == "inspect":
                    self.assertFalse(obj(report["participant_report"])["published"])
                    self.assertFalse(any(event.get("actor") == discovery.NEW for event in after))
                    self.assertEqual(initial, returned)
                else:
                    self.assertTrue(obj(report["participant_report"])["published"])
                    self.assertTrue(obj(report["participant_report"])["readback_verified"])
                    self.assertTrue(interpreted["fresh_caller_found"])
                    self.assertTrue(interpreted["exact_original_retained"])
                    receipt = obj(action["receipt"])
                    actual = next(text for text in returned if client.decode(text.encode())["id"] == receipt["event_id"])
                    self.assertEqual(actual, action["event_record_utf8"])
                    self.assertEqual(client.decode(actual.encode())["actor"], discovery.NEW)
                    self.assertEqual(len(returned), len(initial) + 1)
                self.assertEqual(client.decode((output / "report.json").read_bytes()), report)
                self.assertNotIn("token", json.dumps(report))
                self.assertFalse(list(output.glob("*.sqlite")))
                self.assertFalse(list(output.glob("*config*")))


if __name__ == "__main__":
    unittest.main()
