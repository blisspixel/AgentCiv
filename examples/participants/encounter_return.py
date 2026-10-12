"""Bounded offline comparison of two private caller observations.

Native validation checks original events. Saved metadata is not origin proof,
and differences never authorize publication or diagnose why visibility changed.
"""
from __future__ import annotations

import json
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path

import encounter_client as client
import encounter_private as private

JsonObject = dict[str, object]
FORMAT = "agentciv-private-return-view/0.1-example"
MAX_RESULT_BYTES = 512 * 1024


@dataclass
class Observation:
    metadata: JsonObject
    records: dict[str, tuple[str, JsonObject]]
    started: datetime
    finished: datetime


def _time(value: object) -> datetime:
    if not isinstance(value, str) or len(value) > 64:
        raise private.EncounterError("invalid_retrieval_interval")
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        raise private.EncounterError("invalid_retrieval_interval") from None
    if parsed.utcoffset() is None:
        raise private.EncounterError("invalid_retrieval_interval")
    return parsed


def _load(path: Path, settings: JsonObject, binary: Path) -> Observation:
    view = client._decode(private.read_bounded(path, client.MAX_OUTPUT))
    client._validate_view(view, settings)
    started, finished = _time(view.get("retrieval_started")), _time(view.get("retrieval_finished"))
    if finished < started:
        raise private.EncounterError("invalid_retrieval_interval")
    budgets = client._object(view.get("configured_budgets"))
    report = client._object(view["transport"])
    originals = client._originals(view)
    if (set(budgets) != set(client.BUDGETS)
        or any(type(budgets[key]) is not int or not 1 <= int(str(budgets[key])) <= int(str(maximum))
               for key, maximum in client.BUDGETS.items())
        or type(report.get("pages")) is not int or not 1 <= int(str(report["pages"])) <= int(str(budgets["max_pages"]))
        or len(originals) > int(str(budgets["max_events"]))
        or type(report.get("response_bytes")) is not int
        or not 0 <= int(str(report["response_bytes"])) <= int(str(budgets["max_total_bytes"]))
        or any(len(original.encode("utf-8")) > int(str(budgets["max_record_bytes"])) for original in originals)
        or sum(len(original.encode("utf-8")) for original in originals) > int(str(budgets["max_total_bytes"]))):
        raise private.EncounterError("private_view_budget_invalid")
    # Only the existing offline command receives originals, never credentials.
    # Its validator owns event syntax, identities, ordering and report consistency.
    native: JsonObject = {"snapshot": view["snapshot"], "report": report}
    if "resume" in view:
        native["resume"] = view["resume"]
    records: dict[str, tuple[str, JsonObject]] = {}
    for original in originals:
        event = client._decode(original.encode("utf-8"))
        if client._contains_secret(event, str(settings["token"])):
            raise private.EncounterError("credential_reflected")
        identifier = event.get("id")
        if not isinstance(identifier, str) or identifier in records:
            raise private.EncounterError("invalid_private_view")
        records[identifier] = original, event
    client._project(native, binary, Path(str(settings["cache_directory"])))
    metadata: JsonObject = {"view_path": str(path), "retrieval_started": view["retrieval_started"],
        "retrieval_finished": view["retrieval_finished"], "configured_budgets": budgets,
        "events": len(originals), "pages": report["pages"], "response_bytes": report["response_bytes"]}
    return Observation(metadata, records, started, finished)


def _reference(identifier: str, event: JsonObject) -> JsonObject:
    # Deliberately omit bodies and artifact selectors from this navigation view.
    return {"event_id": identifier, "sequence": client._integer(event["sequence"]), "kind": event["kind"]}


def changes(config: Path, current: Path, previous: Path, *, reader_binary: Path | None = None) -> JsonObject:
    """No HTTP, writes to originals, read marker, publication, or journal retry."""
    try:
        settings = client._configuration(config)
        with private.lock(Path(str(settings["cache_directory"]))):
            binary = client._binary(reader_binary)
            before, after = _load(previous, settings, binary), _load(current, settings, binary)
        if after.started < before.started or after.finished < before.finished:
            raise private.EncounterError("reversed_observations")
        sequences: dict[int, str] = {}
        for identifier, (_, event) in list(before.records.items()) + list(after.records.items()):
            sequence = client._integer(event.get("sequence"))
            if sequence is None or sequences.get(sequence, identifier) != identifier:
                raise private.EncounterError("conflicting_observations")
            sequences[sequence] = identifier
            if identifier in before.records and identifier in after.records:
                if before.records[identifier][1]["sequence"] != after.records[identifier][1]["sequence"]:
                    raise private.EncounterError("conflicting_observations")
        newly_visible: list[JsonObject] = []
        changed: list[JsonObject] = []
        absent: list[JsonObject] = []
        unchanged = 0
        for identifier, (original, event) in after.records.items():
            if identifier not in before.records:
                newly_visible.append(_reference(identifier, event))
            elif original != before.records[identifier][0]:
                changed.append({"event_id": identifier, "sequence": client._integer(event["sequence"]),
                    "previous_kind": before.records[identifier][1]["kind"], "current_kind": event["kind"]})
            else:
                unchanged += 1
        for identifier, (_, event) in before.records.items():
            if identifier not in after.records:
                absent.append(_reference(identifier, event))
        result: JsonObject = {"outcome": "compared", "format": FORMAT, "world": settings["world"],
            "principal": settings["principal"], "previous": before.metadata, "current": after.metadata,
            "retrieval_intervals_overlap": after.started < before.finished,
            "scope": "two_saved_caller_observations_not_live_history", "origin_authenticated": False,
            "copying_permission": "not_granted", "read_boundary_advanced": False,
            "interpretation": "Newly visible is not newly created. Absence has no diagnosed cause. Changed means different original bytes.",
            "unchanged_events": unchanged, "newly_visible": newly_visible,
            "representation_changed": changed, "not_in_current_observation": absent}
        if client._contains_secret(result, str(settings["token"])):
            raise private.EncounterError("credential_reflected")
        if len(json.dumps(result, ensure_ascii=True).encode("utf-8")) > MAX_RESULT_BYTES:
            raise private.EncounterError("return_view_too_large")
        return result
    except private.EncounterError:
        raise
    except (ValueError, OSError, KeyError, TypeError, RuntimeError):
        raise private.EncounterError("invalid_private_view") from None
