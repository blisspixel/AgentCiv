"""Negative evidence cases independent of native processes and current access."""

from __future__ import annotations

import copy
import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import collaboration as client  # noqa: E402
import reader_collaboration as reader  # noqa: E402
import reader_evidence as evidence  # noqa: E402
import oracle  # noqa: E402

JsonObject = dict[str, object]


def obj(value: object) -> JsonObject:
    return client.object_value(value)


def rows(value: object) -> list[JsonObject]:
    if not isinstance(value, list):
        raise AssertionError("expected object list")
    return [obj(item) for item in value]


def originals(events: list[JsonObject]) -> list[str]:
    return [json.dumps(event, ensure_ascii=False) for event in events]


def transport(events: list[JsonObject], *, complete: bool = True) -> JsonObject:
    raw = originals(events)
    return {"pages": max(1, (len(events) + 99) // 100), "events": len(events), "reached_end": complete,
        "scope": "current_caller_view", "copying_permission": "not_granted",
        "original_record_sha256": evidence.hashes(raw),
        "original_representation_sha256": evidence.representations_hash(raw)}


def package(*, repair: bool = True) -> tuple[JsonObject, list[JsonObject], list[JsonObject]]:
    seeds = oracle.validate_events(oracle.load_json(reader.ROOT / "examples/inheritance/history.json"))
    study: list[JsonObject] = []
    observations: list[JsonObject] = []

    def note(index: int) -> None:
        event = copy.deepcopy(seeds[index])
        event.update({"world": reader.WORLD, "sequence": len(study) + 1, "actor": reader.GUIDE})
        record = obj(obj(event["body"])["artifact_revision"])
        record.update({"world": reader.WORLD, "from": reader.GUIDE})
        if index == 3:
            record["derived_from"] = {"from": reader.GUIDE, "artifact_id": record["artifact_id"], "revision": 1}
        event["body"] = {"artifact_revision": record}
        study.append(event)

    note(1)
    for index, principal in enumerate(reader.AUTHORS):
        if index == 1:
            note(3)
        decision = reader.scripted_decision(study, first=index == 0, partial=repair and index == 1)
        record = client.submission(discovery={"id": reader.WORLD}, principal=principal,
            recipients=[reader.OBSERVER], record_id=f"submission:reader-{index}",
            decision={key: decision[key] for key in ("action", "text", "target_event_id", "source_event_ids")},
            events=study, mode="scripted")
        if record is None:
            raise AssertionError("scripted fixture unexpectedly stopped")
        record.update({"artifact_id": "artifact:history-reader", "media_type": "application/json"})
        record["body"] = {**obj(record["body"]), "reader_plan": decision["plan"]}
        event_id = f"event:published-{index}"
        event: JsonObject = {"id": event_id, "world": reader.WORLD, "sequence": len(study) + 1,
            "kind": "artifact.recorded", "actor": principal,
            "body": {"artifact_revision": {**record, "revision": 1}}}
        study.append(event)
        observations.append({"principal": principal, "outcome": "recorded", "decision_source": "scripted", "decision": decision,
            "record": record, "receipt": {"status": "recorded", "record_id": record["id"], "world": reader.WORLD,
                "event_id": event_id, "sequence": event["sequence"], "revision": 1},
            "target_event_id": decision["target_event_id"], "derived_from": record.get("derived_from"),
            "peer_artifact_citations": [decision["target_event_id"]] if decision["target_event_id"] else []})
        if repair and index < 2:
            objection: JsonObject = {"from": reader.GUIDE, "target_from": principal, "type": "objection", "world": reader.WORLD,
                "artifact_id": record["artifact_id"], "revision": 1,
                "body": {"source_event_ids": [event_id, "event:notes-1" if index == 0 else "event:notes-2"]}}
            study.append({"id": f"event:objection-{index}", "world": reader.WORLD, "sequence": len(study) + 1,
                "kind": "objection.recorded", "actor": reader.GUIDE, "body": {"objection": objection}})
    challenge: list[JsonObject] = [{"id": f"event:padding-{index}", "world": reader.GRADE_WORLD,
        "sequence": index + 1, "kind": "message.recorded", "actor": "agent:a", "body": {}} for index in range(100)]
    for seed in seeds:
        event = copy.deepcopy(seed)
        event.update({"world": reader.GRADE_WORLD, "sequence": len(challenge) + 1})
        key = {"artifact.recorded": "artifact_revision", "objection.recorded": "objection", "decline.recorded": "decline"}[str(event["kind"])]
        event["body"] = {key: {**obj(obj(event["body"])[key]), "world": reader.GRADE_WORLD}}
        challenge.append(event)
    checks: list[JsonObject] = []
    for observation in observations:
        first = obj(obj(obj(observation["decision"])["plan"])["reader"])["pagination"] == "first"
        returned = challenge[:100] if first else challenge
        with patch.object(reader, "read_snapshot", return_value=(returned, transport(returned, complete=not first))):
            checks.append(reader.grade(Path("unused"), "http://127.0.0.1:1", "private-token", challenge, study,
                observation["decision"], expected_original_hashes=dict(zip(
                    (str(event["id"]) for event in challenge), evidence.hashes(originals(challenge)), strict=True))))
    report: JsonObject = {"format": "agentciv-paged-reader-experiment/0.1", "outcome": "completed",
        "mode": "scripted", "condition": "repair" if repair else "continuation",
        "controls": {"choice_source": "scripted", "condition": "repair" if repair else "continuation"},
        "source_changed_during_run": False, "source": {"repository_commit": "a" * 40,
            "source_working_tree_dirty": False, "source_sha256": {"fixture.py": "b" * 64}},
        "observations": observations, "checks": checks, "useful_result": True,
        "study_transport": transport(study), "study_revalidation_transport": transport(study),
        "challenge_transport": transport(challenge), "challenge_event_count": 106, "challenge_pages": 2,
        "challenge_unchanged": True,
        "challenge_sha256": hashlib.sha256(json.dumps(challenge, sort_keys=True).encode("utf-8")).hexdigest(),
        "challenge_original_representation_sha256": evidence.representations_hash(originals(challenge))}
    report.update(reader.comparison(checks[0], checks[2], observations[2], study, target_check=checks[1]))
    report.update({"successor_derived_from": observations[2]["derived_from"],
        "successor_peer_artifact_citations": observations[2]["peer_artifact_citations"],
        "model_improvement_observed": False, "study_revalidated": True,
        "unresolved_objection_event_ids": [event["id"] for event in study if event["kind"] == "objection.recorded"]})
    return report, study, challenge


def set_mode(report: JsonObject, study: list[JsonObject], mode: str) -> None:
    """Declare a consistent synthetic condition without claiming model execution."""
    report["mode"] = mode
    report["controls"] = {**obj(report["controls"]), "choice_source": mode}
    observations = rows(report["observations"])
    for observation in observations:
        observation["decision_source"] = mode
        if "record" not in observation:
            continue
        submitted = obj(observation["record"])
        submitted["experiment_provenance"] = {**obj(submitted["experiment_provenance"]), "decision_source": mode}
        observation["record"] = submitted
        event = next(event for event in study if event["id"] == obj(observation["receipt"])["event_id"])
        event["body"] = {"artifact_revision": {**submitted, "revision": obj(observation["receipt"])["revision"]}}
    report["observations"] = observations
    report["study_transport"] = transport(study)
    report["study_revalidation_transport"] = transport(study)
    report["model_improvement_observed"] = mode == "ollama" and report["qualified_improvement"] is True


def save(directory: Path, report: JsonObject, study: list[JsonObject], challenge: list[JsonObject]) -> None:
    (directory / "report.json").write_text(json.dumps(report), encoding="utf-8")
    for name, world, records in (("study", reader.WORLD, study), ("challenge", reader.GRADE_WORLD, challenge)):
        (directory / f"{name}-originals.json").write_text(json.dumps({"world": world, "records": originals(records),
            "copying_condition": evidence.COPYING}), encoding="utf-8")


class ReaderEvidenceTests(unittest.TestCase):
    def test_retained_packages_match_published_digests_and_reproduce(self) -> None:
        directory = reader.ROOT / "conformance/evidence/reader-repair-2026-10-08"
        manifest = client.decode((directory / "manifest.json").read_bytes())
        allowed = {f"{host}/{name}" for host in ("python", "rust")
            for name in ("report.json", "study-originals.json", "challenge-originals.json")}
        entries = rows(manifest["entries"])
        self.assertEqual(len(entries), len(allowed))
        self.assertEqual({str(entry["path"]) for entry in entries}, allowed)
        for entry in entries:
            path = str(entry["path"])
            self.assertIn(path, allowed)
            self.assertEqual(hashlib.sha256((directory / path).read_bytes()).hexdigest(), entry["published_sha256"])
        for host in ("python", "rust"):
            with self.subTest(host=host):
                result = evidence.inspect_package(directory / host)
                self.assertTrue(result["passed"], result)
                self.assertTrue(result["qualified_reader_repair"])

    def inspect(self, report: JsonObject, study: list[JsonObject], challenge: list[JsonObject]) -> JsonObject:
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            save(directory, report, study, challenge)
            return evidence.inspect_package(directory)

    def test_recomputes_repair_and_keeps_passing_parent_continuation_distinct(self) -> None:
        for repair in (False, True):
            with self.subTest(repair=repair):
                result = self.inspect(*package(repair=repair))
                self.assertTrue(result["passed"], result)
                self.assertTrue(result["useful_result"])
                self.assertTrue(result["first_to_successor_objective_gain"])
                self.assertEqual(result["qualified_improvement"], repair)
                self.assertEqual(result["qualified_reader_repair"], repair)
                self.assertEqual(result["current_access_and_history_freshness"], "not_established")

    def test_asserted_success_does_not_replace_reproduction(self) -> None:
        report, study, challenge = package(repair=False)
        report["qualified_improvement"] = True
        self.assertEqual(self.inspect(report, study, challenge)["code"], "stale_report")
        report, study, challenge = package()
        checks = rows(report["checks"])
        checks[1]["passed"] = True
        report["checks"] = checks
        self.assertEqual(self.inspect(report, study, challenge)["code"], "stale_report")

    def test_exact_representation_and_missing_record_checks(self) -> None:
        report, study, challenge = package()
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            save(directory, report, study, challenge)
            path = directory / "challenge-originals.json"
            value = obj(json.loads(path.read_text(encoding="utf-8")))
            raw = list(originals(challenge))
            raw[0] = " " + raw[0]
            value["records"] = raw
            path.write_text(json.dumps(value), encoding="utf-8")
            self.assertEqual(evidence.inspect_package(directory)["code"], "originals_mismatch")
        self.assertEqual(self.inspect(report, study[:-1], challenge)["code"], "incomplete_traversal")

    def test_missing_and_withdrawn_sources_cannot_be_established(self) -> None:
        for withdrawn in (False, True):
            report, study, challenge = package()
            source_index = next(index for index, event in enumerate(study) if event["id"] == "event:notes-2")
            if withdrawn:
                study[source_index] = {**study[source_index], "kind": "artifact.withdrawn", "body": {}}
            else:
                study.pop(source_index)
            report["study_transport"] = transport(study)
            report["study_revalidation_transport"] = transport(study)
            # Model conditions can legitimately omit the deterministic repair story.
            set_mode(report, study, "ollama")
            result = self.inspect(report, study, challenge)
            self.assertEqual(result["code"], "source_withdrawn" if withdrawn else "source_unavailable", result)

    def test_partial_traversal_and_unchecked_revalidation_are_rejected(self) -> None:
        for name in ("study_transport", "study_revalidation_transport", "challenge_transport"):
            report, study, challenge = package()
            changed = obj(report[name])
            changed["reached_end"] = False
            report[name] = changed
            self.assertEqual(self.inspect(report, study, challenge)["code"], "incomplete_traversal")

    def test_publication_revision_and_plan_must_match_originals(self) -> None:
        for field, wrong in (("revision", 2), ("sequence", 999), ("status", "accepted")):
            report, study, challenge = package()
            observations = rows(report["observations"])
            receipt = obj(observations[2]["receipt"])
            receipt[field] = wrong
            observations[2]["receipt"] = receipt
            report["observations"] = observations
            self.assertEqual(self.inspect(report, study, challenge)["code"], "publication_mismatch")
        report, study, challenge = package()
        observations = rows(report["observations"])
        decision = copy.deepcopy(obj(observations[2]["decision"]))
        plan = obj(decision["plan"])
        plan["claims"] = []
        decision["plan"] = plan
        observations[2]["decision"] = decision
        report["observations"] = observations
        self.assertEqual(self.inspect(report, study, challenge)["code"], "publication_mismatch")
        report, study, challenge = package()
        study[-1]["kind"] = "message.recorded"
        report.update({"study_transport": transport(study), "study_revalidation_transport": transport(study)})
        self.assertEqual(self.inspect(report, study, challenge)["code"], "publication_mismatch")

    def test_originals_are_not_permission_and_current_access_is_not_inferred(self) -> None:
        report, study, challenge = package()
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            save(directory, report, study, challenge)
            path = directory / "study-originals.json"
            value = obj(json.loads(path.read_text(encoding="utf-8")))
            value["copying_condition"] = "reader_granted_access"
            path.write_text(json.dumps(value), encoding="utf-8")
            self.assertEqual(evidence.inspect_package(directory)["code"], "copying_condition_unestablished")

    def test_allowlisted_reads_never_include_private_content_in_failures(self) -> None:
        report, study, challenge = package()
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            save(directory, report, study, challenge)
            (directory / "private-a.json").write_text("secret-private-trace", encoding="utf-8")
            self.assertTrue(evidence.inspect_package(directory)["passed"])
            (directory / "report.json").write_text('{"private":"secret-private-trace","private":NaN}', encoding="utf-8")
            result = evidence.inspect_package(directory)
            self.assertEqual(result["code"], "malformed_package")
            self.assertNotIn("secret-private-trace", json.dumps(result))
            (directory / "report.json").unlink()
            self.assertEqual(evidence.inspect_package(directory)["code"], "missing_evidence")

    def test_bounds_wrong_world_and_noncanonical_boolean_are_rejected(self) -> None:
        report, study, challenge = package()
        report["qualified_improvement"] = 1
        self.assertEqual(self.inspect(report, study, challenge)["code"], "stale_report")
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            save(directory, *package())
            (directory / "report.json").write_bytes(b" " * (evidence.MAX_FILE_BYTES + 1))
            self.assertEqual(evidence.inspect_package(directory)["code"], "package_too_large")

    def test_material_report_aliases_cannot_contradict_evidence(self) -> None:
        for field, wrong in (("peer_target_check", {}), ("successor_derived_from", {}),
                             ("successor_peer_artifact_citations", []), ("model_improvement_observed", True),
                             ("study_revalidated", False), ("unresolved_objection_event_ids", [])):
            with self.subTest(field=field):
                report, study, challenge = package()
                report[field] = wrong
                self.assertEqual(self.inspect(report, study, challenge)["code"], "stale_report")

    def test_missing_objection_story_is_not_a_scripted_repair_package(self) -> None:
        report, study, challenge = package()
        study = [event for event in study if event["kind"] != "objection.recorded"]
        report.update({"study_transport": transport(study), "study_revalidation_transport": transport(study),
                       "unresolved_objection_event_ids": []})
        self.assertEqual(self.inspect(report, study, challenge)["code"], "repair_evidence_unavailable")
        report["condition"] = "unknown"
        self.assertEqual(self.inspect(report, study, challenge)["code"], "configuration_unestablished")

    def test_future_citations_and_unrelated_peer_lists_are_not_exact_derivation(self) -> None:
        report, study, challenge = package()
        observations = rows(report["observations"])
        observations[2]["peer_artifact_citations"] = ["event:published-0", "event:published-1"]
        report["observations"] = observations
        self.assertEqual(self.inspect(report, study, challenge)["code"], "publication_mismatch")
        report, study, challenge = package()
        observations = rows(report["observations"])
        decision = obj(observations[0]["decision"])
        decision["source_event_ids"] = ["event:notes-2"]
        observations[0]["decision"] = decision
        submitted = obj(observations[0]["record"])
        submitted["body"] = {**obj(submitted["body"]), "source_event_ids": ["event:notes-2"]}
        observations[0]["record"] = submitted
        study[1]["body"] = {"artifact_revision": {**submitted, "revision": 1}}
        report.update({"observations": observations, "study_transport": transport(study),
                       "study_revalidation_transport": transport(study)})
        self.assertEqual(self.inspect(report, study, challenge)["code"], "publication_mismatch")

    def test_a_model_stop_is_inspectable_without_becoming_a_repair(self) -> None:
        report, study, challenge = package()
        observations = rows(report["observations"])
        observations[2] = {"principal": reader.AUTHORS[2], "outcome": "stopped", "target_event_id": "",
            "peer_artifact_citations": [], "decision": {"action": "stop", "plan": None,
                "target_event_id": "", "source_event_ids": []}}
        checks = rows(report["checks"])
        checks[2] = {"passed": False, "outcome": "not_authored", "http_requests": 0}
        study.pop()
        report.update({"mode": "ollama", "observations": observations, "checks": checks,
            "study_transport": transport(study), "study_revalidation_transport": transport(study),
            "useful_result": False, "successor_derived_from": None, "successor_peer_artifact_citations": []})
        report.update(reader.comparison(checks[0], checks[2], observations[2], study, target_check=None))
        set_mode(report, study, "ollama")
        result = self.inspect(report, study, challenge)
        self.assertTrue(result["passed"], result)
        self.assertFalse(result["useful_result"])
        self.assertFalse(result["qualified_improvement"])
        for field, wrong in (("target_event_id", "event:published-1"), ("source_event_ids", ["event:published-1"]),
                             ("peer_artifact_citations", ["event:published-1"]), ("derived_from", {})):
            with self.subTest(malformed_stop=field):
                changed = copy.deepcopy(report)
                altered = rows(changed["observations"])
                if field in ("target_event_id", "source_event_ids"):
                    altered[2]["decision"] = {**obj(altered[2]["decision"]), field: wrong}
                else:
                    altered[2][field] = wrong
                changed["observations"] = altered
                self.assertEqual(self.inspect(changed, study, challenge)["code"], "publication_mismatch")

    def test_scripted_results_cannot_be_relabelled_as_model_observations(self) -> None:
        for mismatch in ("report", "controls", "observation", "published", "condition"):
            with self.subTest(mismatch=mismatch):
                report, study, challenge = package()
                if mismatch == "report":
                    report.update({"mode": "ollama", "model_improvement_observed": True})
                elif mismatch == "controls":
                    report["controls"] = {**obj(report["controls"]), "choice_source": "ollama"}
                elif mismatch == "condition":
                    report["condition"] = "continuation"
                elif mismatch == "observation":
                    observations = rows(report["observations"])
                    observations[2]["decision_source"] = "ollama"
                    report["observations"] = observations
                else:
                    record = obj(obj(study[-1]["body"])["artifact_revision"])
                    record["experiment_provenance"] = {"decision_source": "ollama"}
                    study[-1]["body"] = {"artifact_revision": record}
                    report["study_transport"] = transport(study)
                    report["study_revalidation_transport"] = transport(study)
                self.assertEqual(self.inspect(report, study, challenge)["code"], "provenance_mismatch")

    def test_nested_records_are_bound_to_event_identity_world_and_kind(self) -> None:
        for kind in ("artifact.recorded", "objection.recorded", "decline.recorded"):
            for field, wrong in (("from", "agent:impostor"), ("world", "civ:other"), ("type", "message")):
                with self.subTest(kind=kind, field=field):
                    report, study, challenge = package()
                    event = next(event for event in challenge if event["kind"] == kind)
                    key = {"artifact.recorded": "artifact_revision", "objection.recorded": "objection", "decline.recorded": "decline"}[kind]
                    record = obj(obj(event["body"])[key])
                    record[field] = wrong
                    event["body"] = {key: record}
                    self.assertEqual(self.inspect(report, study, challenge)["code"], "record_identity_mismatch")

    def test_missing_source_metadata_and_failed_experiment_remain_unestablished(self) -> None:
        for key, value, code in (("source_changed_during_run", True, "source_identity_unestablished"),
                                 ("outcome", "failed", "experiment_incomplete"),
                                 ("source", {"repository_commit": "not-a-commit", "source_sha256": {}},
                                  "source_identity_unestablished")):
            report, study, challenge = package()
            report[key] = value
            self.assertEqual(self.inspect(report, study, challenge)["code"], code)


if __name__ == "__main__":
    unittest.main()
