"""Deterministic acceptance exercise for data-only inherited history readers.

This is an example oracle, not an archive format or protocol requirement.
No submitted artifact is executed and no network request is made.
"""

from __future__ import annotations

import argparse
import copy
import json
from pathlib import Path

JsonObject = dict[str, object]
FORMAT = "agentciv-reader-plan/0.1-fixture"
MAX_BYTES = 512_000
MAX_EVENTS = 128
MAX_CLAIMS = 12
FACTS = {"pagination": "all", "chain_identity": "author_and_artifact", "citation_authority": "none"}
IDENTIFIER_SCHEMA: JsonObject = {"type": "string", "minLength": 1, "maxLength": 256}
PLAN_SCHEMA: JsonObject = {
    "type": "object", "additionalProperties": False,
    "required": ["format", "reader", "runbook", "claims"],
    "properties": {
        "format": {"const": FORMAT},
        "reader": {
            "type": "object", "additionalProperties": False,
            "required": ["pagination", "chain_identity", "revisions", "retain_objections", "retain_declines"],
            "properties": {
                "pagination": {"type": "string", "enum": ["all", "first"]},
                "chain_identity": {"type": "string", "enum": ["author_and_artifact", "artifact_only"]},
                "revisions": {"type": "string", "enum": ["all", "latest"]},
                "retain_objections": {"type": "boolean"}, "retain_declines": {"type": "boolean"},
            },
        },
        "runbook": {
            "type": "object", "additionalProperties": False,
            "required": ["credential_source", "scope", "missing_source", "execute_artifacts"],
            "properties": {
                "credential_source": {"type": "string", "enum": ["operator", "cited_author"]},
                "scope": {"type": "string", "enum": ["permitted_read_only", "citation_grants_access"]},
                "missing_source": {"type": "string", "enum": ["report_unverifiable", "trust_summary"]},
                "execute_artifacts": {"type": "boolean"},
            },
        },
        "claims": {
            "type": "array", "maxItems": MAX_CLAIMS,
            "items": {
                "type": "object", "additionalProperties": False, "required": ["topic", "value", "source"],
                "properties": {
                    "topic": {"type": "string", "enum": list(FACTS)}, "value": IDENTIFIER_SCHEMA,
                    "source": {
                        "type": "object", "additionalProperties": False,
                        "required": ["event_id", "from", "artifact_id", "revision"],
                        "properties": {"event_id": IDENTIFIER_SCHEMA, "from": IDENTIFIER_SCHEMA,
                                       "artifact_id": IDENTIFIER_SCHEMA,
                                       "revision": {"type": "integer", "minimum": 1,
                                                    "maximum": 9_007_199_254_740_991}},
                    },
                },
            },
        },
    },
}


class OracleError(ValueError):
    """Malformed exercise input; the message is a fixed, credential-free code."""


def object_value(value: object) -> JsonObject:
    if not isinstance(value, dict) or not all(isinstance(key, str) for key in value):
        raise OracleError("invalid_object")
    return dict(value)


def string(value: object) -> str:
    if not isinstance(value, str) or not value or len(value) > 256:
        raise OracleError("invalid_identifier")
    try:
        value.encode("utf-8")
    except UnicodeEncodeError as error:
        raise OracleError("invalid_identifier") from error
    return value


def bounded(value: object) -> None:
    try:
        payload = json.dumps(value, ensure_ascii=False, allow_nan=False).encode("utf-8")
    except (TypeError, ValueError, UnicodeEncodeError, RecursionError) as error:
        raise OracleError("invalid_json_data") from error
    if len(payload) > MAX_BYTES:
        raise OracleError("input_too_large")


def artifact(event: JsonObject) -> JsonObject | None:
    if event["kind"] != "artifact.recorded":
        return None
    record = object_value(object_value(event["body"]).get("artifact_revision"))
    string(record.get("from"))
    string(record.get("artifact_id"))
    revision = record.get("revision")
    if type(revision) is not int or not 1 <= revision <= 9_007_199_254_740_991:
        raise OracleError("invalid_revision")
    object_value(record.get("body"))
    return record


