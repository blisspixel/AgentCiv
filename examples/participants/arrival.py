"""Bounded, operator-scripted local arrival using existing messages and collaboration.

The separate return caller rediscovers an optional contribution after restart.
No model, autonomous participation, running community, or useful repair is claimed.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import secrets
import subprocess
import tempfile
from pathlib import Path

import collaboration as client
import discovery
import local_participant as wire
import mock_collaboration as mock
import reader_collaboration as reader
import walk

JsonObject = dict[str, object]
CHOICES = ("inspect", "message", "revise", "object", "decline")
RETURN = "agent:arrival-return"
WORLD = discovery.WORLD
NEW = discovery.NEW
FORMAT = "agentciv-local-arrival/0.1-example"


class ArrivalError(ValueError):
    """Only fixed local stage and code labels cross the CLI boundary."""

    def __init__(self, stage: str, code: str) -> None:
        super().__init__(f"{stage}: {code}")
        self.stage = stage
        self.code = code


def object_value(value: object) -> JsonObject:
    try:
        return client.object_value(value)
    except client.DecisionError:
        raise ValueError("invalid arrival object") from None


def records(originals: list[str]) -> list[JsonObject]:
    return [client.decode(original.encode("utf-8")) for original in originals]


def selector(event: JsonObject) -> JsonObject:
    artifact = object_value(object_value(event.get("body")).get("artifact_revision"))
    return {"world": event["world"], "event_id": event["id"], "from": artifact["from"],
        "artifact_id": artifact["artifact_id"], "revision": artifact["revision"]}


def configure(path: Path, tokens: dict[str, str], choice: str) -> None:
    """Fixture grants belong to the operator, independently of any invitation."""
    discovery.configure(path, tokens, "addressed")
    config = client.decode(path.read_bytes())
    credentials = config.get("credentials")
    if not isinstance(credentials, list):
        raise ValueError("invalid credentials")
    revised: list[JsonObject] = []
    for item in credentials:
        credential = object_value(item)
        if credential.get("principal") == RETURN:
            credential["write"] = False
        if credential.get("principal") == NEW:
            credential["write"] = choice != "inspect"
        revised.append(credential)
    config["credentials"] = revised
    mock.save(path, config)


def capture(binary: Path, origin: str, token: str, output: Path, label: str) -> tuple[JsonObject, list[str]]:
    """A complete fresh traversal, with exact synthetic originals exported separately."""
    originals_path = output / f"{label}-originals.json"
    view_path = output / f"{label}-view.json"
    if originals_path.exists() or view_path.exists():
        raise ValueError("arrival capture already exists")
    # An unexpected host disclosure must not enter public evidence before validation.
    with tempfile.TemporaryDirectory(prefix="agentciv-arrival-capture-private-") as temporary:
        private = Path(temporary)
        view = discovery.view(binary, origin, token, private, label, query="")
        snapshot = client.decode((private / f"{label}-originals.json").read_bytes())
        originals = mock.list_strings(snapshot.get("records"))
        transport = object_value(view.get("report"))
        if (snapshot.get("world") != WORLD or transport.get("reached_end") is not True
            or transport.get("scope") != "current_caller_view" or transport.get("copying_permission") != "not_granted"
            or type(transport.get("events")) is not int or transport.get("events") != len(originals)):
            raise ValueError("incomplete arrival history")
        if token in json.dumps({"view": view, "originals": originals}, ensure_ascii=False):
            raise ValueError("credential reflected")
        if "Restricted fixture sentinel" in json.dumps({"view": view, "originals": originals}):
            raise ValueError("restricted material disclosed")
    # This is the disclosed example's choice, not the discovery helper's inspect-only condition.
    observation = object_value(view.get("observation"))
    observation["choice"] = "operator_scripted_arrival_condition_no_dispatch"
    view["observation"] = observation
    mock.save(originals_path, snapshot)
    mock.save(view_path, view)
    return view, originals


def _inspect_basis(view: JsonObject, originals: list[str]) -> JsonObject:
    """Resolve the seeded offer's material basis exclusively from permitted originals."""
    transport = object_value(view.get("report"))
    if (view.get("format") != "agentciv-offer-view/0.1-example" or view.get("world") != WORLD
        or transport.get("reached_end") is not True or transport.get("scope") != "current_caller_view"
        or transport.get("copying_permission") != "not_granted" or type(transport.get("events")) is not int
        or transport.get("events") != len(originals)):
        raise ValueError("incomplete arrival history")
    if "Restricted fixture sentinel" in json.dumps({"view": view, "originals": originals}):
        raise ValueError("restricted material disclosed")
    events = records(originals)
    by_id = {event["id"]: (event, original) for event, original in zip(events, originals, strict=True)}
    if len(by_id) != len(events):
        raise ValueError("duplicate arrival event")
    offers = view.get("offers")
    if not isinstance(offers, list):
        raise ValueError("invalid arrival offers")
    candidates = [object_value(item) for item in offers if object_value(item).get("from") == discovery.GUIDE]
    if len(candidates) != 1:
        raise ValueError("arrival offer missing")
    offer = candidates[0]
    offered_original = offer.get("record_utf8")
    selected = by_id.get(offer.get("event_id"))
    if selected is None or selected[1] != offered_original or selected[0].get("kind") != "artifact.recorded":
        raise ValueError("arrival offer identity failed")
    if selector(selected[0]) != {key: offer.get(key) for key in ("world", "event_id", "from", "artifact_id", "revision")}:
        raise ValueError("arrival offer identity failed")
    sources = offer.get("sources")
    if not isinstance(sources, list):
        raise ValueError("arrival sources missing")
    available: list[tuple[JsonObject, str]] = []
    for item in sources:
        source = object_value(item)
        found = by_id.get(source.get("event_id"))
        if source.get("status") != "available" or found is None or source.get("record_utf8") != found[1]:
            raise ValueError("arrival source unavailable")
        if found[0].get("kind") != "artifact.recorded":
            raise ValueError("arrival source unavailable")
        expected = {"world": WORLD, **{key: source.get(key) for key in ("event_id", "from", "artifact_id", "revision")}}
        if selector(found[0]) != expected:
            raise ValueError("arrival source identity failed")
        available.append(found)
    parents = [(event, original) for event, original in available if selector(event)["from"] == discovery.GUIDE]
    corrected = [(event, original) for event, original in available if selector(event)["from"] == discovery.SOURCE
        and selector(event)["revision"] == 2]
    earlier = [(event, original) for event, original in available if selector(event)["from"] == discovery.SOURCE
        and selector(event)["revision"] == 1]
    if len(parents) != 1 or len(corrected) != 1 or len(earlier) != 1:
        raise ValueError("arrival basis incomplete")
    parent, parent_original = parents[0]
    correction, correction_original = corrected[0]
    old, old_original = earlier[0]
    if selector(correction)["artifact_id"] != selector(old)["artifact_id"]:
        raise ValueError("arrival source chain changed")
    parent_identity = selector(parent)
    related = offer.get("related_records")
    if not isinstance(related, list):
        raise ValueError("arrival objection missing")
    objections: list[tuple[JsonObject, str]] = []
    for item in related:
        reference = object_value(item)
        found = by_id.get(reference.get("event_id"))
        if found is None or reference.get("record_utf8") != found[1]:
            raise ValueError("arrival related original missing")
        event, original = found
        if event.get("kind") != "objection.recorded":
            continue
        objection = object_value(object_value(event.get("body")).get("objection"))
        if (objection.get("from") == discovery.GUIDE and objection.get("target_from") == parent_identity["from"]
            and objection.get("artifact_id") == parent_identity["artifact_id"]
            and objection.get("revision") == parent_identity["revision"]):
            objections.append((event, original))
    if len(objections) != 1:
        raise ValueError("arrival objection missing")
    objection, objection_original = objections[0]
    return {"offer": selector(selected[0]), "parent": parent_identity, "correction": selector(correction),
        "earlier_source": selector(old), "objection_event_id": objection["id"],
        "offer_record_utf8": offered_original, "parent_record_utf8": parent_original,
        "correction_record_utf8": correction_original, "earlier_source_record_utf8": old_original,
        "objection_record_utf8": objection_original}


