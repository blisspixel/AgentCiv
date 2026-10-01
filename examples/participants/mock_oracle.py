"""Acceptance for a fictional stock checklist, separate from wire validation.

Only the exercise operator's structured stock messages define this task's facts.
This module does not authenticate a host, grant permission, execute artifacts,
or decide whether arbitrary participant prose is true.
"""

from __future__ import annotations

import json

JsonObject = dict[str, object]
OPERATOR = "agent:operator"
ITEMS = frozenset({"nuts", "bolts", "washers"})
MAX_QUANTITY = 1_000_000
MAX_EVENTS = 256
MAX_BYTES = 512_000


class OracleError(ValueError):
    """Malformed exercise input, with a fixed public code."""


def object_value(value: object) -> JsonObject:
    if not isinstance(value, dict) or not all(isinstance(key, str) for key in value):
        raise OracleError("invalid_object")
    return dict(value)


def quantity(value: object) -> bool:
    return type(value) is int and 0 <= value <= MAX_QUANTITY


def identifier(value: object) -> bool:
    return isinstance(value, str) and bool(value) and len(value) <= 1024


def bounded(value: object) -> None:
    try:
        payload = json.dumps(value, ensure_ascii=False, allow_nan=False).encode("utf-8")
    except (TypeError, ValueError, UnicodeEncodeError, RecursionError) as error:
        raise OracleError("invalid_json_data") from error
    if len(payload) > MAX_BYTES:
        raise OracleError("input_too_large")


def source_sequence(value: object) -> int:
    """Interpret schema integers without changing the original source fields."""
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise OracleError("invalid_history")
    if not 0 <= value <= 9_007_199_254_740_991:
        raise OracleError("invalid_history")
    if isinstance(value, float) and not value.is_integer():
        raise OracleError("invalid_history")
    return int(value)


def latest_facts(events: object) -> dict[str, JsonObject]:
    """Read current operator assertions from an already permitted host history.

    Calling code must verify the authenticated HTTP source. Actor names alone
    in an arbitrary imported file do not prove who supplied these assertions.
    Peer messages, artifact revisions, and summaries never become stock facts.
    """
    bounded(events)
    if not isinstance(events, list) or len(events) > MAX_EVENTS:
        raise OracleError("invalid_history")
    facts: dict[str, JsonObject] = {}
    seen: set[str] = set()
    previous = -1
    world: object = None
    for value in events:
        event = object_value(value)
        event_id = event.get("id")
        sequence = source_sequence(event.get("sequence"))
        current_world = event.get("world")
        if (not identifier(event_id) or str(event_id) in seen
            or sequence <= previous
            or not identifier(current_world) or (world is not None and world != current_world)):
            raise OracleError("invalid_history")
        seen.add(str(event_id))
        previous = sequence
        world = current_world
        if event.get("kind") != "message.recorded" or event.get("actor") != OPERATOR:
            continue
        message = object_value(object_value(event.get("body")).get("message"))
        if (message.get("type") != "message" or message.get("from") != OPERATOR
            or message.get("world") != current_world):
            raise OracleError("invalid_operator_record")
        body = object_value(message.get("body"))
        if "stock" not in body:
            continue
        stock = object_value(body["stock"])
        item = stock.get("item")
        count = stock.get("quantity")
        if not isinstance(item, str) or item not in ITEMS or not quantity(count):
            raise OracleError("invalid_stock_fact")
        facts[item] = {"quantity": count, "event_id": event_id}
    return facts


def row_sources_covered(row: JsonObject, sources: list[object]) -> bool:
    row_sources = row.get("source_event_ids")
    return isinstance(row_sources, list) and all(
        isinstance(source, str) and source in sources for source in row_sources)


def evaluate(events: object, decision: object) -> JsonObject:
    """Check exact quantities and source support without repairing a decision.

    A stop, decline, or objection is a possible participant act, but does not
    satisfy this particular checklist acceptance exercise. A valid structure
    can still fail every semantic check below.
    """
    report: JsonObject = {
        "format": "agentciv-stock-checklist-result/0.1-fixture",
        "accepted": False,
        "scope": "fictional_operator_stock_facts_and_exact_event_provenance",
    }
    try:
        facts = latest_facts(events)
        bounded(decision)
        candidate = object_value(decision)
        if candidate.get("action") != "revise":
            report.update({"outcome": "not_authored", "checks": {"artifact_authored": False}})
            return report
        rows_value = candidate.get("rows")
        if not isinstance(rows_value, list) or len(rows_value) > 8:
            raise OracleError("invalid_checklist")
        rows = [object_value(row) for row in rows_value]
        visible_ids = {str(object_value(event)["id"]) for event in events} if isinstance(events, list) else set()
        sources = candidate.get("source_event_ids")
        source_list_valid = (isinstance(sources, list)
                             and all(isinstance(source, str) for source in sources)
                             and len(sources) == len(set(sources))
                             and all(source in visible_ids for source in sources))
        row_items = [row.get("item") for row in rows]
        items_complete = (len(rows) == len(ITEMS)
                          and all(isinstance(item, str) for item in row_items)
                          and set(str(item) for item in row_items) == ITEMS)
        facts_complete = set(facts) == ITEMS
        quantities_match = bool(rows) and all(
            isinstance(row.get("item"), str) and str(row["item"]) in facts
            and quantity(row.get("quantity"))
            and row["quantity"] == facts[str(row["item"])]["quantity"] for row in rows)
        provenance_matches = bool(rows) and all(
            isinstance(row.get("item"), str) and str(row["item"]) in facts
            and row.get("source_event_ids") == [facts[str(row["item"])]["event_id"]]
            for row in rows)
        sources_cover_rows = source_list_valid and all(
            row_sources_covered(row, sources) for row in rows) if isinstance(sources, list) else False
        total = candidate.get("total")
        expected_total = sum(int(str(fact["quantity"])) for fact in facts.values())
        row_quantities_valid = all(quantity(row.get("quantity")) for row in rows)
        checks = {
            "artifact_authored": True,
            "operator_facts_complete": facts_complete,
            "items_unique_and_complete": items_complete,
            "latest_quantities_match": quantities_match,
            "latest_row_sources_match": provenance_matches,
            "visible_sources_cover_rows": sources_cover_rows,
            "total_matches_rows": quantity(total) and row_quantities_valid
            and total == sum(int(str(row["quantity"])) for row in rows),
            "total_matches_latest_facts": quantity(total) and facts_complete and total == expected_total,
        }
        accepted = all(checks.values())
        report.update({"accepted": accepted, "outcome": "accepted" if accepted else "not_accepted",
                       "checks": checks, "expected_latest_facts": facts,
                       "expected_total": expected_total})
    except OracleError as error:
        report.update({"outcome": "invalid_input", "failure_code": str(error), "checks": {}})
    return report
