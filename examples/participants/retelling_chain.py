"""Inspect synthetic retelling ancestry, apparent consensus, and delayed corrections offline.

Exact original strings remain the evidence. This optional example convention is
not a protocol, copying grant, authenticated package, or participant scheduler.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import stat
import subprocess
import tempfile
from pathlib import Path
from typing import Any

import encounter_client as client

JsonObject = dict[str, Any]
FORMAT = "agentciv-retelling-table/0.1-example"
COPYING = "operator_authorized_synthetic_fixture_export_only"
MAX_INPUT = 1_048_576
MAX_RECORDS = 32
MAX_OUTPUT = 65_536
TEMP_PREFIX = "agentciv-retelling-table-"
PROPOSAL_FORMAT = "agentciv-retelling-proposal/0.1-example"
RECORD_FORMAT = "agentciv-retelling-record/0.1-example"
WITHDRAWAL_FORMAT = "agentciv-claim-withdrawal/0.1-example"
ADOPTION_FORMAT = "agentciv-reading-table-adoption/0.1-example"


class RetellingError(ValueError):
    """Only fixed codes, never untrusted input, are printable."""


def obj(value: object) -> JsonObject:
    if not isinstance(value, dict):
        raise RetellingError("invalid_fixture")
    return value


def rows(value: object) -> list[object]:
    if not isinstance(value, list):
        raise RetellingError("invalid_fixture")
    return value


def text(value: object, maximum: int = 1024) -> str:
    if not isinstance(value, str) or not 1 <= len(value.encode("utf-8")) <= maximum:
        raise RetellingError("invalid_fixture")
    return value


def digest(raw: bytes) -> str:
    return hashlib.sha256(raw).hexdigest()


def encoded(value: object) -> bytes:
    # Escaping keeps exact originals round-trippable on a terminal.
    return json.dumps(value, ensure_ascii=True, allow_nan=False).encode("ascii")


def load(path: Path) -> tuple[bytes, JsonObject, list[str]]:
    try:
        if path.is_symlink() or not path.is_file():
            raise RetellingError("input_unavailable")
        with path.open("rb") as stream:
            raw = stream.read(MAX_INPUT + 1)
        if len(raw) > MAX_INPUT:
            raise RetellingError("input_too_large")
        value = client._decode(raw)
        if (
            set(value) != {"format", "world", "copying_condition", "records", "producer_report"}
            or value.get("format") != FORMAT
            or value.get("copying_condition") != COPYING
        ):
            raise RetellingError("invalid_fixture")
        text(value.get("world"), 1024)
        originals: list[str] = []
        entries = rows(value.get("records"))
        if len(entries) > MAX_RECORDS:
            raise RetellingError("input_too_large")
        for entry in entries:
            item = obj(entry)
            if set(item) != {"original_utf8", "sha256"}:
                raise RetellingError("invalid_fixture")
            original = text(item.get("original_utf8"), 16000)
            if digest(original.encode("utf-8")) != item.get("sha256"):
                raise RetellingError("original_hash_mismatch")
            originals.append(original)
        # producer_report is ignored: winner, consensus, and independent count flags are unverified.
        return raw, value, originals
    except OSError:
        raise RetellingError("input_unavailable") from None


def cleanup(directory: Path, parent: Path) -> None:
    """Remove only this newly allocated directory and its one known input."""
    try:
        info = directory.lstat()
        if (
            not stat.S_ISDIR(info.st_mode)
            or getattr(info, "st_file_attributes", 0) & 0x400
            or directory.resolve().parent != parent
            or not directory.name.startswith(TEMP_PREFIX)
            or set(p.name for p in directory.iterdir()) - {"originals.json"}
        ):
            raise RetellingError("validation_cleanup_failed")
        path = directory / "originals.json"
        if path.exists() or path.is_symlink():
            item_info = path.lstat()
            if not stat.S_ISREG(item_info.st_mode) or item_info.st_nlink != 1 or getattr(item_info, "st_file_attributes", 0) & 0x400:
                raise RetellingError("validation_cleanup_failed")
            path.unlink()
        directory.rmdir()
    except OSError:
        raise RetellingError("validation_cleanup_failed") from None


def validate_native(originals: list[str], world: object, binary: Path) -> JsonObject:
    """Reuse existing strict native reader corrections projection to revalidate syntax."""
    native = {
        "snapshot": {"world": world, "records": originals},
        "report": {
            "pages": 1,
            "events": len(originals),
            "response_bytes": sum(len(row.encode("utf-8")) for row in originals),
            "reached_end": True,
            "scope": "current_caller_view",
            "copying_permission": "not_granted",
        },
    }
    try:
        directory = Path(tempfile.mkdtemp(prefix=TEMP_PREFIX)).absolute()
        parent = directory.parent.resolve()
        try:
            path = directory / "originals.json"
            path.write_bytes(encoded(native))
            process = subprocess.run(
                [str(binary), "corrections", str(path)],
                capture_output=True,
                timeout=40,
                check=False,
            )
        finally:
            cleanup(directory, parent)
        if process.returncode != 0 or process.stderr or len(process.stdout) > MAX_INPUT:
            raise RetellingError("native_validation_failed")
        result = json.loads(process.stdout.decode("utf-8"))
        if result.get("format") != "agentciv-correction-view/0.1-example" or result.get("world") != world:
            raise RetellingError("native_validation_failed")
        return obj(result)
    except (OSError, subprocess.SubprocessError, json.JSONDecodeError):
        raise RetellingError("native_validation_failed") from None


def artifact(event: JsonObject) -> JsonObject | None:
    if event.get("kind") != "artifact.recorded":
        return None
    return obj(obj(event.get("body")).get("artifact_revision"))


def tuple_of(record: JsonObject) -> tuple[str, str, int]:
    actor = text(record.get("from"))
    art_id = text(record.get("artifact_id"))
    rev = client._integer(record.get("revision"), 1)
    if rev is None:
        raise RetellingError("invalid_revision")
    return actor, art_id, rev


def selector_of(event: JsonObject, original: str) -> JsonObject:
    result: JsonObject = {
        "event_id": event["id"],
        "sequence": client._integer(event["sequence"]),
        "kind": event["kind"],
        "original_sha256": digest(original.encode("utf-8")),
    }
    record = artifact(event)
    if record is not None:
        result.update(
            {
                "from": record["from"],
                "artifact_id": record["artifact_id"],
                "revision": client._integer(record["revision"], 1),
            }
        )
    return result


def reconstruct_retellings(originals: list[str]) -> JsonObject:
    events = [client._decode(original.encode("utf-8")) for original in originals]
    by_id = {str(event["id"]): (event, original) for event, original in zip(events, originals, strict=True)}
    all_selectors = [selector_of(event, original) for event, original in zip(events, originals, strict=True)]

    # 1. Collect artifacts, derivations, and revisions
    proposals: list[JsonObject] = []
    retellings: list[JsonObject] = []
    objections: list[JsonObject] = []
    declines: list[JsonObject] = []
    withdrawals: list[JsonObject] = []
    adoptions: list[JsonObject] = []
    non_adopting_notes: list[JsonObject] = []

    # Map of (from, artifact_id, revision) -> (event, original, artifact_record)
    artifact_map: dict[tuple[str, str, int], tuple[JsonObject, str, JsonObject]] = {}
    derivation_parent: dict[tuple[str, str, int], tuple[str, str, int]] = {}

    for event, original in zip(events, originals, strict=True):
        record = artifact(event)
        if record is not None:
            ident = tuple_of(record)
            if ident in artifact_map:
                raise RetellingError("duplicate_artifact_revision")
            artifact_map[ident] = (event, original, record)
            if "derived_from" in record and record["derived_from"] is not None:
                parent_ident = tuple_of(obj(record["derived_from"]))
                # Derivation parent must be previously recorded in chronological sequence
                if parent_ident not in artifact_map:
                    raise RetellingError("invalid_derivation_parent")
                parent_event = artifact_map[parent_ident][0]
                p_seq = client._integer(parent_event["sequence"])
                e_seq = client._integer(event["sequence"])
                if p_seq is None or e_seq is None or p_seq >= e_seq:
                    raise RetellingError("inconsistent_temporal_sequence")
                derivation_parent[ident] = parent_ident

            body = obj(record["body"])
            if "retelling_proposal" in body:
                prop = obj(body["retelling_proposal"])
                if prop.get("format") != PROPOSAL_FORMAT:
                    raise RetellingError("invalid_proposal")
                proposals.append(
                    {
                        "selector": selector_of(event, original),
                        "title": text(prop.get("title")),
                        "text": text(prop.get("text")),
                    }
                )
            elif "retelling_record" in body:
                retell = obj(body["retelling_record"])
                if retell.get("format") != RECORD_FORMAT:
                    raise RetellingError("invalid_retelling")
                if any(type(retell.get(key)) is not bool for key in ("recorded_objections_noted", "claims_consensus")):
                    raise RetellingError("invalid_retelling")
                if "cites_source" in retell:
                    citation = obj(retell["cites_source"])
                    cited = by_id.get(text(citation.get("event_id")))
                    parent = derivation_parent.get(ident)
                    if cited is None or parent is None:
                        raise RetellingError("invalid_source_selector")
                    cited_record = artifact(cited[0])
                    if cited_record is None or tuple_of(citation) != parent or tuple_of(cited_record) != parent:
                        raise RetellingError("invalid_source_selector")
                retellings.append(
                    {
                        "selector": selector_of(event, original),
                        "ident": {"from": ident[0], "artifact_id": ident[1], "revision": ident[2]},
                        "parent": (
                            {"from": derivation_parent[ident][0], "artifact_id": derivation_parent[ident][1], "revision": derivation_parent[ident][2]}
                            if ident in derivation_parent
                            else None
                        ),
                        "summary": text(retell.get("summary")),
                        "recorded_objections_noted": bool(retell.get("recorded_objections_noted")),
                        "claims_consensus": bool(retell.get("claims_consensus")),
                        "basis_stated_age": str(retell.get("basis_stated_age")) if "basis_stated_age" in retell else None,
                    }
                )

        elif event["kind"] == "objection.recorded":
            obj_body = obj(obj(event["body"])["objection"])
            target = (text(obj_body["target_from"]), text(obj_body["artifact_id"]), client._integer(obj_body["revision"], 1))
            if target[2] is None:
                raise RetellingError("invalid_revision")
            objections.append(
                {
                    "selector": selector_of(event, original),
                    "from": text(obj_body["from"]),
                    "target": {"from": target[0], "artifact_id": target[1], "revision": target[2]},
                    "text": text(obj(obj_body["body"]).get("text")),
                }
            )

        elif event["kind"] == "decline.recorded":
            dec_body = obj(obj(event["body"])["decline"])
            target = (text(dec_body["target_from"]), text(dec_body["artifact_id"]), client._integer(dec_body["revision"], 1))
            if target[2] is None:
                raise RetellingError("invalid_revision")
            declines.append(
                {
                    "selector": selector_of(event, original),
                    "from": text(dec_body["from"]),
                    "target": {"from": target[0], "artifact_id": target[1], "revision": target[2]},
                    "text": text(obj(dec_body["body"]).get("text")),
                }
            )

        elif event["kind"] == "message.recorded":
            msg = obj(obj(event["body"])["message"])
            msg_body = obj(msg["body"])
            if "claim_withdrawal" in msg_body:
                w = obj(msg_body["claim_withdrawal"])
                if w.get("format") != WITHDRAWAL_FORMAT:
                    raise RetellingError("invalid_withdrawal")
                target_ref = obj(w["target"])
                withdrawals.append(
                    {
                        "selector": selector_of(event, original),
                        "from": text(msg["from"]),
                        "target": {
                            "event_id": text(target_ref["event_id"]),
                            "from": text(target_ref["from"]),
                            "artifact_id": text(target_ref["artifact_id"]),
                            "revision": client._integer(target_ref["revision"], 1),
                        },
                        "statement": text(w.get("statement")),
                    }
                )
            elif "practice_adoption" in msg_body:
                adopt = obj(msg_body["practice_adoption"])
                if adopt.get("format") != ADOPTION_FORMAT:
                    raise RetellingError("invalid_adoption")
                adoptions.append(
                    {
                        "selector": selector_of(event, original),
                        "from": text(msg["from"]),
                        "practice": obj(adopt["practice"]),
                        "scope": text(adopt["scope"]),
                        "statement": text(adopt["statement"]),
                    }
                )
            else:
                non_adopting_notes.append(
                    {
                        "selector": selector_of(event, original),
                        "from": text(msg["from"]),
                        "content": msg_body,
                    }
                )

    # 2. Trace root ancestry for all retellings (and detect cycles)
    def trace_root(ident: tuple[str, str, int]) -> tuple[tuple[str, str, int], int]:
        visited: set[tuple[str, str, int]] = {ident}
        curr = ident
        depth = 0
        while curr in derivation_parent:
            parent = derivation_parent[curr]
            if parent in visited:
                raise RetellingError("cycle_detected_in_derivation")
            visited.add(parent)
            curr = parent
            depth += 1
        return curr, depth

    # 3. Analyze retelling chains, shared roots, and distinct author repetition
    root_groups: dict[tuple[str, str, int], list[JsonObject]] = {}
    for r in retellings:
        ident_tuple = (r["ident"]["from"], r["ident"]["artifact_id"], r["ident"]["revision"])
        root_ident, depth = trace_root(ident_tuple)
        r["root_source"] = {"from": root_ident[0], "artifact_id": root_ident[1], "revision": root_ident[2]}
        r["depth"] = depth
        root_groups.setdefault(root_ident, []).append(r)

    ancestry_analyses: list[JsonObject] = []
    for root_ident, group in root_groups.items():
        distinct_authors = sorted({item["ident"]["from"] for item in group})
        ancestry_analyses.append(
            {
                "root_source": {"from": root_ident[0], "artifact_id": root_ident[1], "revision": root_ident[2]},
                "descendant_count": len(group),
                "distinct_author_count": len(distinct_authors),
                "distinct_authors": distinct_authors,
                "independent_support_established": False,
                "lineage_finding": "Multiple descendants trace to a single root proposal; repetition does not constitute independent corroboration.",
            }
        )

    # 4. Analyze pruning of recorded objections
    # Check if root or parent had recorded objections that were omitted
    objections_by_target: dict[tuple[str, str, int], list[JsonObject]] = {}
    for obj_item in objections:
        t_tup = (obj_item["target"]["from"], obj_item["target"]["artifact_id"], obj_item["target"]["revision"])
        objections_by_target.setdefault(t_tup, []).append(obj_item)

    retelling_evaluations: list[JsonObject] = []
    for r in retellings:
        ident_tuple = (r["ident"]["from"], r["ident"]["artifact_id"], r["ident"]["revision"])
        root_ident, _ = trace_root(ident_tuple)
        root_has_objection = root_ident in objections_by_target
        r_eval: JsonObject = {
            "artifact": r["ident"],
            "summary": r["summary"],
            "notes_recorded_objection": r["recorded_objections_noted"],
            "claims_consensus": r["claims_consensus"],
            "evaluation_basis": "attributed_example_annotations_not_summary_semantics",
        }
        if root_has_objection and not r["recorded_objections_noted"]:
            r_eval["pruned_recorded_objections"] = True
            r_eval["omission_note"] = "The reteller's annotation reports omission of a recorded objection; summary semantics are not independently checked."
        else:
            r_eval["pruned_recorded_objections"] = False

        if r["claims_consensus"]:
            r_eval["consensus_claim_evaluation"] = {
                "apparent_consensus_claimed": True,
                "supported_by_records": False,
                "finding": "Attributed assertion; no unanimous agreement, office, or collective grant is recorded in permitted history.",
            }
        retelling_evaluations.append(r_eval)

    # 5. Analyze delayed corrections and stale basis citations
    # Look for author superseding revisions: author has r2 where r1 existed
    revisions_by_author_artifact: dict[tuple[str, str], list[int]] = {}
    for ident in artifact_map:
        revisions_by_author_artifact.setdefault((ident[0], ident[1]), []).append(ident[2])

    stale_basis_evaluations: list[JsonObject] = []
    for r in retellings:
        if r["parent"] is not None:
            p_from, p_art, p_rev = r["parent"]["from"], r["parent"]["artifact_id"], r["parent"]["revision"]
            all_known_revs = revisions_by_author_artifact.get((p_from, p_art), [])
            max_rev = max(all_known_revs) if all_known_revs else p_rev
            if max_rev > p_rev:
                r_seq = client._integer(r["selector"]["sequence"])
                earlier_revisions = [
                    rev for rev in all_known_revs
                    if rev > p_rev
                    and (seq := client._integer(artifact_map[(p_from, p_art, rev)][0]["sequence"])) is not None
                    and r_seq is not None and seq < r_seq
                ]
                earlier_revision = max(earlier_revisions) if earlier_revisions else None
                stale_basis_evaluations.append(
                    {
                        "retelling": r["ident"],
                        "cited_parent": r["parent"],
                        "latest_author_revision": max_rev,
                        "newer_revision_preceded_retelling": earlier_revision is not None,
                        "latest_author_revision_before_retelling": earlier_revision,
                        "staleness_finding": (
                            f"Derived from revision {p_rev} after author published revision {earlier_revision}."
                            if earlier_revision is not None else
                            f"Derived from revision {p_rev} before the retained newer author revisions."
                        ),
                    }
                )

    # 6. Attributed adoption vs non-adoption (refusal, decline, silence)
    non_adopting_summary = {
        "declined_participants": [d["from"] for d in declines],
        "silent_or_general_notes": [n["from"] for n in non_adopting_notes],
        "civic_status": "Silence, decline, and unfinished activity are preserved and not converted into adoption.",
    }

    return {
        "original_selectors": all_selectors,
        "proposals": proposals,
        "retellings": retellings,
        "objections": objections,
        "declines": declines,
        "withdrawals": withdrawals,
        "adoptions": adoptions,
        "ancestry_analyses": ancestry_analyses,
        "retelling_evaluations": retelling_evaluations,
        "stale_basis_evaluations": stale_basis_evaluations,
        "non_adopting_participants": non_adopting_summary,
        "group_state": "not_inferred",
        "silence_or_refusal": "not_converted_into_adoption",
        "consensus_authority": "none_inferred",
    }


def inspect(path: Path, binary: Path, event_id: str | None = None) -> JsonObject:
    try:
        raw, package, originals = load(path)
        native_view = validate_native(originals, package["world"], binary)
        findings = reconstruct_retellings(originals)
        result: JsonObject = {
            "outcome": "inspected",
            "format": FORMAT,
            "fixture_sha256": digest(raw),
            "world": package["world"],
            "scope": "frozen_synthetic_records_not_an_observed_http_traversal",
            "copying_authority": "operator_fixture_assertion_not_authenticated_or_granted",
            "origin_authenticated": False,
            "producer_report_used": False,
            "native_concerns_count": len(native_view.get("concerns", [])),
            **findings,
        }
        if event_id is not None:
            selected = next((row for row in originals if client._decode(row.encode("utf-8"))["id"] == event_id), None)
            if selected is None:
                raise RetellingError("event_unavailable")
            result = {
                "outcome": "opened",
                "format": FORMAT,
                "fixture_sha256": digest(raw),
                "original_utf8": selected,
                "original_sha256": digest(selected.encode("utf-8")),
                "origin_authenticated": False,
            }
        if len(encoded(result)) > MAX_OUTPUT:
            raise RetellingError("result_too_large")
        return result
    except RetellingError as error:
        return {"outcome": "failed", "code": str(error)}
    except (ValueError, OSError, KeyError, TypeError, UnicodeError, RecursionError):
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
