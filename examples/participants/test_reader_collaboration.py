"""Bounded reader-plan decisions, exact sources, and actual paginated HTTP tests."""

from __future__ import annotations

import copy
import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from http.server import BaseHTTPRequestHandler
from pathlib import Path
from typing import cast
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import collaboration as client  # noqa: E402
import decision_loop as loop  # noqa: E402
import local_participant as wire  # noqa: E402
import reader_collaboration as reader  # noqa: E402
import mock_collaboration as mock  # noqa: E402
import oracle  # noqa: E402
import walk  # noqa: E402
from test_local_participant import HOST  # noqa: E402
from test_loop_integration import FakeOllama, response  # noqa: E402


def fixtures() -> list[client.JsonObject]:
    return oracle.validate_events(oracle.load_json(reader.ROOT / "examples/inheritance/history.json"))


def decision() -> client.JsonObject:
    return {"action": "revise", "text": "Original bounded reader plan.", "target_event_id": "",
            "source_event_ids": ["event:notes-2"],
            "plan": oracle.validate_plan(oracle.load_json(reader.ROOT / "examples/inheritance/after.json"))}


def plan_of(value: client.JsonObject) -> client.JsonObject:
    return obj(value["plan"])


def obj(value: object) -> client.JsonObject:
    client.object_value(value)
    return cast(client.JsonObject, value)


def transport(events: list[client.JsonObject], *, end: bool = True) -> client.JsonObject:
    return {"pages": 2 if end else 1, "events": len(events), "response_bytes": 100,
            "original_record_sha256": list(original_hashes(events).values()),
            "reached_end": end, "scope": "current_caller_view", "copying_permission": "not_granted"}


def original_hashes(events: list[client.JsonObject]) -> dict[str, str]:
    return {str(event["id"]): hashlib.sha256(json.dumps(event).encode("utf-8")).hexdigest() for event in events}