def inspect_basis(view: JsonObject, originals: list[str]) -> JsonObject:
    try:
        return _inspect_basis(view, originals)
    except (ValueError, KeyError, TypeError, client.DecisionError):
        raise ArrivalError("source_inspection", "invalid_source_basis") from None


def _publish_choice(origin: str, token: str, choice: str, basis: JsonObject) -> tuple[JsonObject, JsonObject] | None:
    """Explicit fixture choice only. There is no invitation acceptance or dispatch loop."""
    if choice == "inspect":
        return None
    if choice not in CHOICES:
        raise ValueError("invalid arrival choice")
    descriptor = wire.discover(origin)
    if descriptor.get("id") != WORLD:
        raise ValueError("arrival world changed")
    parent = object_value(basis["parent"])
    correction = object_value(basis["correction"])
    references = [{key: item[key] for key in ("event_id", "from", "artifact_id", "revision")}
        for item in (parent, correction)]
    texts = {"message": "I inspected these originals. The open question remains: how should a return reader handle changes to older records?",
        "revise": "Unfinished arrival note. I inspected the parent and correction; the earlier objection remains open. This is not a repaired reader.",
        "object": "The parent remains unfinished. The source correction and original objection need to stay inspectable before reliance.",
        "decline": "I decline work on this offered parent revision. Others may continue under their own authority."}
    record: JsonObject = {"protocol_version": "0.1-draft", "type": "message" if choice == "message" else
        "artifact_revision" if choice == "revise" else "objection" if choice == "object" else "decline",
        "id": f"submission:arrival-{choice}", "world": WORLD, "from": NEW,
        "to": [NEW, discovery.GUIDE, RETURN], "body": {"text": texts[choice], "source_refs": references}}
    if choice == "revise":
        record.update({"artifact_id": "artifact:arrival-note", "media_type": "application/json",
            "derived_from": {key: parent[key] for key in ("from", "artifact_id", "revision")}})
        record["body"] = {**object_value(record["body"]), "work_status": "unfinished", "activity_offer": {
            "format": "agentciv-activity-offer/0.1-example", "title": "An unfinished arrival question",
            "purpose": "Choose whether to discuss source-preserving return views.",
            "offered_scope": "Optional conversation about permitted originals.",
            "limitations": "No assignment, resolved objection, or useful repair is claimed.",
            "copying_conditions": "Operator permits synthetic fixture export only.", "source_refs": references}}
    elif choice in ("object", "decline"):
        record.update({"target_from": parent["from"], "artifact_id": parent["artifact_id"], "revision": parent["revision"]})
    if choice == "message":
        endpoint, _ = wire.world_endpoints(origin, descriptor)
        status, raw = wire.exchange("POST", endpoint, token=token, body=json.dumps(record).encode("utf-8"))
        if status != 200:
            wire._fail(status, raw)
        receipt = client.decode(raw)
    else:
        if "collaboration.submit" not in mock.list_strings(descriptor.get("capabilities")):
            raise ValueError("collaboration unavailable")
        receipt = discovery.publish(origin, token, record)
    if token in json.dumps({"record": record, "receipt": receipt}, ensure_ascii=False):
        raise ValueError("credential reflected")
    return record, receipt