def validate_events(events: object) -> list[JsonObject]:
    bounded(events)
    if not isinstance(events, list) or len(events) > MAX_EVENTS:
        raise OracleError("invalid_history")
    found: list[JsonObject] = []
    identifiers: set[str] = set()
    revisions: set[tuple[str, str, int]] = set()
    world: str | None = None
    previous_sequence = -1
    for value in events:
        event = object_value(value)
        event_id = string(event.get("id"))
        if event_id in identifiers:
            raise OracleError("duplicate_event_id")
        identifiers.add(event_id)
        current_world = string(event.get("world"))
        if world is not None and world != current_world:
            raise OracleError("mixed_worlds")
        world = current_world
        sequence = event.get("sequence")
        if type(sequence) is not int or not previous_sequence < sequence <= 9_007_199_254_740_991:
            raise OracleError("invalid_sequence")
        previous_sequence = sequence
        string(event.get("kind"))
        object_value(event.get("body"))
        if event["kind"] == "artifact.withdrawn" and event["body"] != {}:
            raise OracleError("invalid_tombstone")
        record = artifact(event)
        if record is not None:
            key = (string(record["from"]), string(record["artifact_id"]), int(str(record["revision"])))
            if key in revisions:
                raise OracleError("duplicate_revision")
            revisions.add(key)
        found.append(copy.deepcopy(event))
    return found


def validate_candidate(value: object) -> JsonObject:
    bounded(value)
    candidate = object_value(value)
    if set(candidate) != {"format", "reader", "runbook", "claims"} or candidate["format"] != FORMAT:
        raise OracleError("invalid_candidate")
    reader = object_value(candidate["reader"])
    if set(reader) != {"pagination", "chain_identity", "revisions", "retain_objections", "retain_declines"}:
        raise OracleError("invalid_reader")
    for key, options in (("pagination", ("all", "first")),
                         ("chain_identity", ("author_and_artifact", "artifact_only")),
                         ("revisions", ("all", "latest"))):
        if not isinstance(reader[key], str) or reader[key] not in options:
            raise OracleError("invalid_reader")
    if any(type(reader[key]) is not bool for key in ("retain_objections", "retain_declines")):
        raise OracleError("invalid_reader")
    runbook = object_value(candidate["runbook"])
    options_by_key = {"credential_source": ("operator", "cited_author"),
                      "scope": ("permitted_read_only", "citation_grants_access"),
                      "missing_source": ("report_unverifiable", "trust_summary")}
    if set(runbook) != {*options_by_key, "execute_artifacts"}:
        raise OracleError("invalid_runbook")
    for key, options in options_by_key.items():
        if not isinstance(runbook[key], str) or runbook[key] not in options:
            raise OracleError("invalid_runbook")
    if type(runbook["execute_artifacts"]) is not bool:
        raise OracleError("invalid_runbook")
    claims = candidate["claims"]
    if not isinstance(claims, list) or len(claims) > MAX_CLAIMS:
        raise OracleError("invalid_claims")
    topics: set[str] = set()
    for value in claims:
        claim = object_value(value)
        if set(claim) != {"topic", "value", "source"}:
            raise OracleError("invalid_claim")
        topic = string(claim["topic"])
        if topic not in FACTS or topic in topics:
            raise OracleError("invalid_claim_topic")
        topics.add(topic)
        string(claim["value"])
        source = object_value(claim["source"])
        if set(source) != {"event_id", "from", "artifact_id", "revision"}:
            raise OracleError("invalid_citation")
        for key in ("event_id", "from", "artifact_id"):
            string(source[key])
        if type(source["revision"]) is not int or not 1 <= source["revision"] <= 9_007_199_254_740_991:
            raise OracleError("invalid_citation")
    return copy.deepcopy(candidate)


def validate_plan(value: object) -> JsonObject:
    """Validate structure only; incorrect choices and unsupported citations remain valid."""
    return validate_candidate(value)


def exercise_reader(events: list[JsonObject], reader: JsonObject) -> list[JsonObject]:
    """A separate fresh paged caller interprets fixed choices, never submitted code."""
    pages = [copy.deepcopy(events[offset:offset + 2]) for offset in range(0, len(events), 2)]
    if reader["pagination"] == "first":
        pages = pages[:1]
    selected: dict[tuple[str, ...], JsonObject] = {}
    key: tuple[str, ...]
    for page in pages:
        for event in page:
            if event["kind"] == "objection.recorded" and not reader["retain_objections"]:
                continue
            if event["kind"] == "decline.recorded" and not reader["retain_declines"]:
                continue
            record = artifact(event)
            if record is None:
                key = (string(event["id"]),)
            else:
                key = (string(record["artifact_id"]),)
                if reader["chain_identity"] == "author_and_artifact":
                    key = (string(record["from"]), *key)
                if reader["revisions"] == "all":
                    key = (*key, str(record["revision"]))
            selected[key] = event
    return sorted(selected.values(), key=lambda event: int(str(event["sequence"])))


