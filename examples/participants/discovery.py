"""Disclosed discovery fixture using existing HTTP history and the native reader.

Offers are attributed invitations, not assignments or grants. This example never
executes artifacts or dispatches a participant. Export is permitted only for this
operator-owned synthetic fixture.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import secrets
import subprocess
import tempfile
from datetime import datetime, timezone
from pathlib import Path

import collaboration as client
import local_participant as wire
import mock_collaboration as mock
import reader_collaboration as reader
import walk

JsonObject = dict[str, object]
WORLD = "civ:bounded-discovery"
GUIDE, SOURCE, NEW, HIDDEN = ("agent:offer-guide", "agent:offer-source", "agent:offer-new", "agent:offer-hidden")
LIMITS: JsonObject = {"max_pages": 20, "max_events": 128, "max_response_bytes": 131072,
    "max_record_bytes": 16000, "max_total_bytes": 262144, "seconds": 30, "process_seconds": 40}


def configure(path: Path, tokens: dict[str, str], visibility: str) -> None:
    if visibility not in ("members", "addressed", "sender_only"):
        raise ValueError("invalid visibility")
    mock.save(path, {"world_id": WORLD, "title": "Bounded discovery fixture",
        "database_path": str(path.with_suffix(".sqlite")), "listen": "127.0.0.1:0",
        "visibility": visibility, "retention_seconds": 86400, "max_payload_bytes": 16384,
        "credentials": [{"principal": name, "token": token, "read": True, "write": name != NEW}
            for name, token in tokens.items()]})


def publish(origin: str, token: str, record: JsonObject) -> JsonObject:
    """Submit without an unbounded Python history read; native traversal verifies it."""
    discovery = wire.discover(origin)
    endpoint = client.object_value(discovery["endpoints"])["collaborate"]
    if discovery.get("id") != WORLD or not isinstance(endpoint, str) or not wire._same_origin(origin, endpoint):
        raise ValueError("invalid discovery")
    status, raw = wire.exchange("POST", endpoint, token=token, body=json.dumps(record).encode("utf-8"))
    if status != 200:
        wire._fail(status, raw)
    receipt = client.decode(raw)
    if (receipt.get("record_id") != record["id"] or receipt.get("status") != "recorded"
        or receipt.get("world") != WORLD or not isinstance(receipt.get("event_id"), str)
        or not receipt.get("event_id") or type(receipt.get("sequence")) is not int
        or token in json.dumps(receipt, ensure_ascii=False)):
        raise ValueError("invalid receipt")
    return receipt


def artifact(author: str, name: str, text: str, *, revision: int = 1) -> JsonObject:
    value: JsonObject = {"protocol_version": "0.1-draft", "type": "artifact_revision",
        "id": f"submission:{name}-{revision}", "world": WORLD, "from": author,
        "to": [NEW, GUIDE, SOURCE], "artifact_id": f"artifact:{name}",
        "media_type": "application/json", "body": {"text": text}}
    if revision > 1:
        value["derived_from"] = {"from": author, "artifact_id": f"artifact:{name}", "revision": revision - 1}
    return value


def seed(origin: str, tokens: dict[str, str], *, extra_recipient: str | None = None) -> str:
    if extra_recipient is not None and (extra_recipient not in tokens or extra_recipient == HIDDEN):
        raise ValueError("invalid extra recipient")
    extra = [extra_recipient] if extra_recipient is not None and extra_recipient != NEW else []

    def visible_artifact(author: str, name: str, text: str, *, revision: int = 1) -> JsonObject:
        result = artifact(author, name, text, revision=revision)
        audience = mock.list_strings(result["to"])
        result["to"] = audience + [principal for principal in extra if principal not in audience]
        return result

    old = publish(origin, tokens[SOURCE], visible_artifact(SOURCE, "notes", "Old claim: one page is complete."))
    corrected = publish(origin, tokens[SOURCE], visible_artifact(SOURCE, "notes", "Correction: inspect all permitted pages.", revision=2))
    parent = publish(origin, tokens[GUIDE], visible_artifact(GUIDE, "parent", "An unfinished reader plan; objections remain open."))
    publish(origin, tokens[GUIDE], {"protocol_version": "0.1-draft", "type": "objection",
        "id": "submission:reader-objection", "world": WORLD, "from": GUIDE, "to": [NEW, GUIDE, *[principal for principal in extra if principal != GUIDE]],
        "target_from": GUIDE, "artifact_id": "artifact:parent", "revision": 1,
        "body": {"text": "A first-page reader omits later originals. This objection remains unresolved."}})
    for number in range(100):
        wire.record_message(origin, tokens[GUIDE], GUIDE, [NEW, *extra], text=f"Pagination fixture {number}",
            message_id=f"message:discovery-padding-{number}")
    offer = visible_artifact(GUIDE, "reader-offer", "Optional reader repair invitation.")
    offered: JsonObject = {
        "format": "agentciv-activity-offer/0.1-example", "title": "Inspect an unfinished reader",
        "purpose": "Choose whether to inspect and repair a reader using permitted originals.",
        "offered_scope": "Read-only inspection; participation is optional.",
        "limitations": "Unresolved objection; no assignment, access grant or acceptance is implied.",
        "copying_conditions": "Operator permits synthetic fixture export only.",
        "source_refs": [{"event_id": corrected["event_id"], "from": SOURCE, "artifact_id": "artifact:notes", "revision": 2},
            {"event_id": old["event_id"], "from": SOURCE, "artifact_id": "artifact:notes", "revision": 1},
            {"event_id": parent["event_id"], "from": GUIDE, "artifact_id": "artifact:parent", "revision": 1}]}
    offer["body"] = {**client.object_value(offer["body"]), "activity_offer": offered}
    publish(origin, tokens[GUIDE], offer)
    hidden = artifact(HIDDEN, "hidden-offer", "Restricted fixture sentinel.")
    hidden["to"] = [HIDDEN]
    hidden["body"] = {**client.object_value(hidden["body"]), "activity_offer": offered}
    publish(origin, tokens[HIDDEN], hidden)
    return str(corrected["event_id"])


def view(binary: Path, origin: str, token: str, output: Path, label: str, query: str = "reader") -> JsonObject:
    if (output / f"{label}-view.json").exists():
        raise ValueError("offer_view_already_exists")
    started = datetime.now(timezone.utc).isoformat()
    originals = output / f"{label}-originals.json"
    _, transport = reader.read_snapshot(binary, origin, token, WORLD, originals_path=originals)
    snapshot = client.decode(originals.read_bytes())
    snapshot.pop("copying_condition")
    with tempfile.TemporaryDirectory(prefix="agentciv-offer-input-") as temporary:
        path = Path(temporary) / "input.json"
        mock.save(path, {"snapshot": snapshot, "report": transport})
        try:
            completed = subprocess.run([str(binary), "offers", str(path), query], capture_output=True, timeout=40, check=False)
        except (OSError, subprocess.TimeoutExpired):
            raise ValueError("offer_projection_process_failed") from None
    if completed.returncode != 0 or len(completed.stdout) > 1_048_576:
        raise ValueError("offer_projection_failed")
    result = client.decode(completed.stdout)
    if result.get("format") != "agentciv-offer-view/0.1-example" or result.get("world") != WORLD:
        raise ValueError("invalid_offer_projection")
    if token in json.dumps(result, ensure_ascii=False):
        raise ValueError("credential_reflected")
    result["observation"] = {"retrieval_started": started, "retrieval_finished": datetime.now(timezone.utc).isoformat(),
        "configured_budgets": LIMITS, "freshness": "bounded_full_revalidation_not_an_atomic_snapshot",
        "copying_condition": "operator_authorized_synthetic_fixture_export_only", "choice": "inspect_only_no_dispatch"}
    mock.save(output / f"{label}-view.json", result)
    return result


def verify(first: JsonObject, returned: JsonObject, visibility: str) -> None:
    """Check observed sources separately from projection syntax and publication."""
    for value, expected in ((first, "available"), (returned, "withdrawn")):
        offers = value.get("offers")
        if not isinstance(offers, list):
            raise ValueError("invalid_offer_list")
        if visibility == "sender_only":
            if offers:
                raise ValueError("sender_only_disclosed_offer")
            continue
        expected_count = 2 if visibility == "members" else 1
        report = value.get("report")
        if (len(offers) != expected_count or not isinstance(report, dict) or report.get("pages") != 2
            or any(not isinstance(offer, dict) for offer in offers)):
            raise ValueError("incomplete_discovery")
        visible = [client.object_value(offer) for offer in offers]
        selected = next((offer for offer in visible if offer.get("from") == GUIDE), None)
        if selected is None:
            raise ValueError("offered_scope_missing")
        sources = selected.get("sources")
        related = selected.get("related_records")
        if not isinstance(sources, list) or not isinstance(related, list):
            raise ValueError("inspection_evidence_missing")
        source = next((client.object_value(item) for item in sources
            if client.object_value(item).get("from") == SOURCE and client.object_value(item).get("revision") == 2), {})
        if source.get("status") != expected:
            raise ValueError("source_freshness_failed")
        exact = source.get("record_utf8")
        if not isinstance(exact, str):
            raise ValueError("source_original_missing")
        event = client.decode(exact.encode("utf-8"))
        if event.get("id") != source.get("event_id") or event.get("world") != WORLD:
            raise ValueError("source_identity_failed")
        if expected == "available":
            original = client.object_value(client.object_value(event.get("body")).get("artifact_revision"))
            if (event.get("kind") != "artifact.recorded" or original.get("from") != SOURCE
                or original.get("artifact_id") != "artifact:notes" or original.get("revision") != 2):
                raise ValueError("source_identity_failed")
        if expected == "withdrawn" and (event.get("kind") != "artifact.withdrawn" or event.get("body") != {}):
            raise ValueError("withdrawn_source_disclosed")
        originals = [client.decode(str(client.object_value(item)["record_utf8"]).encode("utf-8")) for item in related]
        if not any(event.get("kind") == "objection.recorded"
            and client.object_value(client.object_value(event.get("body")).get("objection")).get("target_from") == GUIDE
            and client.object_value(client.object_value(event.get("body")).get("objection")).get("artifact_id") == "artifact:parent"
            for event in originals):
            raise ValueError("objection_missing")
        if visibility == "addressed" and "Restricted fixture sentinel" in json.dumps(value):
            raise ValueError("restricted_material_disclosed")


def run(output: Path, *, host: str = "python", visibility: str = "addressed",
        reader_binary: Path | None = None, host_binary: Path | None = None) -> JsonObject:
    if host not in ("python", "rust") or visibility not in ("members", "addressed", "sender_only"):
        raise ValueError("invalid configuration")
    source = reader.source_identity()
    output.mkdir(parents=True, exist_ok=False)
    binary = reader_binary or walk.build_rust_binaries("agentciv-reader")["agentciv-reader"]
    native_host = host_binary or (walk.rust_binary() if host == "rust" else None)
    binary_hashes = {"reader": hashlib.sha256(binary.read_bytes()).hexdigest()}
    if host == "rust" and native_host is not None:
        binary_hashes["host"] = hashlib.sha256(native_host.read_bytes()).hexdigest()
    with tempfile.TemporaryDirectory(prefix="agentciv-discovery-private-") as temporary:
        config = Path(temporary) / "world.json"
        tokens = {name: secrets.token_urlsafe(24) for name in (GUIDE, SOURCE, NEW, HIDDEN)}
        configure(config, tokens, visibility)
        argv = walk.python_argv(config) if host == "python" else [str(native_host), "--config", str(config)]
        running = walk.start_host(argv)
        try:
            origin = walk.wait_until_ready(running)
            corrected_id = seed(origin, tokens)
            first = view(binary, origin, tokens[NEW], output, "first")
            receipt = publish(origin, tokens[SOURCE], {"protocol_version": "0.1-draft", "type": "withdrawal",
                "id": "submission:withdraw-source", "world": WORLD, "from": SOURCE,
                "target_from": SOURCE, "artifact_id": "artifact:notes", "revision": 2})
            if receipt.get("event_id") != corrected_id:
                raise ValueError("withdrawal_identity_changed")
            returned = view(binary, origin, tokens[NEW], output, "return")
            verify(first, returned, visibility)
        finally:
            walk.stop_host(running)
        old_token = tokens[NEW]
        tokens[NEW] = secrets.token_urlsafe(24)
        configure(config, tokens, "sender_only")
        running = walk.start_host(argv)
        try:
            origin = walk.wait_until_ready(running)
            try:
                reader.read_snapshot(binary, origin, old_token, WORLD)
            except reader.ReaderSnapshotError as error:
                revoked = error.code == "authentication_required"
            else:
                revoked = False
            restricted = view(binary, origin, tokens[NEW], output, "restricted")
        finally:
            walk.stop_host(running)
    report: JsonObject = {"outcome": "passed", "host": host, "visibility": visibility,
        "source": source, "binaries_sha256": binary_hashes,
        "condition": "operator_scripted_discovery_not_model_behavior", "first": first, "return": returned,
        "restricted": restricted, "old_credential_rejected": revoked,
        "scope": "two_repository_hosts_not_independent_interoperability",
        "recovery": "full_traversal_from_origin_no_cached_view_fallback"}
    if not revoked or restricted.get("offers") != []:
        raise ValueError("access_change_failed")
    if reader.source_identity() != source:
        raise ValueError("source_changed")
    report["source_unchanged"] = True
    mock.save(output / "report.json", report)
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--host", choices=("python", "rust"), default="python")
    parser.add_argument("--visibility", choices=("members", "addressed", "sender_only"), default="addressed")
    args = parser.parse_args()
    try:
        report = run(args.output, host=args.host, visibility=args.visibility)
    except (ValueError, OSError, wire.ParticipantError, client.DecisionError, walk.WalkFailure,
            KeyError, subprocess.SubprocessError):
        print(json.dumps({"outcome": "failed", "code": "discovery_experiment_failed"}))
        return 1
    print(json.dumps({"outcome": report["outcome"], "host": args.host, "visibility": args.visibility}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
