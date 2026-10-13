"""Inspect one selected synthetic reading table offline, without selecting a winner.

Exact original strings remain the evidence. This optional example convention is
not a protocol, copying grant, authenticated package or participant scheduler.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import stat
import subprocess
import tempfile
from pathlib import Path

import encounter_client as client
import collaboration as civic

JsonObject = dict[str, object]
FORMAT = "agentciv-reading-table/0.1-example"
COPYING = "operator_authorized_synthetic_fixture_export_only"
MAX_INPUT = 1_048_576
MAX_RECORDS = 32
MAX_OUTPUT = 65_536
TEMP_PREFIX = "agentciv-reading-table-"
SOURCE = "agentciv-reading-table-source/0.1-example"
PRACTICE = "agentciv-reading-table-practice/0.1-example"
ADOPTION = "agentciv-reading-table-adoption/0.1-example"


class TableError(ValueError):
    """Only fixed codes, never untrusted input, are printable."""


def obj(value: object) -> JsonObject:
    if not isinstance(value, dict):
        raise TableError("invalid_fixture")
    return value


def rows(value: object) -> list[object]:
    if not isinstance(value, list):
        raise TableError("invalid_fixture")
    return value


def text(value: object, maximum: int = 1024) -> str:
    if not isinstance(value, str) or not 1 <= len(value.encode("utf-8")) <= maximum:
        raise TableError("invalid_fixture")
    return value


def digest(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def encoded(value: object) -> bytes:
    # Escaping also keeps exact originals round-trippable on a terminal.
    return json.dumps(value, ensure_ascii=True, allow_nan=False).encode("ascii")


def load(path: Path) -> tuple[bytes, JsonObject, list[str]]:
    try:
        if path.is_symlink() or not path.is_file():
            raise TableError("input_unavailable")
        with path.open("rb") as stream:
            raw = stream.read(MAX_INPUT + 1)
        if len(raw) > MAX_INPUT:
            raise TableError("input_too_large")
        value = client._decode(raw)
        if (set(value) != {"format", "world", "copying_condition", "records", "producer_report"}
            or value.get("format") != FORMAT or value.get("copying_condition") != COPYING):
            raise TableError("invalid_fixture")
        text(value.get("world"), 1024)
        originals: list[str] = []
        entries = rows(value.get("records"))
        if len(entries) > MAX_RECORDS:
            raise TableError("input_too_large")
        for entry in entries:
            item = obj(entry)
            if set(item) != {"original_utf8", "sha256"}:
                raise TableError("invalid_fixture")
            original = text(item.get("original_utf8"), 16000)
            if digest(original.encode("utf-8")) != item.get("sha256"):
                raise TableError("original_hash_mismatch")
            originals.append(original)
        # producer_report is deliberately not consulted, even if it claims a winner.
        return raw, value, originals
    except OSError:
        raise TableError("input_unavailable") from None


def cleanup(directory: Path, parent: Path) -> None:
    """Remove only this newly allocated directory and its one known input."""
    try:
        info = directory.lstat()
        if (not stat.S_ISDIR(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400
            or directory.resolve().parent != parent or not directory.name.startswith(TEMP_PREFIX)
            or set(path.name for path in directory.iterdir()) - {"originals.json"}):
            raise TableError("validation_cleanup_failed")
        path = directory / "originals.json"
        if path.exists() or path.is_symlink():
            info = path.lstat()
            if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or getattr(info, "st_file_attributes", 0) & 0x400:
                raise TableError("validation_cleanup_failed")
            path.unlink()
        directory.rmdir()
    except OSError:
        raise TableError("validation_cleanup_failed") from None


def validate(originals: list[str], world: object, binary: Path) -> None:
    """Reuse existing strict native event validation; the report is constructed data."""
    native = {"snapshot": {"world": world, "records": originals}, "report": {
        "pages": 1, "events": len(originals), "response_bytes": sum(len(row.encode("utf-8")) for row in originals),
        "reached_end": True, "scope": "current_caller_view", "copying_permission": "not_granted"}}
    try:
        directory = Path(tempfile.mkdtemp(prefix=TEMP_PREFIX)).absolute()
        parent = directory.parent.resolve()
        try:
            path = directory / "originals.json"
            path.write_bytes(encoded(native))
            process = subprocess.run([str(binary), "offers", str(path), ""], capture_output=True, timeout=40, check=False)
        finally:
            cleanup(directory, parent)
        if process.returncode != 0 or process.stderr or len(process.stdout) > MAX_INPUT:
            raise TableError("native_validation_failed")
        result = civic.decode(process.stdout)
        report = obj(result.get("report"))
        if (result.get("format") != "agentciv-offer-view/0.1-example" or result.get("world") != world
            or report.get("truncated") is not False or report.get("events") != len(originals)):
            raise TableError("native_validation_failed")
    except (OSError, subprocess.SubprocessError, civic.DecisionError):
        raise TableError("native_validation_failed") from None


def artifact(event: JsonObject) -> JsonObject | None:
    if event.get("kind") != "artifact.recorded":
        return None
    return obj(obj(event.get("body")).get("artifact_revision"))


def selector(event: JsonObject, original: str) -> JsonObject:
    result: JsonObject = {"event_id": event["id"], "sequence": client._integer(event["sequence"]),
        "kind": event["kind"], "original_sha256": digest(original.encode("utf-8"))}
    record = artifact(event)
    if record is not None:
        result.update({"from": record["from"], "artifact_id": record["artifact_id"],
            "revision": client._integer(record["revision"], 1)})
    return result


def tuple_of(record: JsonObject) -> tuple[object, object, int | None]:
    return record.get("from"), record.get("artifact_id"), client._integer(record.get("revision"), 1)


def control_selector(value: object) -> JsonObject:
    ref = obj(value)
    return {**ref, "revision": client._integer(ref.get("revision"), 1)}


def reference(value: object, by_id: dict[str, tuple[JsonObject, str]], before: JsonObject) -> tuple[str, JsonObject | None]:
    ref = obj(value)
    if (set(ref) != {"event_id", "from", "artifact_id", "revision", "original_sha256"}
        or client._integer(ref.get("revision"), 1) is None):
        raise TableError("invalid_source_selector")
    for key in ("event_id", "from", "artifact_id"):
        text(ref.get(key), 1024)
    hashed = ref.get("original_sha256")
    if not isinstance(hashed, str) or len(hashed) != 64 or any(char not in "0123456789abcdef" for char in hashed):
        raise TableError("invalid_source_selector")
    found = by_id.get(str(ref["event_id"]))
    if found is None:
        return "unavailable", None
    event, original = found
    sequence, boundary = client._integer(event["sequence"]), client._integer(before["sequence"])
    if sequence is None or boundary is None or sequence >= boundary:
        raise TableError("invalid_source_selector")
    if event["kind"] in ("artifact.withdrawn", "event.redacted"):
        # The current tombstone no longer carries a checkable artifact tuple/hash.
        return "withdrawn_or_redacted", None
    record = artifact(event)
    if record is None or tuple_of(record) != tuple_of(ref) or digest(original.encode("utf-8")) != hashed:
        raise TableError("invalid_source_selector")
    return "available", record


def contribution(value: JsonObject, source: JsonObject | None) -> None:
    entries = rows(value.get("contribution"))
    if not 1 <= len(entries) <= 16:
        raise TableError("invalid_practice")
    known: set[str] | None = None
    if source is not None:
        material = obj(obj(source.get("body")).get("reading_table_source"))
        known = {text(obj(row).get("id")) for row in rows(material.get("passages"))}
    seen: set[str] = set()
    for entry in entries:
        item = obj(entry)
        if value["organizes_by"] == "passages":
            if set(item) != {"passage_id", "note"}:
                raise TableError("invalid_practice")
            identifier = text(item["passage_id"])
            text(item["note"])
            targets = [identifier]
        else:
            if set(item) != {"question", "passage_ids"}:
                raise TableError("invalid_practice")
            identifier = text(item["question"])
            targets = [text(row) for row in rows(item["passage_ids"])]
            if not targets or len(targets) != len(set(targets)) or len(targets) > 16:
                raise TableError("invalid_practice")
        if identifier in seen or known is not None and any(target not in known for target in targets):
            raise TableError("invalid_practice")
        seen.add(identifier)


def reconstruct(originals: list[str]) -> JsonObject:
    events = [client._decode(original.encode("utf-8")) for original in originals]
    by_id = {str(event["id"]): (event, original) for event, original in zip(events, originals, strict=True)}
    sources: list[JsonObject] = []
    practices: list[JsonObject] = []
    responses: list[JsonObject] = []
    adoptions: list[JsonObject] = []
    all_selectors = [selector(event, original) for event, original in zip(events, originals, strict=True)]
    for event, original in zip(events, originals, strict=True):
        record = artifact(event)
        if record is not None:
            body = obj(record["body"])
            if "reading_table_source" in body:
                source = obj(body["reading_table_source"])
                if set(source) != {"format", "copying_condition", "passages"} or source.get("format") != SOURCE or source.get("copying_condition") != COPYING:
                    raise TableError("invalid_source")
                passages = rows(source.get("passages"))
                if not 1 <= len(passages) <= 16:
                    raise TableError("invalid_source")
                identifiers = []
                for passage in passages:
                    item = obj(passage)
                    if set(item) != {"id", "text"}:
                        raise TableError("invalid_source")
                    identifiers.append(text(item.get("id")))
                    text(item.get("text"))
                if len(set(identifiers)) != len(identifiers):
                    raise TableError("invalid_source")
                sources.append(selector(event, original))
            if "reading_table_practice" in body:
                practice = obj(body["reading_table_practice"])
                if (set(practice) != {"format", "organizes_by", "source", "contribution", "state", "basis"}
                    or practice.get("format") != PRACTICE or practice.get("organizes_by") not in ("passages", "open_questions")
                    or practice.get("state") not in ("unfinished", "offered")):
                    raise TableError("invalid_practice")
                status, source_record = reference(practice["source"], by_id, event)
                contribution(practice, source_record)
                basis_status, basis_record = reference(practice["basis"], by_id, event)
                basis = control_selector(practice["basis"])
                expected = {key: basis[key] for key in ("from", "artifact_id", "revision")}
                if not client._json_equal(record.get("derived_from"), expected):
                    raise TableError("invalid_derivation")
                if basis_record is not None and tuple_of(basis_record) == tuple_of(record):
                    raise TableError("invalid_derivation")
                practices.append({"selector": selector(event, original), "organizes_by": practice["organizes_by"],
                    "state": practice["state"], "contribution": practice["contribution"], "source": control_selector(practice["source"]),
                    "source_status": status, "basis": basis, "basis_status": basis_status})
        elif event["kind"] in ("objection.recorded", "decline.recorded"):
            key = "objection" if event["kind"] == "objection.recorded" else "decline"
            response = obj(obj(event["body"])[key])
            target = (response["target_from"], response["artifact_id"], client._integer(response["revision"], 1))
            match = next((item for item in all_selectors if (item.get("from"), item.get("artifact_id"), item.get("revision")) == target), None)
            if match is not None:
                target_sequence, response_sequence = client._integer(match["sequence"]), client._integer(event["sequence"])
                if target_sequence is None or response_sequence is None or target_sequence >= response_sequence:
                    raise TableError("invalid_response")
            responses.append({"selector": selector(event, original), "from": response["from"], "type": key,
                "target": {"from": target[0], "artifact_id": target[1], "revision": target[2]},
                "target_status": "available" if match is not None else "unavailable", "target_selector": match})
        elif event["kind"] == "message.recorded":
            message = obj(obj(event["body"])["message"])
            body = obj(message["body"])
            if "practice_adoption" not in body:
                continue
            adoption = obj(body["practice_adoption"])
            if set(adoption) != {"format", "practice", "scope", "statement"} or adoption.get("format") != ADOPTION:
                raise TableError("invalid_adoption")
            text(adoption.get("scope"))
            text(adoption.get("statement"))
            status, adopted = reference(adoption["practice"], by_id, event)
            if adopted is not None and "reading_table_practice" not in obj(adopted["body"]):
                raise TableError("invalid_adoption")
            adoptions.append({"selector": selector(event, original), "from": message["from"],
                "practice": control_selector(adoption["practice"]), "practice_status": status,
                "scope": adoption["scope"], "statement": adoption["statement"]})
    return {"original_selectors": all_selectors, "sources": sources, "practices": practices,
        "responses": responses, "attributed_adoption_statements": adoptions,
        "group_state": "not_inferred", "quiet_or_no_activity": "not_inferred_from_absence",
        "interpretation_quality": "not_evaluated"}


def inspect(path: Path, binary: Path, event_id: str | None = None) -> JsonObject:
    try:
        raw, package, originals = load(path)
        validate(originals, package["world"], binary)
        findings = reconstruct(originals)
        result: JsonObject = {"outcome": "inspected", "format": FORMAT, "fixture_sha256": digest(raw),
            "world": package["world"], "scope": "frozen_synthetic_records_not_an_observed_http_traversal",
            "copying_authority": "operator_fixture_assertion_not_authenticated_or_granted",
            "origin_authenticated": False, "producer_report_used": False, **findings}
        if event_id is not None:
            selected = next((row for row in originals if client._decode(row.encode("utf-8"))["id"] == event_id), None)
            if selected is None:
                raise TableError("event_unavailable")
            result = {"outcome": "opened", "format": FORMAT, "fixture_sha256": digest(raw), "original_utf8": selected,
                "original_sha256": digest(selected.encode("utf-8")), "origin_authenticated": False}
        if len(encoded(result)) > MAX_OUTPUT:
            raise TableError("result_too_large")
        return result
    except TableError as error:
        return {"outcome": "failed", "code": str(error)}
    except (ValueError, OSError, KeyError, TypeError, UnicodeError, RecursionError, civic.DecisionError):
        return {"outcome": "failed", "code": "invalid_fixture"}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--reader-binary", type=Path, required=True)
    parser.add_argument("--event-id")
    args = parser.parse_args()
    result = inspect(args.fixture, args.reader_binary, args.event_id)
    print(encoded(result).decode("ascii"))
    return 1 if result["outcome"] == "failed" else 0


if __name__ == "__main__":
    raise SystemExit(main())