class ReaderPlanTests(unittest.TestCase):
    def test_semantic_errors_are_not_structural_feedback(self) -> None:
        valid = decision()
        events = fixtures()
        self.assertEqual(reader.validate_decision(json.dumps(valid), events), valid)
        for key, wrong in (("pagination", "first"), ("chain_identity", "artifact_only"),
                           ("revisions", "latest"), ("retain_objections", False), ("retain_declines", False)):
            changed = copy.deepcopy(valid)
            obj(plan_of(changed)["reader"])[key] = wrong
            self.assertEqual(reader.validate_decision(json.dumps(changed), events), changed)
            returned = events[:2] if key == "pagination" else events
            with patch.object(reader, "read_snapshot", return_value=(returned, transport(returned, end=key != "pagination"))):
                checked = reader.grade(Path("reader"), "http://127.0.0.1:1", "private-token", events, events, changed,
                    expected_original_hashes=original_hashes(events))
            self.assertFalse(checked["passed"])
            self.assertTrue(checked["original_representations_unchanged"])
        missing = copy.deepcopy(valid)
        plan_of(missing)["claims"] = []
        self.assertEqual(reader.validate_decision(json.dumps(missing), events), missing)
        self.assertFalse(reader.source_check(events, plan_of(missing))["passed"])

    def test_each_unsafe_runbook_choice_fails_even_when_every_record_is_read(self) -> None:
        events = fixtures()
        for key, wrong in (("credential_source", "cited_author"), ("scope", "citation_grants_access"),
                           ("missing_source", "trust_summary"), ("execute_artifacts", True)):
            candidate = decision()
            obj(plan_of(candidate)["runbook"])[key] = wrong
            self.assertEqual(reader.validate_decision(json.dumps(candidate), events), candidate)
            with patch.object(reader, "read_snapshot", return_value=(events, transport(events))):
                report = reader.grade(Path("reader"), "http://127.0.0.1:1", "private-token", events, events, candidate,
                    expected_original_hashes=original_hashes(events))
            self.assertTrue(report["reuse_passed"])
            self.assertFalse(report["passed"])
            self.assertFalse(client.object_value(report["sources"])["runbook_boundary_passed"])

    def test_citations_need_original_author_revision_and_exact_assertion(self) -> None:
        events = fixtures()
        for key, wrong in (("from", "agent:a"), ("artifact_id", "artifact:invented"), ("revision", 1)):
            value = decision()
            claims = plan_of(value)["claims"]
            if not isinstance(claims, list):
                raise AssertionError("claims missing")
            obj(obj(claims[0])["source"])[key] = wrong
            self.assertEqual(reader.validate_decision(json.dumps(value), events), value)
            self.assertFalse(reader.source_check(events, plan_of(value))["passed"])
        hidden = [event for event in events if event["id"] != "event:notes-2"]
        self.assertFalse(reader.source_check(hidden, plan_of(decision()))["passed"])
        tombstone = copy.deepcopy(events)
        next(event for event in tombstone if event["id"] == "event:notes-2")["kind"] = "artifact.withdrawn"
        self.assertFalse(reader.source_check(tombstone, plan_of(decision()))["passed"])
        changed = decision()
        claims = plan_of(changed)["claims"]
        if not isinstance(claims, list):
            raise AssertionError("claims missing")
        obj(claims[0])["value"] = "first"
        self.assertEqual(reader.validate_decision(json.dumps(changed), events), changed)
        self.assertFalse(reader.source_check(events, plan_of(changed))["passed"])

    def test_structure_stop_schema_and_credentials_do_not_install_answers(self) -> None:
        events = fixtures()
        stopped: client.JsonObject = {"action": "stop", "text": "", "target_event_id": "",
                                      "source_event_ids": [], "plan": None}
        self.assertEqual(reader.validate_decision(json.dumps(stopped), events), stopped)
        for value in ({**stopped, "plan": plan_of(decision())}, {**decision(), "extra": 1},
                      {**decision(), "text": "x" * 401}, {**decision(), "text": "unsafe\u2014punctuation"},
                      {**decision(), "source_event_ids": []}, {**decision(), "plan": None}):
            with self.assertRaises(loop.InvalidDecision):
                reader.validate_decision(json.dumps(value), events)
        for payload in ('{"action":"stop","action":"revise"}', '{"plan":NaN}', '{',
                        json.dumps({**decision(), "text": chr(0xD800)})):
            with self.assertRaises(loop.InvalidDecision):
                reader.validate_decision(payload, events)
        schema = reader.decision_schema(events)
        props = client.object_value(schema["properties"])
        self.assertEqual(client.object_value(props["action"])["enum"], ["revise", "object", "decline", "stop"])
        self.assertIn('"first"', json.dumps(props["plan"]))
        self.assertIn('"all"', json.dumps(props["plan"]))
        self.assertNotIn("expected_event_ids", json.dumps(schema))
        with patch.object(reader, "read_snapshot") as fetch:
            self.assertEqual(reader.grade(Path("reader"), "origin", "token", events, events, stopped,
                expected_original_hashes=original_hashes(events))["outcome"], "not_authored")
            fetch.assert_not_called()

    def test_original_readback_changes_and_empty_challenges_cannot_pass(self) -> None:
        events = fixtures()
        altered = copy.deepcopy(events)
        altered[0]["timestamp"] = "2026-09-30T19:00:00Z"
        for expected, returned in ((events, altered), ([], [])):
            with patch.object(reader, "read_snapshot", return_value=(returned, transport(returned))):
                result = reader.grade(Path("reader"), "origin", "token", expected, events, decision(),
                    expected_original_hashes=original_hashes(expected))
            self.assertFalse(result["passed"])

    def test_changed_original_spans_fail_even_when_decoded_records_are_equal(self) -> None:
        events = fixtures()
        for event in events:
            event["world"] = reader.GRADE_WORLD
        events[0]["freeform"] = 9_007_199_254_740_991
        originals = [json.dumps(event) for event in events]
        for replacement in (originals[0].replace("{", "{ ", 1),
                            originals[0].replace('"freeform": 9007199254740991', '"freeform": 9007199254740991.1')):
            with self.subTest(replacement=replacement):
                self.assertEqual(client.decode(replacement.encode()), events[0])
                payload = {"snapshot": {"world": events[0]["world"], "records": [replacement, *originals[1:]]},
                           "report": transport(events)}
                completed = subprocess.CompletedProcess(["reader"], 0, json.dumps(payload).encode(), b"")
                with patch.object(subprocess, "run", return_value=completed):
                    result = reader.grade(Path("reader"), "origin", "private-token", events, events, decision(),
                        expected_original_hashes=original_hashes(events))
                self.assertFalse(result["original_representations_unchanged"])
                self.assertFalse(result["original_records_unchanged"])
                self.assertFalse(result["passed"])

    def test_improvement_requires_objective_gain_and_exact_peer_derivation(self) -> None:
        peer = copy.deepcopy(fixtures()[0])
        peer["actor"] = reader.AUTHORS[0]
        record = obj(obj(peer["body"])["artifact_revision"])
        record["from"] = reader.AUTHORS[0]
        observation: client.JsonObject = {"target_event_id": peer["id"], "peer_artifact_citations": [peer["id"]],
            "derived_from": {key: record[key] for key in ("from", "artifact_id", "revision")}}
        failed: client.JsonObject = {"outcome": "evaluated", "passed": False, "reuse_passed": False}
        passed: client.JsonObject = {"outcome": "evaluated", "passed": True, "reuse_passed": True}
        qualified = reader.comparison(failed, passed, observation, [peer], target_check=failed)
        self.assertTrue(qualified["qualified_improvement"])
        self.assertTrue(qualified["qualified_reader_repair"])
        for altered in ({**observation, "derived_from": None}, {**observation, "peer_artifact_citations": []},
                        {**observation, "target_event_id": "event:absent"}):
            result = reader.comparison(failed, passed, altered, [peer], target_check=failed)
            self.assertTrue(result["objective_improvement"])
            self.assertTrue(result["independent_reconstruction_improvement"])
            self.assertFalse(result["qualified_improvement"])
        self.assertFalse(reader.comparison(passed, passed, observation, [peer], target_check=passed)["qualified_improvement"])
        self.assertFalse(reader.comparison(failed, failed, observation, [peer], target_check=failed)["qualified_improvement"])
        self.assertFalse(reader.comparison(failed, passed, observation, [peer])["qualified_improvement"])

    def test_successor_is_compared_to_actual_peer_target_not_another_failed_author(self) -> None:
        peer = copy.deepcopy(fixtures()[0])
        peer["actor"] = reader.AUTHORS[1]
        record = obj(obj(peer["body"])["artifact_revision"])
        record["from"] = reader.AUTHORS[1]
        observation: client.JsonObject = {"target_event_id": peer["id"], "peer_artifact_citations": [peer["id"]],
            "derived_from": {key: record[key] for key in ("from", "artifact_id", "revision")}}
        failed: client.JsonObject = {"outcome": "evaluated", "passed": False, "reuse_passed": False}
        passed: client.JsonObject = {"outcome": "evaluated", "passed": True, "reuse_passed": True}
        continued = reader.comparison(failed, passed, observation, [peer], target_check=passed)
        self.assertTrue(continued["first_to_successor_objective_gain"])
        self.assertTrue(continued["exact_peer_artifact_derivation"])
        self.assertFalse(continued["qualified_improvement"])
        self.assertFalse(continued["qualified_reader_repair"])
        repaired = reader.comparison(passed, passed, observation, [peer], target_check=failed)
        self.assertFalse(repaired["first_to_successor_objective_gain"])
        self.assertTrue(repaired["qualified_improvement"])
        self.assertTrue(repaired["qualified_reader_repair"])
        absent = {**observation, "target_event_id": "event:absent"}
        self.assertFalse(reader.comparison(failed, passed, absent, [peer], target_check=failed)["qualified_improvement"])
        stopped: client.JsonObject = {"outcome": "not_authored", "passed": False}
        self.assertFalse(reader.comparison(stopped, passed, observation, [peer], target_check=stopped)["qualified_improvement"])
        self.assertFalse(reader.comparison(stopped, passed, observation, [peer], target_check=stopped)["first_to_successor_objective_gain"])

    def test_challenge_receipts_bind_kind_author_content_and_assigned_revision(self) -> None:
        event = copy.deepcopy(fixtures()[0])
        event["world"] = reader.GRADE_WORLD
        record = obj(obj(event["body"])["artifact_revision"])
        record["world"] = reader.GRADE_WORLD
        posted = copy.deepcopy(record)
        posted.pop("revision")
        receipt: client.JsonObject = {"status": "recorded", "world": reader.GRADE_WORLD,
            "record_id": posted["id"], "event_id": event["id"], "sequence": event["sequence"], "revision": record["revision"]}
        reader.verify_challenge([event], [(posted, receipt)])
        for field, wrong in (("kind", "message.recorded"), ("actor", "agent:wrong"), ("sequence", 999)):
            altered = {**event, field: wrong}
            with self.assertRaises(ValueError):
                reader.verify_challenge([altered], [(posted, receipt)])
        for field, wrong in (("revision", 999), ("status", "rejected"), ("world", "wrong")):
            with self.assertRaises(ValueError):
                reader.verify_challenge([event], [(posted, {**receipt, field: wrong})])
        with self.assertRaises(ValueError):
            reader.verify_challenge([], [(posted, receipt)])