def publish_choice(origin: str, token: str, choice: str, basis: JsonObject) -> tuple[JsonObject, JsonObject] | None:
    try:
        return _publish_choice(origin, token, choice, basis)
    except (ValueError, OSError, KeyError, TypeError, client.DecisionError, wire.ParticipantError):
        raise ArrivalError("publication", "publication_failed") from None


def _verify_readback(record: JsonObject, receipt: JsonObject, originals: list[str]) -> str:
    """A receipt is insufficient: match the submitted fields to the retained original."""
    if (receipt.get("record_id") != record.get("id") or receipt.get("world") != WORLD
        or receipt.get("status") != "recorded" or not isinstance(receipt.get("event_id"), str)
        or type(receipt.get("sequence")) is not int):
        raise ValueError("arrival receipt invalid")
    matches = [(event, original) for event, original in zip(records(originals), originals, strict=True)
        if event.get("id") == receipt["event_id"]]
    if len(matches) != 1:
        raise ValueError("arrival publication missing")
    event, original = matches[0]
    record_type = str(record.get("type"))
    expected_kind = {"message": "message.recorded", "artifact_revision": "artifact.recorded",
        "objection": "objection.recorded", "decline": "decline.recorded"}.get(record_type)
    if (expected_kind is None or event.get("kind") != expected_kind or event.get("world") != WORLD
        or event.get("actor") != NEW or event.get("sequence") != receipt["sequence"]):
        raise ValueError("arrival publication identity failed")
    stored = object_value(object_value(event.get("body")).get(record_type))
    expected = copy.deepcopy(record)
    if record_type == "artifact_revision":
        if (receipt.get("artifact_id") != record.get("artifact_id") or type(receipt.get("revision")) is not int
            or receipt.get("revision") != 1):
            raise ValueError("arrival assigned revision invalid")
        expected["revision"] = 1
    if stored != expected:
        raise ValueError("arrival stored contribution changed")
    return original


