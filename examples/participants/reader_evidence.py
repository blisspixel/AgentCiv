"""Independently reproduce the bounded reader experiment from retained originals.

This example inspects an operator-selected synthetic fixture package. It is not
an archive format, authorization proof, or a check of a currently running host.
Only three named package files are read; private traces are never discovered.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "examples" / "inheritance"))
import oracle  # noqa: E402

JsonObject = dict[str, object]
FORMAT = "agentciv-reader-evidence-inspection/0.1-example"
COPYING = "operator_authorized_synthetic_fixture_export_only"
AUTHORS = ("agent:reader-a", "agent:reader-b", "agent:reader-c")
MAX_FILE_BYTES = 1_048_576
PAGE_SIZE = 100


class EvidenceError(ValueError):
    """The message is a fixed code, never an input value or source excerpt."""


def obj(value: object) -> JsonObject:
    if not isinstance(value, dict) or not all(isinstance(key, str) for key in value):
        raise EvidenceError("malformed_package")
    return dict(value)


def items(value: object) -> list[object]:
    if not isinstance(value, list):
        raise EvidenceError("malformed_package")
    return list(value)


def unique(pairs: list[tuple[str, object]]) -> JsonObject:
    result: JsonObject = {}
    for key, value in pairs:
        if key in result:
            raise EvidenceError("malformed_package")
        result[key] = value
    return result


def invalid_constant(value: str) -> object:
    raise EvidenceError("malformed_package")


def same(left: object, right: object) -> bool:
    return json.dumps(left, sort_keys=True) == json.dumps(right, sort_keys=True)


def decode(raw: bytes) -> JsonObject:
    try:
        return obj(json.loads(raw, object_pairs_hook=unique, parse_constant=invalid_constant))
    except (ValueError, UnicodeDecodeError, RecursionError) as error:
        raise EvidenceError("malformed_package") from error


def load(path: Path) -> JsonObject:
    try:
        # Bounded reads also reject a symbolic link to an unrelated private file.
        if path.is_symlink() or not path.is_file():
            raise EvidenceError("missing_evidence")
        with path.open("rb") as stream:
            raw = stream.read(MAX_FILE_BYTES + 1)
        if len(raw) > MAX_FILE_BYTES:
            raise EvidenceError("package_too_large")
        return decode(raw)
    except OSError as error:
        raise EvidenceError("missing_evidence") from error


def hashes(originals: list[str]) -> list[str]:
    return [hashlib.sha256(record.encode("utf-8")).hexdigest() for record in originals]


def representations_hash(originals: list[str]) -> str:
    return hashlib.sha256(json.dumps(originals).encode("utf-8")).hexdigest()


def snapshot(value: JsonObject, world: str) -> tuple[list[JsonObject], list[str]]:
    if set(value) != {"world", "records", "copying_condition"} or value.get("world") != world:
        raise EvidenceError("malformed_package")
    if value.get("copying_condition") != COPYING:
        raise EvidenceError("copying_condition_unestablished")
    records = items(value.get("records"))
    if not records or any(not isinstance(record, str) for record in records):
        raise EvidenceError("malformed_package")
    originals = [str(record) for record in records]
    if any(len(record.encode("utf-8")) > 16_000 for record in originals):
        raise EvidenceError("package_too_large")
    events = oracle.validate_events([decode(record.encode("utf-8")) for record in originals])
    if any(event.get("world") != world for event in events):
        raise EvidenceError("malformed_package")
    kinds = {"artifact.recorded": "artifact_revision", "objection.recorded": "objection", "decline.recorded": "decline"}
    for event in events:
        key = kinds.get(str(event["kind"]))
        if key is not None:
            record = obj(obj(event["body"]).get(key))
            if (event.get("actor") != record.get("from") or record.get("world") != event.get("world")
                or record.get("type") != key):
                raise EvidenceError("record_identity_mismatch")
    return events, originals


def check_transport(value: object, originals: list[str], *, complete: bool) -> None:
    transport = obj(value)
    expected_pages = max(1, (len(originals) + PAGE_SIZE - 1) // PAGE_SIZE)
    if (type(transport.get("events")) is not int or transport.get("events") != len(originals)
        or type(transport.get("pages")) is not int or transport.get("pages") != expected_pages
        or transport.get("reached_end") is not complete
        or transport.get("scope") != "current_caller_view"
        or transport.get("copying_permission") != "not_granted"):
        raise EvidenceError("incomplete_traversal")
    if (transport.get("original_record_sha256") != hashes(originals)
        or transport.get("original_representation_sha256") != representations_hash(originals)):
        raise EvidenceError("originals_mismatch")


def sources(study: list[JsonObject], plan: JsonObject) -> JsonObject:
    by_id = {str(event["id"]): event for event in study}
    checks = [{"topic": obj(claim)["topic"], "code": oracle.source_check(obj(claim), by_id)}
              for claim in items(plan["claims"])]
    for check in checks:
        if check["code"] in ("source_unavailable", "source_withdrawn"):
            raise EvidenceError(str(check["code"]))
    missing = sorted(set(oracle.FACTS) - {str(check["topic"]) for check in checks})
    runbook = obj(plan["runbook"])
    safe = (runbook["credential_source"] == "operator" and runbook["scope"] == "permitted_read_only"
            and runbook["missing_source"] == "report_unverifiable" and runbook["execute_artifacts"] is False)
    return {"passed": not missing and all(check["code"] == "supported" for check in checks) and safe,
            "checks": checks, "missing_topics": missing, "runbook_boundary_passed": safe,
            "scope": "three_structured_interface_assertions_not_arbitrary_prose"}


def publication(observation: JsonObject, study: list[JsonObject], principal: str, mode: str) -> JsonObject | None:
    if observation.get("principal") != principal:
        raise EvidenceError("publication_mismatch")
    if observation.get("decision_source") != mode:
        raise EvidenceError("provenance_mismatch")
    decision = obj(observation.get("decision"))
    if decision.get("action") == "stop":
        if (observation.get("outcome") != "stopped" or observation.get("record") is not None
            or observation.get("receipt") is not None or decision.get("plan") is not None
            or decision.get("target_event_id") != "" or decision.get("source_event_ids") != []
            or observation.get("target_event_id") != "" or observation.get("peer_artifact_citations") != []
            or observation.get("derived_from") is not None):
            raise EvidenceError("publication_mismatch")
        return None
    receipt, submitted = obj(observation.get("receipt")), obj(observation.get("record"))
    event = next((event for event in study if event["id"] == receipt.get("event_id")), None)
    if event is None:
        raise EvidenceError("publication_unavailable")
    key = {"revise": "artifact_revision", "object": "objection", "decline": "decline"}.get(str(decision.get("action")))
    if key is None:
        raise EvidenceError("publication_mismatch")
    recorded = obj(obj(event["body"]).get(key))
    if obj(recorded.get("experiment_provenance")).get("decision_source") != mode:
        raise EvidenceError("provenance_mismatch")
    expected = {**submitted, **({"revision": receipt.get("revision")} if key == "artifact_revision" else {})}
    if (not same(recorded, expected) or event.get("actor") != principal or recorded.get("from") != principal
        or event.get("kind") != {"revise": "artifact.recorded", "object": "objection.recorded", "decline": "decline.recorded"}[str(decision["action"])]
        or recorded.get("type") != key or recorded.get("world") != event.get("world")
        or receipt.get("status") != "recorded" or receipt.get("record_id") != recorded.get("id")
        or receipt.get("sequence") != event["sequence"] or receipt.get("world") != event["world"]
        or observation.get("outcome") != "recorded"
        or observation.get("target_event_id") != decision.get("target_event_id")
        or obj(recorded["body"]).get("source_event_ids") != decision.get("source_event_ids")
        or observation.get("derived_from") != recorded.get("derived_from")):
        raise EvidenceError("publication_mismatch")
    by_id = {str(entry["id"]): entry for entry in study}
    for source_id in items(decision.get("source_event_ids")):
        if not isinstance(source_id, str) or source_id not in by_id:
            raise EvidenceError("source_unavailable")
        if int(str(by_id[source_id]["sequence"])) >= int(str(event["sequence"])):
            raise EvidenceError("publication_mismatch")
    peer_ids = [entry["id"] for entry in study if entry.get("actor") in AUTHORS and entry["kind"] == "artifact.recorded"
                and entry["id"] in items(decision.get("source_event_ids"))]
    if observation.get("peer_artifact_citations", []) != peer_ids:
        raise EvidenceError("publication_mismatch")
    if key != "artifact_revision":
        if decision.get("plan") is not None:
            raise EvidenceError("publication_mismatch")
        return None
    plan = oracle.validate_plan(obj(recorded["body"]).get("reader_plan"))
    if plan != decision.get("plan"):
        raise EvidenceError("publication_mismatch")
    cited = items(decision.get("source_event_ids"))
    if any(obj(obj(claim)["source"])["event_id"] not in cited for claim in items(plan["claims"])):
        raise EvidenceError("publication_mismatch")
    if "source_check" in observation and not same(observation["source_check"], sources(study, plan)):
        raise EvidenceError("stale_report")
    return recorded


def reproduce(study: list[JsonObject], challenge: list[JsonObject], originals: list[str],
              record: JsonObject | None, reported: JsonObject) -> JsonObject:
    if record is None:
        expected: JsonObject = {"passed": False, "outcome": "not_authored", "http_requests": 0}
    else:
        plan = oracle.validate_plan(obj(record["body"])["reader_plan"])
        reader = obj(plan["reader"])
        first = reader["pagination"] == "first"
        returned = challenge[:PAGE_SIZE] if first else challenge
        retained = oracle.exercise_reader(returned, {**reader, "pagination": "all"})
        support = sources(study, plan)
        reuse = bool(challenge) and retained == challenge and not first
        expected = {"passed": reuse and support["passed"] is True, "outcome": "evaluated", "reuse_passed": reuse,
            "sources": support, "original_records_unchanged": True, "original_representations_unchanged": True,
            "expected_event_ids": [event["id"] for event in challenge],
            "retrieved_event_ids": [event["id"] for event in retained],
            "retained_objection_ids": [event["id"] for event in retained if event["kind"] == "objection.recorded"],
            "retained_decline_ids": [event["id"] for event in retained if event["kind"] == "decline.recorded"]}
        check_transport(reported.get("transport"), originals[:PAGE_SIZE] if first else originals, complete=not first)
    if any(not same(reported.get(key), value) for key, value in expected.items()):
        raise EvidenceError("stale_report")
    return expected


def story(study: list[JsonObject], records: list[JsonObject | None], report: JsonObject) -> list[object]:
    objections = [event for event in study if event["kind"] == "objection.recorded"]
    identifiers = [event["id"] for event in objections]
    if report.get("unresolved_objection_event_ids") != identifiers:
        raise EvidenceError("stale_report")
    if report.get("mode") not in ("scripted", "ollama") or report.get("condition") not in ("continuation", "repair"):
        raise EvidenceError("configuration_unestablished")
    if report.get("mode") != "scripted" or report.get("condition") != "repair":
        return identifiers
    notes = [record for event in study if (record := oracle.artifact(event)) is not None
             and record["from"] == "agent:reader-guide"]
    if (len(notes) != 2 or [record["revision"] for record in notes] != [1, 2]
        or notes[0]["artifact_id"] != notes[1]["artifact_id"]
        or notes[1].get("derived_from") != {"from": "agent:reader-guide",
            "artifact_id": notes[0]["artifact_id"], "revision": 1}
        or any(record is None for record in records)):
        raise EvidenceError("repair_evidence_unavailable")
    by_id = {str(event["id"]): event for event in study}
    for parent in records[:2]:
        if parent is None:
            raise EvidenceError("repair_evidence_unavailable")
        target = {"target_from": parent["from"], "artifact_id": parent["artifact_id"], "revision": parent["revision"]}
        found = False
        for event in objections:
            record = obj(obj(event["body"]).get("objection"))
            if record.get("from") != "agent:reader-guide" or any(record.get(key) != value for key, value in target.items()):
                continue
            citations = items(obj(record.get("body")).get("source_event_ids"))
            peers = [by_id.get(str(identifier)) for identifier in citations]
            if (any(peer is not None and oracle.artifact(peer) == parent for peer in peers)
                and any(peer is not None and (source := oracle.artifact(peer)) is not None
                        and source["from"] == "agent:reader-guide" for peer in peers)
                and all(peer is not None and int(str(peer["sequence"])) < int(str(event["sequence"])) for peer in peers)
                and obj(obj(obj(parent["body"])["reader_plan"])["reader"])["pagination"] == "first"):
                found = True
        if not found:
            raise EvidenceError("repair_evidence_unavailable")
    return identifiers


def inspect_package(directory: Path) -> JsonObject:
    result: JsonObject = {"format": FORMAT, "passed": False, "code": "unverified",
        "scope": "retained_synthetic_fixture_reproduction_only",
        "http_traversal": "reported_by_retained_native_reader_not_independently_observed_offline",
        "current_access_and_history_freshness": "not_established",
        "copying_authority": "operator_fixture_assertion_not_verified_or_granted",
        "source_identity": "reported_commit_and_hashes_not_independently_authenticated",
        "model_execution": "reported_provenance_consistency_not_independently_observed_or_authenticated",
        "arbitrary_prose_truth": "not_evaluated"}
    try:
        report = load(directory / "report.json")
        if report.get("format") != "agentciv-paged-reader-experiment/0.1" or report.get("outcome") != "completed":
            raise EvidenceError("experiment_incomplete")
        if report.get("source_changed_during_run") is not False:
            raise EvidenceError("source_identity_unestablished")
        identity = obj(report.get("source"))
        source_hashes = obj(identity.get("source_sha256"))
        if (re.fullmatch(r"[0-9a-f]{40}", str(identity.get("repository_commit"))) is None
            or type(identity.get("source_working_tree_dirty")) is not bool or not source_hashes
            or any(re.fullmatch(r"[0-9a-f]{64}", str(digest)) is None for digest in source_hashes.values())):
            raise EvidenceError("source_identity_unestablished")
        controls = obj(report.get("controls"))
        mode, condition = report.get("mode"), report.get("condition")
        if mode not in ("scripted", "ollama") or condition not in ("continuation", "repair"):
            raise EvidenceError("configuration_unestablished")
        if controls.get("choice_source") != mode or controls.get("condition") != condition:
            raise EvidenceError("provenance_mismatch")
        study, study_originals = snapshot(load(directory / "study-originals.json"), "civ:reader-collaboration")
        challenge, challenge_originals = snapshot(load(directory / "challenge-originals.json"), "civ:reader-challenge")
        check_transport(report.get("study_transport"), study_originals, complete=True)
        check_transport(report.get("study_revalidation_transport"), study_originals, complete=True)
        check_transport(report.get("challenge_transport"), challenge_originals, complete=True)
        if (len(challenge) != 106 or report.get("challenge_event_count") != 106 or report.get("challenge_pages") != 2
            or report.get("challenge_unchanged") is not True
            or report.get("challenge_original_representation_sha256") != representations_hash(challenge_originals)
            or report.get("challenge_sha256") != hashlib.sha256(json.dumps(challenge, sort_keys=True).encode("utf-8")).hexdigest()):
            raise EvidenceError("stale_report")
        observations, checks = items(report.get("observations")), items(report.get("checks"))
        if len(observations) != 3 or len(checks) != 3:
            raise EvidenceError("missing_evidence")
        records = [publication(obj(observation), study, principal, str(mode))
                   for observation, principal in zip(observations, AUTHORS, strict=True)]
        objection_ids = story(study, records, report)
        measured = [reproduce(study, challenge, challenge_originals, record, obj(check))
                    for record, check in zip(records, checks, strict=True)]
        successor = obj(observations[2])
        target = successor.get("target_event_id")
        peer = next((event for event in study if event["id"] == target and event.get("actor") in AUTHORS[:2]
                     and event["kind"] == "artifact.recorded"), None)
        peer_record = oracle.artifact(peer) if peer is not None else None
        exact = (records[2] is not None and peer_record is not None and successor.get("derived_from") == {
            key: peer_record[key] for key in ("from", "artifact_id", "revision")}
            and target in items(obj(successor["decision"]).get("source_event_ids"))
            and target in items(successor.get("peer_artifact_citations", [])))
        parent_index = next((index for index, observation in enumerate(observations[:2])
                             if obj(obj(observation).get("receipt", {})).get("event_id") == target), None)
        first, last = measured[0], measured[2]
        gain = first["outcome"] == "evaluated" and first["passed"] is False and last["passed"] is True
        parent_gain = exact and parent_index is not None and measured[parent_index]["passed"] is False and last["passed"] is True
        conclusions: JsonObject = {"useful_result": last["passed"] is True,
            "objective_improvement": gain, "first_to_successor_objective_gain": gain,
            "first_to_successor_reader_repair": first.get("reuse_passed") is False and last["passed"] is True,
            "exact_peer_artifact_derivation": exact, "qualified_improvement": parent_gain,
            "qualified_reader_repair": parent_gain and parent_index is not None and measured[parent_index].get("reuse_passed") is False,
            "independent_reconstruction_improvement": gain and not exact,
            "peer_target_event_id": target if exact else None}
        if any(not same(report.get(key), value) for key, value in conclusions.items()):
            raise EvidenceError("stale_report")
        aliases: JsonObject = {"peer_target_check": checks[parent_index] if exact and parent_index is not None else None,
            "successor_derived_from": successor.get("derived_from"),
            "successor_peer_artifact_citations": successor.get("peer_artifact_citations", []),
            "model_improvement_observed": report["mode"] == "ollama" and parent_gain,
            "study_revalidated": True}
        if any(not same(report.get(key), value) for key, value in aliases.items()):
            raise EvidenceError("stale_report")
        result.update(conclusions)
        result.update({"passed": True, "code": "verified", "checks": measured,
            "retained_objection_ids": objection_ids,
            "retained_decline_ids": [event["id"] for event in study if event["kind"] == "decline.recorded"]})
    except EvidenceError as error:
        result["code"] = str(error)
    except (oracle.OracleError, KeyError, TypeError, UnicodeError, RecursionError):
        result["code"] = "malformed_package"
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", type=Path, required=True)
    result = inspect_package(parser.parse_args().package)
    print(json.dumps(result, ensure_ascii=False, allow_nan=False))
    return 0 if result["passed"] is True else 1


if __name__ == "__main__":
    raise SystemExit(main())