class ReaderTransportTests(unittest.TestCase):
    def test_private_native_configuration_and_exact_original_export(self) -> None:
        events = fixtures()
        originals = [json.dumps(event, indent=1) for event in events]
        paths: list[Path] = []

        def native(command: list[str], **unused: object) -> subprocess.CompletedProcess[bytes]:
            path = Path(command[2])
            paths.append(path)
            configuration = client.decode(path.read_bytes())
            self.assertEqual(configuration["token"], "secret-fixture-token")
            self.assertEqual(configuration["traversal"], "first_page")
            self.assertFalse(path.is_relative_to(reader.ROOT))
            payload = {"snapshot": {"world": events[0]["world"], "records": originals},
                       "report": {**transport(events, end=False), "pages": 1}}
            return subprocess.CompletedProcess(command, 0, json.dumps(payload).encode(), b"")

        with tempfile.TemporaryDirectory() as temporary, patch.object(subprocess, "run", side_effect=native):
            archive = Path(temporary) / "originals.json"
            returned, report = reader.read_snapshot(Path("reader"), "http://127.0.0.1:1", "secret-fixture-token",
                str(events[0]["world"]), first=True, originals_path=archive)
            self.assertEqual(client.decode(archive.read_bytes())["records"], originals)
            self.assertEqual(returned, events)
            self.assertNotIn("secret-fixture-token", json.dumps(report))
        self.assertTrue(paths)
        self.assertTrue(all(not path.exists() for path in paths))

    def test_native_failure_invalid_report_and_credential_reflection_are_fixed(self) -> None:
        events = fixtures()
        base: client.JsonObject = {"snapshot": {"world": events[0]["world"], "records": [json.dumps(event) for event in events]},
                                   "report": transport(events)}
        variants: list[client.JsonObject] = [{}, {**base, "snapshot": {"world": "wrong", "records": []}},
            {**base, "snapshot": {"world": events[0]["world"],
                "records": [json.dumps({**event, "world": "civ:foreign"}) for event in events]}},
            {**base, "report": {**transport(events), "events": 999}},
            {**base, "report": {**transport(events), "pages": True}},
            {**base, "report": {**transport(events), "scope": "global"}},
            {**base, "report": {**transport(events), "copying_permission": "granted"}},
            {**base, "report": {**transport(events), "echo": "secret-fixture-token"}}]
        for value in variants:
            completed = subprocess.CompletedProcess(["reader"], 0, json.dumps(value).encode(), b"")
            with patch.object(subprocess, "run", return_value=completed), self.assertRaises((ValueError, client.DecisionError)):
                reader.read_snapshot(Path("reader"), "origin", "secret-fixture-token", str(events[0]["world"]))
        with patch.object(subprocess, "run", return_value=subprocess.CompletedProcess(["reader"], 2, b"secret", b"secret")):
            with self.assertRaisesRegex(ValueError, "^reader failed$"):
                reader.read_snapshot(Path("reader"), "origin", "token", "world")


class ReaderProviderTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory(prefix="agentciv-reader-provider-")
        self.directory = Path(self.temporary.name)
        self.server = HOST.start_server(HOST.HostConfig(world_id=reader.WORLD, title="Reader provider fixture",
            database_path=self.directory / "world.sqlite", listen=("127.0.0.1", 0), visibility="members",
            retention_seconds=60, max_payload_bytes=16384, credentials=(
                HOST.Credential(reader.GUIDE, "guide-fixture-token", True, True),
                HOST.Credential(reader.AUTHORS[2], "writer-fixture-token", True, True))))
        self.origin: str = self.server.origin
        source = copy.deepcopy(client.object_value(client.object_value(fixtures()[3]["body"])["artifact_revision"]))
        source.update({"world": reader.WORLD, "from": reader.GUIDE, "to": [reader.AUTHORS[2]]})
        source.pop("revision")
        source.pop("derived_from", None)
        mock.publish(self.origin, "guide-fixture-token", source)
        self.events = client.history(self.origin, "writer-fixture-token")
        self.valid = reader.scripted_decision(self.events, first=False)
        self.config: client.JsonObject = {"origin": self.origin, "principal": reader.AUTHORS[2],
            "token": "writer-fixture-token", "recipients": [reader.GUIDE], "record_id": "submission:provider-fixture",
            "mode": "ollama", "model": "fixture:8b", "seed": 42, "decision_attempts": 2,
            "attempt_journal": str(self.directory / "attempts.json")}
        self.provider: FakeOllama | None = None

    def tearDown(self) -> None:
        if self.provider is not None:
            self.provider.close()
        self.server.shutdown()
        self.server.server_close()
        self.temporary.cleanup()

    def serve(self, replies: list[client.JsonObject | None]) -> None:
        self.provider = FakeOllama(replies)
        self.config["ollama_origin"] = self.provider.origin

    def test_invalid_then_valid_preserves_frozen_sources_shared_budget_and_one_publication(self) -> None:
        self.serve([response({**self.valid, "target_event_id": "event:invented"}, 250), response(self.valid, 100)])
        result = reader.participant(self.config)
        self.assertEqual(result["outcome"], "recorded")
        self.assertEqual(result["decision"], self.valid)
        current = client.history(self.origin, "writer-fixture-token")
        self.assertEqual(current[:-1], self.events)
        stored = client.artifact(current[-1])
        if stored is None or self.provider is None:
            raise AssertionError("publication/provider missing")
        self.assertEqual(client.object_value(stored["body"])["reader_plan"], self.valid["plan"])
        self.assertEqual(len(self.provider.requests), 2)
        self.assertEqual(self.provider.requests[0]["format"], self.provider.requests[1]["format"])
        self.assertEqual(client.object_value(self.provider.requests[0]["options"])["num_predict"], 1024)
        self.assertEqual(client.object_value(self.provider.requests[1]["options"])["num_predict"], 774)
        self.assertNotIn("expected_event_ids", json.dumps(self.provider.requests))
        provider = client.object_value(result["provider"])
        self.assertEqual(provider["calls"], 2)
        self.assertEqual(client.object_value(provider["metrics"])["eval_count"], 100)
        self.assertTrue(client.object_value(result["source_check"])["passed"])

    def test_wrong_plan_is_published_without_oracle_feedback_or_repair(self) -> None:
        wrong = copy.deepcopy(self.valid)
        obj(plan_of(wrong)["runbook"])["execute_artifacts"] = True
        self.serve([response(wrong), response(self.valid)])
        result = reader.participant(self.config)
        self.assertEqual(result["outcome"], "recorded")
        self.assertEqual(result["decision"], wrong)
        self.assertFalse(client.object_value(result["source_check"])["passed"])
        if self.provider is None:
            raise AssertionError("provider missing")
        self.assertEqual(len(self.provider.requests), 1)

    def test_provider_failure_and_reflected_token_never_become_stop(self) -> None:
        self.serve([None, response(self.valid)])
        result = reader.participant(self.config)
        self.assertEqual(result["outcome"], "failed")
        self.assertEqual(result["failure_stage"], "provider")
        self.assertNotIn("decision", result)
        self.assertEqual(client.history(self.origin, "writer-fixture-token"), self.events)
        self.assertNotIn("writer-fixture-token", json.dumps(result))

    def test_stop_after_invalid_attempt_retains_failure_evidence_without_publication(self) -> None:
        stopped: client.JsonObject = {"action": "stop", "text": "", "target_event_id": "", "source_event_ids": [], "plan": None}
        self.serve([response({**self.valid, "target_event_id": "event:invented"}, 250), response(stopped, 100)])
        result = reader.participant(self.config)
        self.assertEqual(result["outcome"], "stopped")
        self.assertEqual(client.history(self.origin, "writer-fixture-token"), self.events)
        self.assertEqual(client.object_value(result["decision_loop"])["charged_output_tokens"], 350)
        self.assertNotIn("record", result)

    def test_reflected_credential_and_early_configuration_errors_are_not_exported(self) -> None:
        self.serve([response({**self.valid, "text": "writer-fixture-token"})])
        result = reader.participant(self.config)
        self.assertEqual(result["outcome"], "failed")
        self.assertNotIn("writer-fixture-token", json.dumps(result))
        self.assertEqual(client.history(self.origin, "writer-fixture-token"), self.events)
        self.assertEqual(reader.participant({})["failure_stage"], "configuration")

    def test_escaped_credential_is_rejected_before_publication_or_public_journaling(self) -> None:
        reply = response({**self.valid, "text": "writer-fixture-token"})
        content = str(obj(reply["message"])["content"]).replace("writer-fixture-token", "\\u0077riter-fixture-token")
        self.assertNotIn("writer-fixture-token", content)
        reply["message"] = {"content": content}
        self.serve([reply])
        result = reader.participant(self.config)
        self.assertEqual(result["outcome"], "failed")
        self.assertNotIn("decision", result)
        self.assertNotIn("writer-fixture-token", json.dumps(result))
        self.assertEqual(client.history(self.origin, "writer-fixture-token"), self.events)
        journal = client.decode((self.directory / "attempts.json").read_bytes())
        self.assertNotIn("writer-fixture-token", json.dumps(journal))
        self.assertEqual(obj(journal["publication"])["outcome"], "not_attempted")
        if self.provider is None:
            raise AssertionError("provider missing")
        self.assertEqual(len(self.provider.requests), 1)

    def test_malformed_candidate_still_receives_bounded_structural_feedback(self) -> None:
        malformed = response(self.valid)
        malformed["message"] = {"content": "{"}
        self.serve([malformed, response(self.valid)])
        result = reader.participant(self.config)
        self.assertEqual(result["outcome"], "recorded")
        self.assertEqual(result["decision"], self.valid)
        summary = obj(result["decision_loop"])
        attempts = summary["attempts"]
        if not isinstance(attempts, list) or self.provider is None:
            raise AssertionError("attempts/provider missing")
        self.assertEqual(obj(attempts[0])["feedback_code"], "invalid_json")
        self.assertEqual(obj(attempts[1])["outcome"], "valid")
        self.assertEqual(summary["charged_output_tokens"], 200)
        self.assertEqual(len(self.provider.requests), 2)

    def test_provider_metadata_version_and_metrics_reflection_never_reach_public_result(self) -> None:
        for field in ("metadata", "version", "metrics"):
            with self.subTest(field=field):
                if self.provider is not None:
                    self.provider.close()
                reply = response(self.valid)
                if field == "metrics":
                    reply["total_duration"] = "writer-fixture-token"
                self.serve([reply])
                if self.provider is None:
                    raise AssertionError("provider missing")
                handler_type = self.provider.server.RequestHandlerClass
                original_respond = getattr(handler_type, "respond")

                def reflected(handler: BaseHTTPRequestHandler, status: int, body: client.JsonObject) -> None:
                    if field == "metadata" and handler.path == "/api/tags":
                        body["models"] = [{"name": "fixture:8b", "extra": {"nested": ["writer-fixture-token"]}}]
                    if field == "version" and handler.path == "/api/version":
                        body["version"] = "writer-fixture-token"
                    original_respond(handler, status, body)

                journal = self.directory / f"reflection-{field}.json"
                candidates = self.directory / f"private-{field}"
                self.config.update({"attempt_journal": str(journal), "private_candidates": str(candidates)})
                with patch.object(handler_type, "respond", reflected):
                    result = reader.participant(self.config)
                self.assertEqual(result["outcome"], "failed")
                self.assertEqual(result["failure_stage"], "provider")
                if field == "metrics":
                    self.assertEqual(obj(result["provider"])["metrics"], {})
                    self.assertEqual(obj(result["provider"])["calls"], 1)
                else:
                    self.assertNotIn("provider", result)
                self.assertNotIn("decision", result)
                self.assertNotIn("writer-fixture-token", json.dumps(result))
                self.assertEqual(client.history(self.origin, "writer-fixture-token"), self.events)
                self.assertFalse(list(candidates.glob("*.json")))
                if journal.exists():
                    self.assertNotIn("writer-fixture-token", json.dumps(client.decode(journal.read_bytes())))
                self.assertEqual(len(self.provider.requests), 1 if field == "metrics" else 0)