def verify_readback(record: JsonObject, receipt: JsonObject, originals: list[str]) -> str:
    try:
        return _verify_readback(record, receipt, originals)
    except (ValueError, KeyError, TypeError, client.DecisionError):
        raise ArrivalError("readback", "invalid_stored_contribution") from None


def _find_returned(choice: str, originals: list[str]) -> str | None:
    """No event/artifact ID is supplied to the fresh return caller."""
    if choice not in CHOICES:
        raise ValueError("invalid arrival choice")
    found = [original for event, original in zip(records(originals), originals, strict=True) if event.get("actor") == NEW]
    if choice == "inspect":
        if found:
            raise ValueError("inspect published a contribution")
        return None
    if len(found) != 1:
        raise ValueError("return contribution missing")
    return found[0]


def find_returned(choice: str, originals: list[str]) -> str | None:
    try:
        return _find_returned(choice, originals)
    except (ValueError, KeyError, TypeError, client.DecisionError):
        raise ArrivalError("restart_and_return", "return_discovery_failed") from None


def run(output: Path, *, host: str = "python", choice: str = "inspect",
        reader_binary: Path | None = None, host_binary: Path | None = None) -> JsonObject:
    if host not in ("python", "rust") or choice not in CHOICES:
        raise ArrivalError("configuration", "invalid_configuration")
    try:
        output.mkdir(parents=True, exist_ok=False)
    except FileExistsError:
        raise ArrivalError("configuration", "output_exists") from None
    stage = "preparation"
    report: JsonObject = {"format": FORMAT, "outcome": "failed", "host": host, "choice": choice,
        "condition": "operator_scripted_arrival_not_model_behavior"}
    mock.save(output / "report.json", report)
    try:
        source = reader.source_identity()
        report["source"] = source
        binary = reader_binary or walk.build_rust_binaries("agentciv-reader")["agentciv-reader"]
        native_host = host_binary or (walk.rust_binary() if host == "rust" else None)
        hashes = {"reader": hashlib.sha256(binary.read_bytes()).hexdigest()}
        if host == "rust" and native_host is not None:
            hashes["host"] = hashlib.sha256(native_host.read_bytes()).hexdigest()
        report["binaries_sha256"] = hashes
        with tempfile.TemporaryDirectory(prefix="agentciv-arrival-private-") as temporary:
            config = Path(temporary) / "world.json"
            tokens = {name: secrets.token_urlsafe(24) for name in (discovery.GUIDE, discovery.SOURCE, NEW, discovery.HIDDEN, RETURN)}
            configure(config, tokens, choice)
            argv = walk.python_argv(config) if host == "python" else [str(native_host), "--config", str(config)]
            stage = "initial_history"
            running = walk.start_host(argv)
            try:
                origin = walk.wait_until_ready(running)
                discovery.seed(origin, tokens, extra_recipient=RETURN)
                initial, initial_originals = capture(binary, origin, tokens[NEW], output, "initial")
                report["initial"] = initial
                basis = inspect_basis(initial, initial_originals)
                report["basis"] = basis
                stage = "source_revalidation"
                before, before_originals = capture(binary, origin, tokens[NEW], output, "pre-submit")
                if inspect_basis(before, before_originals) != basis:
                    raise ValueError("arrival basis changed")
                stage = "publication"
                action: JsonObject = {"choice": choice, "principal": NEW, "choice_source": "operator_scripted",
                    "publication_state": "not_submitted" if choice == "inspect" else "attempt_pending_outcome_unknown"}
                report["participant_action"] = action
                mock.save(output / "report.json", report)
                result = publish_choice(origin, tokens[NEW], choice, basis)
                if result is not None:
                    record, receipt = result
                    action.update({"record": record, "receipt": receipt, "publication_state": "receipt_received_readback_pending"})
                else:
                    action["publication_state"] = "not_submitted"
                report["participant_action"] = action
                mock.save(output / "report.json", report)
                stage = "readback"
                after, after_originals = capture(binary, origin, tokens[NEW], output, "after-submit")
                report["after_submit"] = after
                published_original: str | None = None
                if result is not None:
                    published_original = verify_readback(record, receipt, after_originals)
                    action["event_record_utf8"] = published_original
                    action["publication_state"] = "readback_verified"
                elif after_originals != before_originals:
                    raise ValueError("inspect changed history")
                # This ensures no unrelated loss or change was hidden by the successful receipt.
                if after_originals[:len(before_originals)] != before_originals or len(after_originals) != len(before_originals) + (result is not None):
                    raise ValueError("arrival prior history changed")
            finally:
                walk.stop_host(running)
            stage = "restart_and_return"
            # Same database and visibility; a distinct caller receives its own new read-only credential.
            tokens[RETURN] = secrets.token_urlsafe(24)
            configure(config, tokens, choice)
            running = walk.start_host(argv)
            try:
                origin = walk.wait_until_ready(running)
                returned, returned_originals = capture(binary, origin, tokens[RETURN], output, "return")
                report["return"] = returned
                found = find_returned(choice, returned_originals)
                if found != published_original:
                    raise ValueError("return original changed")
                if inspect_basis(returned, returned_originals) != basis:
                    raise ValueError("return basis changed")
                if returned_originals != after_originals:
                    raise ValueError("return history changed")
            finally:
                walk.stop_host(running)
        stage = "source_identity"
        if reader.source_identity() != source:
            raise ValueError("arrival source changed")
        report.update({"outcome": "passed", "source_unchanged": True,
            "participant_report": {"published": published_original is not None, "readback_verified": result is not None,
                "reported_action": choice, "submitted_account": object_value(record["body"]) if result is not None else None,
                "account_source": "operator_scripted_submission_or_no_account_for_inspection",
                "scope": "submitted_bytes_not_belief_or_delivery"},
            "operator_interventions": ["operator_seeded_sources_parent_objection_offers_and_100_padding_messages",
                "operator_selected_scripted_choice_and_provisioned_explicit_grants",
                "operator_restarted_same_world_and_rotated_distinct_return_caller_read_credential"],
            "later_interpretation": {"fresh_caller_found": found is not None, "exact_original_retained": found == published_original,
                "objection_retained": True, "meaningful_repair": False,
                "scope": "bounded_stored_contribution_not_autonomous_choice_shared_understanding_or_interoperability"},
            "limits": {"retrieval": discovery.LIMITS, "freshness": "full_revalidation_not_atomic_or_continuing_authorization",
                "copying_condition": "operator_authorized_synthetic_fixture_export_only",
                "world_lifetime": "temporary_fixture_removed_after_run_no_ongoing_world",
                "external_spend_usd": 0, "runtime_dispatch": "none"}})
    except (ValueError, OSError, KeyError, TypeError, client.DecisionError, wire.ParticipantError, walk.WalkFailure, subprocess.SubprocessError) as error:
        failure_stage = error.stage if isinstance(error, ArrivalError) else stage
        failure_code = error.code if isinstance(error, ArrivalError) else "arrival_stage_failed"
        report.update({"outcome": "failed", "failure_stage": failure_stage, "failure_code": failure_code})
        mock.save(output / "report.json", report)
        raise ArrivalError(failure_stage, failure_code) from None
    mock.save(output / "report.json", report)
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--host", choices=("python", "rust"), default="python")
    parser.add_argument("--choice", choices=CHOICES, default="inspect")
    args = parser.parse_args()
    try:
        report = run(args.output, host=args.host, choice=args.choice)
    except ArrivalError as error:
        print(json.dumps({"outcome": "failed", "stage": error.stage, "code": error.code}))
        return 1
    except (ValueError, OSError, KeyError, client.DecisionError, wire.ParticipantError, walk.WalkFailure, subprocess.SubprocessError):
        print(json.dumps({"outcome": "failed", "stage": "preparation", "code": "arrival_stage_failed"}))
        return 1
    print(json.dumps({"outcome": report["outcome"], "host": args.host, "choice": args.choice}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
