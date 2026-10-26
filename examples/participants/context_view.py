"""Inspect optional context views across unequal reader budgets.

Lenses present different selections of the same permitted observation without
appointing a canonical summary or inferring group consensus.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

import encounter_client as client
import reading_table as table

JsonObject = dict[str, object]
FORMAT = "agentciv-context-view/0.1-example"
MAX_INPUT = 1_048_576
MAX_OUTPUT = 65_536
ALLOWED_LENSES = ("chronological", "passages", "open_questions")


class ViewError(ValueError):
    """Only fixed codes, never untrusted input, are printable."""


def obj(value: object) -> JsonObject:
    if not isinstance(value, dict):
        raise ViewError("invalid_fixture")
    return value


def rows(value: object) -> list[object]:
    if not isinstance(value, list):
        raise ViewError("invalid_fixture")
    return value


def text(value: object, maximum: int = 1024) -> str:
    if not isinstance(value, str) or not 1 <= len(value.encode("utf-8")) <= maximum:
        raise ViewError("invalid_fixture")
    return value


def digest(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def encoded(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=True, allow_nan=False).encode("ascii")


def lens_config(value: object) -> JsonObject:
    item = obj(value)
    if set(item) != {"name", "author", "purpose", "version", "byte_budget"}:
        raise ViewError("invalid_lens")
    name = text(item.get("name"), 64)
    if name not in ALLOWED_LENSES:
        raise ViewError("invalid_lens")
    text(item.get("author"), 256)
    text(item.get("purpose"), 256)
    text(item.get("version"), 64)
    budget = item.get("byte_budget")
    if budget is not None:
        budget_int = client._integer(budget, 1)
        if budget_int is None or budget_int < 64 or budget_int > MAX_INPUT:
            raise ViewError("invalid_lens")
    return item


def record_summary(event: JsonObject) -> str:
    kind = str(event.get("kind", ""))
    ident = str(event.get("id", ""))
    author = str(event.get("from", ""))
    body = obj(event.get("body", {}))
    if kind == "artifact.recorded":
        rev = obj(body.get("artifact_revision", {}))
        art_id = str(rev.get("artifact_id", ""))
        art_body = obj(rev.get("body", {}))
        if "reading_table_source" in art_body:
            return f"Source story ({art_id}) by {author}"
        if "reading_table_practice" in art_body:
            practice = obj(art_body["reading_table_practice"])
            org = str(practice.get("organizes_by", ""))
            return f"Practice ({art_id}) organized by {org} by {author}"
        return f"Artifact ({art_id}) by {author}"
    if kind == "objection.recorded":
        obj_body = obj(body.get("objection", {}))
        target_art = str(obj_body.get("artifact_id", ""))
        return f"Objection to {target_art} by {author}"
    if kind == "decline.recorded":
        dec_body = obj(body.get("decline", {}))
        target_art = str(dec_body.get("artifact_id", ""))
        return f"Decline on {target_art} by {author}"
    if kind == "message.recorded":
        msg = obj(body.get("message", {}))
        msg_body = obj(msg.get("body", {}))
        if "practice_adoption" in msg_body:
            scope = str(obj(msg_body["practice_adoption"]).get("scope", ""))
            return f"Adoption statement for scope '{scope}' by {author}"
        return f"Message by {author}"
    return f"Event {ident} ({kind}) by {author}"


def record_full_text(event: JsonObject, original: str) -> str:
    summary = record_summary(event)
    return f"[{event.get('id')}] {summary}\nRaw: {original}"


def record_priority(event: JsonObject, lens_name: str) -> int:
    kind = str(event.get("kind", ""))
    body = obj(event.get("body", {}))
    if lens_name == "chronological":
        return client._integer(event.get("sequence"), 0) or 0
    if lens_name == "passages":
        if kind == "artifact.recorded":
            art_body = obj(obj(body.get("artifact_revision", {})).get("body", {}))
            if "reading_table_source" in art_body:
                return 1
            if "reading_table_practice" in art_body and obj(art_body["reading_table_practice"]).get("organizes_by") == "passages":
                return 2
        if kind in ("objection.recorded", "decline.recorded"):
            return 3
        return 4
    if lens_name == "open_questions":
        if kind in ("objection.recorded", "decline.recorded"):
            return 1
        if kind == "artifact.recorded":
            art_body = obj(obj(body.get("artifact_revision", {})).get("body", {}))
            if "reading_table_practice" in art_body and obj(art_body["reading_table_practice"]).get("organizes_by") == "open_questions":
                return 2
            if "reading_table_source" in art_body:
                return 3
        return 4
    return 10


def build_context(
    originals: list[str],
    lens: JsonObject,
    *,
    boundary_sequence: int | None = None,
) -> JsonObject:
    validated_lens = lens_config(lens)
    events: list[JsonObject] = []
    for original in originals:
        decoded = client._decode(original.encode("utf-8"))
        events.append(decoded)

    # Filter to permitted caller observation boundary
    observed_pairs: list[tuple[JsonObject, str]] = []
    for event, original in zip(events, originals, strict=True):
        seq = client._integer(event.get("sequence"), 0)
        if boundary_sequence is not None and seq is not None and seq > boundary_sequence:
            continue
        observed_pairs.append((event, original))

    lens_name = str(validated_lens["name"])
    budget = validated_lens.get("byte_budget")
    max_bytes = client._integer(budget, 1) if budget is not None else None

    # Sort candidates by priority according to lens
    candidates = list(range(len(observed_pairs)))
    if lens_name != "chronological":
        candidates.sort(key=lambda idx: (record_priority(observed_pairs[idx][0], lens_name), idx))

    delivered_sections: list[str] = []
    current_bytes = 0
    accounting_records: dict[int, JsonObject] = {}

    for idx in candidates:
        event, original = observed_pairs[idx]
        event_id = str(event.get("id"))
        seq = client._integer(event.get("sequence"), 0)
        full_text = record_full_text(event, original)
        summary = record_summary(event)
        link = f"[{event_id}] {summary} (selector available)"
        full_len = len(full_text.encode("utf-8"))
        link_len = len(link.encode("utf-8"))

        route = table.selector(event, original)

        if max_bytes is None:
            status = "delivered_in_full"
            delivered_sections.append(full_text)
            current_bytes += full_len
        elif current_bytes + full_len + 2 <= max_bytes:
            status = "delivered_in_full"
            delivered_sections.append(full_text)
            current_bytes += full_len + 2
        elif current_bytes + link_len + 2 <= max_bytes:
            status = "linked_only"
            delivered_sections.append(link)
            current_bytes += link_len + 2
        else:
            status = "omitted_by_budget"

        accounting_records[idx] = {
            "event_id": event_id,
            "sequence": seq,
            "author": event.get("from"),
            "kind": event.get("kind"),
            "status": status,
            "route": route,
        }

    # Order accounting records in observation order
    ordered_accounting = [accounting_records[i] for i in range(len(observed_pairs))]

    has_omissions = any(row["status"] in ("omitted_by_budget", "omitted_by_lens", "linked_only") for row in ordered_accounting)
    coverage = "selection" if has_omissions else "complete"

    objection_in_obs = any(row["kind"] in ("objection.recorded", "decline.recorded") for row in ordered_accounting)
    objection_in_del = any(
        row["kind"] in ("objection.recorded", "decline.recorded") and row["status"] == "delivered_in_full"
        for row in ordered_accounting
    )

    delivered_text = "\n\n".join(delivered_sections)

    return {
        "format": FORMAT,
        "lens": validated_lens,
        "observation": {
            "total_records": len(observed_pairs),
            "boundary_sequence": boundary_sequence,
        },
        "delivered_context": delivered_text,
        "delivered_bytes": len(delivered_text.encode("utf-8")),
        "accounting": {
            "records": ordered_accounting,
            "coverage": coverage,
            "objection_present_in_observation": objection_in_obs,
            "objection_delivered_in_context": objection_in_del,
            "omission_reason": "budget_constraint" if has_omissions else "none",
        },
        "group_state": "not_inferred",
        "consensus": "not_evaluated",
        "route_to_originals": "exact_event_or_reading_table_selector",
    }


def inspect_view(
    fixture_path: Path,
    lens: JsonObject,
    binary: Path,
    *,
    boundary_sequence: int | None = None,
) -> JsonObject:
    try:
        raw, package, originals = table.load(fixture_path)
        table.validate(originals, package["world"], binary)
        view = build_context(originals, lens, boundary_sequence=boundary_sequence)
        result: JsonObject = {
            "outcome": "inspected",
            "format": FORMAT,
            "world": package["world"],
            "fixture_sha256": digest(raw),
            **view,
        }
        if len(encoded(result)) > MAX_OUTPUT:
            raise ViewError("result_too_large")
        return result
    except ViewError as error:
        return {"outcome": "failed", "code": str(error)}
    except (ValueError, OSError, KeyError, TypeError, UnicodeError):
        return {"outcome": "failed", "code": "invalid_fixture"}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--reader-binary", type=Path, required=True)
    parser.add_argument("--lens", choices=ALLOWED_LENSES, default="chronological")
    parser.add_argument("--budget", type=int, default=None)
    parser.add_argument("--author", default="agent:lens-maker")
    parser.add_argument("--purpose", default="reading navigation")
    parser.add_argument("--version", default="0.1-example")
    parser.add_argument("--boundary", type=int, default=None)
    args = parser.parse_args()

    cfg: JsonObject = {
        "name": args.lens,
        "author": args.author,
        "purpose": args.purpose,
        "version": args.version,
        "byte_budget": args.budget,
    }
    result = inspect_view(args.fixture, cfg, args.reader_binary, boundary_sequence=args.boundary)
    print(encoded(result).decode("ascii"))
    return 1 if result["outcome"] == "failed" else 0


if __name__ == "__main__":
    raise SystemExit(main())