class ReaderProcessTests(unittest.TestCase):
    reader_binary: Path
    host_binary: Path
    @classmethod
    def setUpClass(cls) -> None:
        binaries = walk.build_rust_binaries("agentciv-reader", "agentciv-host")
        cls.reader_binary = binaries["agentciv-reader"]
        cls.host_binary = binaries["agentciv-host"]

    def execute(self, output: Path, *, host: str = "python") -> client.JsonObject:
        return reader.run(output, host=host, reader_binary=self.reader_binary, host_binary=self.host_binary)

    def test_scripted_originals_and_successor_grade_same_actual_two_page_world_on_both_hosts(self) -> None:
        for host in ("python", "rust"):
            with self.subTest(host=host), tempfile.TemporaryDirectory(prefix="agentciv-reader-process-") as temporary, \
                    patch.object(reader, "source_identity", return_value={"fixture_identity": True}):
                output = Path(temporary) / "run"
                report = self.execute(output, host=host)
                self.assertEqual(report.get("failure_stage"), None, report)
                self.assertEqual(report["outcome"], "completed")
                self.assertTrue(report["useful_result"])
                self.assertFalse(report["qualified_improvement"])
                self.assertTrue(report["first_to_successor_objective_gain"])
                self.assertFalse(report["qualified_reader_repair"])
                self.assertFalse(report["model_improvement_observed"])
                self.assertTrue(report["exact_peer_artifact_derivation"])
                self.assertTrue(report["original_author_grants_revoked"])
                self.assertTrue(report["prior_records_retained"])
                self.assertEqual(report["challenge_event_count"], 106)
                self.assertEqual(report["challenge_pages"], 2)
                checks = report["checks"]
                if not isinstance(checks, list):
                    raise AssertionError("checks missing")
                initial, successor = client.object_value(checks[0]), client.object_value(checks[2])
                self.assertEqual(client.object_value(initial["transport"])["pages"], 1)
                self.assertFalse(client.object_value(initial["transport"])["reached_end"])
                self.assertEqual(len(mock.list_strings(initial["retrieved_event_ids"])), 100)
                self.assertEqual(client.object_value(successor["transport"])["pages"], 2)
                self.assertEqual(len(mock.list_strings(successor["retrieved_event_ids"])), 106)
                self.assertTrue(successor["passed"])
                self.assertEqual(len(mock.list_strings(successor["retained_objection_ids"])), 1)
                self.assertEqual(len(mock.list_strings(successor["retained_decline_ids"])), 1)
                archive = client.decode((output / "challenge-originals.json").read_bytes())
                self.assertEqual(len(mock.list_strings(archive["records"])), 106)
                self.assertEqual(archive["copying_condition"], "operator_authorized_synthetic_fixture_export_only")
                self.assertFalse(list(output.glob("*.sqlite")))
                self.assertFalse(list(output.glob("participant-*.json")))
                self.assertEqual(client.decode((output / "report.json").read_bytes()), report)

    def test_late_cleanup_failure_vetoes_success_and_preserves_completed_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "run"
            actual = walk.stop_host
            calls = 0

            def fail_after_shutdown(process: walk.RunningHost) -> str:
                nonlocal calls
                calls += 1
                result = actual(process)
                if calls == 3:
                    raise walk.WalkFailure("private-error-secret")
                return result

            with patch.object(reader, "source_identity", return_value={"fixture_identity": True}), \
                    patch.object(walk, "stop_host", side_effect=fail_after_shutdown):
                report = self.execute(output)
            self.assertEqual(report["outcome"], "failed")
            self.assertFalse(report["useful_result"])
            self.assertFalse(report["qualified_improvement"])
            self.assertFalse(report["objective_improvement"])
            self.assertTrue((output / "checks.json").exists())
            self.assertTrue((output / "history.json").exists())
            self.assertNotIn("private-error-secret", json.dumps(report))

    def test_final_exact_span_change_vetoes_an_otherwise_passing_run(self) -> None:
        actual = reader.read_snapshot
        calls = 0

        def changed_last_read(binary: Path, origin: str, token: str, world: str, *, first: bool = False,
                              originals_path: Path | None = None) -> tuple[list[client.JsonObject], client.JsonObject]:
            nonlocal calls
            events, report = actual(binary, origin, token, world, first=first, originals_path=originals_path)
            if world == reader.GRADE_WORLD and originals_path is None and not first:
                # Each full graded read has the same hash, but the final full read changes.
                calls += 1
                if calls == 3:
                    report["original_representation_sha256"] = "changed-exact-spans"
            return events, report

        with tempfile.TemporaryDirectory() as temporary, \
                patch.object(reader, "source_identity", return_value={"fixture_identity": True}), \
                patch.object(reader, "read_snapshot", side_effect=changed_last_read):
            report = self.execute(Path(temporary) / "run")
        self.assertEqual(report["outcome"], "failed")
        self.assertEqual(report["failure_stage"], "frozen_challenge")
        self.assertFalse(report["useful_result"])
        self.assertFalse(report["qualified_improvement"])

    def test_source_drift_vetoes_completed_trials(self) -> None:
        with tempfile.TemporaryDirectory() as temporary, patch.object(reader, "source_identity", side_effect=[{"sha": "before"}, {"sha": "after"}]):
            report = self.execute(Path(temporary) / "run")
        self.assertTrue(report["source_changed_during_run"])
        self.assertFalse(report["useful_result"])
        self.assertFalse(report["qualified_improvement"])
        self.assertEqual(report["failure_stage"], "source_identity")

    def test_failed_child_preserves_accepted_original_and_failed_observation(self) -> None:
        actual = subprocess.run

        def fail_second(command: list[str], *, cwd: Path | None = None, capture_output: bool,
                        timeout: float, check: bool) -> subprocess.CompletedProcess[bytes]:
            if "--config" in command and command[-1].endswith("participant-b.json"):
                failed = {"outcome": "failed", "failure_stage": "provider", "failure_code": "stage_failed"}
                return subprocess.CompletedProcess(command, 1, json.dumps(failed).encode(), b"private-provider-error")
            return actual(command, cwd=cwd, capture_output=capture_output, timeout=timeout, check=check)

        with tempfile.TemporaryDirectory() as temporary, patch.object(reader, "source_identity", return_value={"fixture_identity": True}), \
                patch.object(subprocess, "run", side_effect=fail_second):
            output = Path(temporary) / "run"
            report = self.execute(output)
            self.assertEqual(report["outcome"], "failed")
            observations = report["observations"]
            if not isinstance(observations, list):
                raise AssertionError("observations missing")
            self.assertEqual(len(observations), 2)
            self.assertEqual(obj(observations[0])["outcome"], "recorded")
            self.assertEqual(obj(observations[1])["failure_stage"], "provider")
            history = json.loads((output / "history.json").read_bytes())
            self.assertTrue(any(event.get("actor") == reader.AUTHORS[0] for event in history))
            self.assertNotIn("private-provider-error", json.dumps(report))

    def test_failed_build_and_configuration_keep_manifest_and_never_overwrite(self) -> None:
        with tempfile.TemporaryDirectory() as temporary, patch.object(reader, "source_identity", return_value={"fixture_identity": True}):
            output = Path(temporary) / "run"
            with patch.object(walk, "build_rust_binaries", side_effect=subprocess.CalledProcessError(1, "private-path")):
                failed = reader.run(output, mode="ollama", model="installed-fixture", seed=77)
            self.assertEqual(failed["failure_stage"], "native_build")
            self.assertEqual(failed["source"], {"fixture_identity": True})
            self.assertEqual(client.object_value(failed["controls"])["seed"], 77)
            self.assertNotIn("private-path", json.dumps(failed))
            existing = (output / "report.json").read_bytes()
            with self.assertRaises(FileExistsError):
                self.execute(output)
            self.assertEqual((output / "report.json").read_bytes(), existing)
            invalid = reader.run(Path(temporary) / "invalid", host="unknown")
            self.assertEqual(invalid["failure_stage"], "configuration")


if __name__ == "__main__":
    unittest.main()