def source_check(claim: JsonObject, by_id: dict[str, JsonObject]) -> str:
    source = object_value(claim["source"])
    event = by_id.get(string(source["event_id"]))
    if event is None:
        return "source_unavailable"
    if event["kind"] == "artifact.withdrawn":
        return "source_withdrawn"
    record = artifact(event)
    if record is None:
        return "source_not_artifact"
    if any(record[key] != source[key] for key in ("from", "artifact_id", "revision")):
        return "source_revision_mismatch"
    assertions = object_value(record["body"]).get("assertions")
    expected_assertion = {"topic": claim["topic"], "value": claim["value"]}
    if not isinstance(assertions, list) or expected_assertion not in assertions:
        return "source_does_not_support_claim"
    if claim["value"] != FACTS[string(claim["topic"])]:
        return "contradicts_interface_reference"
    return "supported"


def evaluate(events: list[JsonObject], candidate: JsonObject) -> JsonObject:
    """Report fixture reuse and exact structured-source support separately.

    Inputs are already permitted records. Authorization and export integrity are
    the caller's duties. Source support covers three declared exercise topics,
    not the truth of arbitrary natural-language explanations.
    """
    records = validate_events(events)
    plan = validate_candidate(candidate)
    selected = exercise_reader(records, object_value(plan["reader"]))
    expected_ids = [event["id"] for event in records]
    selected_ids = [event["id"] for event in selected]
    objections = any(event["kind"] == "objection.recorded" for event in records)
    declines = any(event["kind"] == "decline.recorded" for event in records)
    retained_objections = [event["id"] for event in selected if event["kind"] == "objection.recorded"]
    retained_declines = [event["id"] for event in selected if event["kind"] == "decline.recorded"]
    by_id = {string(event["id"]): event for event in records}
    claims = plan["claims"]
    if not isinstance(claims, list):
        raise OracleError("invalid_claims")
    checks = [{"topic": object_value(claim)["topic"],
               "code": source_check(object_value(claim), by_id)} for claim in claims]
    present_topics = {string(check["topic"]) for check in checks}
    missing_topics = sorted(set(FACTS) - present_topics)
    sources_pass = not missing_topics and all(check["code"] == "supported" for check in checks)
    runbook = object_value(plan["runbook"])
    runbook_checks = {
        "operator_credential_required": runbook["credential_source"] == "operator",
        "read_scope_explicit": runbook["scope"] == "permitted_read_only",
        "missing_source_reported": runbook["missing_source"] == "report_unverifiable",
        "artifact_execution_disabled": runbook["execute_artifacts"] is False,
    }
    reuse_pass = bool(records) and expected_ids == selected_ids and objections and declines
    return {
        "format": "agentciv-inheritance-result/0.1-fixture",
        "evaluation": "deterministic_data_plan_not_model_or_live_host",
        "useful_continuation": reuse_pass and sources_pass and all(runbook_checks.values()),
        "reuse": {"passed": reuse_pass, "expected_event_ids": expected_ids,
                  "retrieved_event_ids": selected_ids, "objection_available": objections,
                  "decline_available": declines, "retained_objection_event_ids": retained_objections,
                  "retained_decline_event_ids": retained_declines},
        "source_correctness": {"passed": sources_pass, "checks": checks, "missing_topics": missing_topics,
                               "scope": "exact_revision_structured_assertions_and_declared_interface_facts"},
        "runbook": {"passed": all(runbook_checks.values()), "checks": runbook_checks},
    }


def load_json(path: Path) -> object:
    def unique(pairs: list[tuple[str, object]]) -> JsonObject:
        found: JsonObject = {}
        for key, value in pairs:
            if key in found:
                raise ValueError("duplicate_member")
            found[key] = value
        return found

    def invalid_constant(value: str) -> object:
        raise ValueError("nonstandard_constant")

    with path.open("rb") as stream:
        payload = stream.read(MAX_BYTES + 1)
    if len(payload) > MAX_BYTES:
        raise OracleError("input_too_large")
    try:
        value: object = json.loads(payload, object_pairs_hook=unique, parse_constant=invalid_constant)
    except (UnicodeDecodeError, ValueError, RecursionError) as error:
        raise OracleError("invalid_json_data") from error
    return value


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--history", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    args = parser.parse_args()
    try:
        events = validate_events(load_json(args.history))
        candidate = validate_candidate(load_json(args.candidate))
        report = evaluate(events, candidate)
    except OracleError as error:
        print(json.dumps({"outcome": "invalid_input", "code": str(error)}))
        return 2
    except OSError:
        print(json.dumps({"outcome": "input_unavailable"}))
        return 2
    print(json.dumps(report, indent=2))
    return 0 if report["useful_continuation"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
