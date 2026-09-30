"""Independent mechanical and source-support failures for the inheritance fixture."""

from __future__ import annotations

import copy
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import oracle  # noqa: E402

HERE = Path(__file__).resolve().parent


def history() -> list[oracle.JsonObject]:
    return oracle.validate_events(oracle.load_json(HERE / "history.json"))


def candidate(name: str = "after") -> oracle.JsonObject:
    return oracle.validate_candidate(oracle.load_json(HERE / f"{name}.json"))


class AcceptanceTests(unittest.TestCase):
    def test_ambiguous_nonstandard_and_resource_exhausted_json_are_invalid_inputs(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "input.json"
            for content in ('{"reader":{},"reader":{}}', '{"body":{"field":1,"field":2}}',
                            '{"field":NaN}', '{"field":Infinity}'):
                path.write_text(content, encoding="utf-8")
                with self.assertRaises(oracle.OracleError):
                    oracle.load_json(path)
            path.write_text("{}", encoding="utf-8")
            with patch.object(json, "loads", side_effect=RecursionError("parser depth exhausted")):
                with self.assertRaisesRegex(oracle.OracleError, "invalid_json_data"):
                    oracle.load_json(path)
            with patch.object(json, "dumps", side_effect=RecursionError("serializer depth exhausted")):
                with self.assertRaisesRegex(oracle.OracleError, "invalid_json_data"):
                    oracle.bounded({})

    def test_disclosed_before_fails_and_independent_after_passes_without_mutation(self) -> None:
        events = history()
        original = copy.deepcopy(events)
        before = oracle.evaluate(events, candidate("before"))
        after = oracle.evaluate(events, candidate())
        self.assertFalse(before["useful_continuation"])
        self.assertFalse(oracle.object_value(before["reuse"])["passed"])
        self.assertFalse(oracle.object_value(before["source_correctness"])["passed"])
        self.assertFalse(oracle.object_value(before["runbook"])["passed"])
        self.assertTrue(after["useful_continuation"])
        self.assertEqual(oracle.object_value(after["reuse"])["retrieved_event_ids"], [event["id"] for event in events])
        self.assertEqual(events, original)
        self.assertEqual(oracle.evaluate(history(), candidate()), after)

    def test_reader_must_visit_later_pages_keep_author_chains_and_all_revisions(self) -> None:
        for key, value in (("pagination", "first"), ("chain_identity", "artifact_only"), ("revisions", "latest"),
                           ("retain_objections", False), ("retain_declines", False)):
            with self.subTest(choice=key):
                plan = candidate()
                reader = oracle.object_value(plan["reader"])
                reader[key] = value
                plan["reader"] = reader
                report = oracle.evaluate(history(), plan)
                self.assertFalse(oracle.object_value(report["reuse"])["passed"])
                self.assertTrue(oracle.object_value(report["source_correctness"])["passed"])
                self.assertFalse(report["useful_continuation"])

    def test_unsafe_runbook_does_not_pass_when_reader_and_citations_work(self) -> None:
        for key, value in (("credential_source", "cited_author"), ("scope", "citation_grants_access"),
                           ("missing_source", "trust_summary"), ("execute_artifacts", True)):
            with self.subTest(choice=key):
                plan = candidate()
                runbook = oracle.object_value(plan["runbook"])
                runbook[key] = value
                plan["runbook"] = runbook
                report = oracle.evaluate(history(), plan)
                self.assertTrue(oracle.object_value(report["reuse"])["passed"])
                self.assertTrue(oracle.object_value(report["source_correctness"])["passed"])
                self.assertFalse(oracle.object_value(report["runbook"])["passed"])
                self.assertFalse(report["useful_continuation"])

    def test_unavailable_source_is_reported_without_revealing_absent_contents(self) -> None:
        events = [event for event in history() if event["id"] != "event:notes-2"]
        report = oracle.evaluate(events, candidate())
        checks = oracle.object_value(report["source_correctness"])["checks"]
        self.assertIsInstance(checks, list)
        self.assertEqual([oracle.object_value(check)["code"] for check in list_value(checks)], ["source_unavailable"] * 3)
        self.assertFalse(report["useful_continuation"])
        exported = json.dumps(report)
        self.assertNotIn("event:notes-2", exported)
        self.assertNotIn("Correction to my earlier notes", exported)

    def test_referenceable_withdrawn_source_is_not_usable_content(self) -> None:
        events = history()
        events[3]["kind"] = "artifact.withdrawn"
        events[3]["body"] = {}
        report = oracle.evaluate(events, candidate())
        self.assertTrue(oracle.object_value(report["reuse"])["passed"])
        self.assertEqual(check_codes(report), ["source_withdrawn"] * 3)
        self.assertFalse(report["useful_continuation"])

    def test_citation_presence_and_summary_cannot_replace_exact_support(self) -> None:
        for transform, expected in (("wrong_revision", "source_revision_mismatch"),
                                    ("wrong_author", "source_revision_mismatch"),
                                    ("wrong_artifact", "source_revision_mismatch"),
                                    ("old_revision", "source_does_not_support_claim"),
                                    ("non_artifact", "source_not_artifact")):
            with self.subTest(case=transform):
                plan = candidate()
                claims = list_value(plan["claims"])
                first = oracle.object_value(claims[0])
                source = oracle.object_value(first["source"])
                if transform == "wrong_revision":
                    source["revision"] = 1
                elif transform == "wrong_author":
                    source["from"] = "agent:a"
                elif transform == "wrong_artifact":
                    source["artifact_id"] = "artifact:history-reader"
                elif transform == "old_revision":
                    source.update({"event_id": "event:notes-1", "revision": 1})
                else:
                    source["event_id"] = "event:objection"
                first["source"] = source
                claims[0] = first
                plan["claims"] = claims
                report = oracle.evaluate(history(), plan)
                self.assertEqual(check_codes(report)[0], expected)
                self.assertFalse(report["useful_continuation"])

    def test_a_source_supporting_stale_interface_claim_is_still_wrong(self) -> None:
        plan = candidate("before")
        baseline = candidate()
        plan["reader"] = baseline["reader"]
        plan["runbook"] = baseline["runbook"]
        report = oracle.evaluate(history(), plan)
        self.assertEqual(check_codes(report), ["contradicts_interface_reference"] * 3)
        self.assertTrue(oracle.object_value(report["reuse"])["passed"])
        self.assertFalse(report["useful_continuation"])

    def test_empty_or_context_free_history_cannot_claim_useful_inheritance(self) -> None:
        for events in ([], [event for event in history() if event["kind"] != "objection.recorded"],
                       [event for event in history() if event["kind"] != "decline.recorded"]):
            report = oracle.evaluate(events, candidate())
            self.assertFalse(report["useful_continuation"])
            self.assertFalse(oracle.object_value(report["reuse"])["passed"])

    def test_missing_claims_are_not_vacuous_success_or_invented_support(self) -> None:
        plan = candidate()
        plan["claims"] = []
        report = oracle.evaluate(history(), plan)
        sources = oracle.object_value(report["source_correctness"])
        self.assertFalse(sources["passed"])
        self.assertEqual(sources["missing_topics"], sorted(oracle.FACTS))

    def test_structural_validator_and_schema_do_not_select_correct_answers(self) -> None:
        before = candidate("before")
        self.assertEqual(oracle.validate_plan(before), before)
        properties = oracle.object_value(oracle.PLAN_SCHEMA["properties"])
        reader = oracle.object_value(oracle.object_value(properties["reader"])["properties"])
        self.assertEqual(oracle.object_value(reader["pagination"])["enum"], ["all", "first"])
        plan = candidate()
        claims = list_value(plan["claims"])
        first = oracle.object_value(claims[0])
        source = oracle.object_value(first["source"])
        source["event_id"] = "event:not-visible"
        first["source"] = source
        claims[0] = first
        plan["claims"] = claims
        self.assertEqual(oracle.validate_plan(plan), plan)
        self.assertFalse(oracle.evaluate(history(), plan)["useful_continuation"])


def list_value(value: object) -> list[object]:
    if not isinstance(value, list):
        raise AssertionError("expected list")
    return value


def check_codes(report: oracle.JsonObject) -> list[object]:
    return [oracle.object_value(check)["code"]
            for check in list_value(oracle.object_value(report["source_correctness"])["checks"])]


class InvalidInputTests(unittest.TestCase):
    def test_history_identity_order_world_and_revisions_are_not_coerced(self) -> None:
        bad: list[object] = [None, {}, [None], history() + [history()[0]], list(reversed(history()))]
        for key, value in (("id", ""), ("id", chr(0xD800)), ("sequence", True), ("sequence", "1"),
                           ("kind", []), ("body", []), ("world", "civ:other")):
            events = history()
            events[1][key] = value
            bad.append(events)
        for revision in (False, 0, "1"):
            events = history()
            record = oracle.artifact(events[0])
            if record is None:
                raise AssertionError("fixture missing artifact")
            record["revision"] = revision
            events[0]["body"] = {"artifact_revision": record}
            bad.append(events)
        duplicate = history()
        record = oracle.artifact(duplicate[-1])
        if record is None:
            raise AssertionError("fixture missing artifact")
        record["from"] = "agent:a"
        duplicate[-1]["body"] = {"artifact_revision": record}
        bad.append(duplicate)
        tombstone = history()
        tombstone[3]["kind"] = "artifact.withdrawn"
        bad.append(tombstone)
        for index, events_value in enumerate(bad):
            with self.subTest(case=index), self.assertRaises(oracle.OracleError):
                oracle.validate_events(events_value)

    def test_plan_never_executes_arbitrary_fields_or_coerces_decision_types(self) -> None:
        bad: list[object] = [None, [], {"format": oracle.FORMAT}, {**candidate(), "shell": "echo hostile"},
                             {**candidate(), "format": "unknown"}, {**candidate(), "claims": {}},
                             {**candidate(), "claims": [{}]}, {**candidate(), "claims": [None]}]
        for section, key, value in (("reader", "pagination", "http://outside.invalid"),
                                    ("reader", "retain_declines", 1),
                                    ("runbook", "credential_source", ["operator"]),
                                    ("runbook", "execute_artifacts", "false"),
                                    ("reader", "extra", "hostile"), ("runbook", "extra", "hostile")):
            plan = candidate()
            part = oracle.object_value(plan[section])
            part[key] = value
            plan[section] = part
            bad.append(plan)
        for key, value in (("topic", "unknown"), ("value", ""), ("extra", "hostile")):
            plan = candidate()
            claims = list_value(plan["claims"])
            claim = oracle.object_value(claims[0])
            claim[key] = value
            claims[0] = claim
            plan["claims"] = claims
            bad.append(plan)
        for key, value in (("revision", True), ("event_id", ""), ("extra", "hostile")):
            plan = candidate()
            claims = list_value(plan["claims"])
            claim = oracle.object_value(claims[0])
            source = oracle.object_value(claim["source"])
            source[key] = value
            claim["source"] = source
            claims[0] = claim
            plan["claims"] = claims
            bad.append(plan)
        plan = candidate()
        claims = list_value(plan["claims"])
        plan["claims"] = claims + [claims[0]]
        bad.append(plan)
        for index, invalid_value in enumerate(bad):
            with self.subTest(case=index), self.assertRaises(oracle.OracleError):
                oracle.validate_candidate(invalid_value)

    def test_input_bounds_non_json_data_and_nonfinite_numbers_fail_closed(self) -> None:
        for value in (object(), float("nan"), chr(0xD800), "x" * (oracle.MAX_BYTES + 1)):
            with self.subTest(value_type=type(value).__name__), self.assertRaises(oracle.OracleError):
                oracle.bounded(value)
        with self.assertRaises(oracle.OracleError):
            oracle.validate_events([{}] * (oracle.MAX_EVENTS + 1))
        with self.assertRaises(oracle.OracleError):
            oracle.validate_candidate({**candidate(), "claims": [{}] * (oracle.MAX_CLAIMS + 1)})
        with self.assertRaises(oracle.OracleError):
            oracle.string("x" * 257)
        with self.assertRaises(oracle.OracleError):
            oracle.string(chr(0xD800))

    def test_file_and_cli_failure_evidence_is_distinct_and_does_not_echo_content(self) -> None:
        with tempfile.TemporaryDirectory(prefix="agentciv-inheritance-") as directory:
            path = Path(directory) / "input.json"
            for content in (b"{malformed_secret", b"\xff", b"x" * (oracle.MAX_BYTES + 1)):
                path.write_bytes(content)
                with self.assertRaises(oracle.OracleError):
                    oracle.load_json(path)
                result = subprocess.run([sys.executable, str(HERE / "oracle.py"), "--history", str(path),
                                         "--candidate", str(HERE / "after.json")], capture_output=True, text=True, check=False)
                self.assertEqual(result.returncode, 2)
                self.assertEqual(json.loads(result.stdout)["outcome"], "invalid_input")
                self.assertNotIn("malformed_secret", result.stdout + result.stderr)
            result = subprocess.run([sys.executable, str(HERE / "oracle.py"), "--history", str(path.with_name("missing")),
                                     "--candidate", str(HERE / "after.json")], capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, 2)
            self.assertEqual(json.loads(result.stdout), {"outcome": "input_unavailable"})

    def test_cli_before_fail_after_pass_are_real_exit_statuses(self) -> None:
        for name, code in (("before", 1), ("after", 0)):
            result = subprocess.run([sys.executable, str(HERE / "oracle.py"), "--history", str(HERE / "history.json"),
                                     "--candidate", str(HERE / f"{name}.json")], capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, code)
            self.assertEqual(json.loads(result.stdout)["useful_continuation"], name == "after")


if __name__ == "__main__":
    unittest.main()
