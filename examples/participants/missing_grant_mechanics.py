"""Synthetic mechanics packet for a local room with a missing write grant.

Exercises the existing local room wrapper, bounded reader, and both loopback
hosts without running any model calls. A read-only room and a separately
initialized matched writer room are tested independently (never a live
same-principal grant upgrade).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import socket
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

import collaboration as civic
import encounter_client as client
import encounter_private as private
import local_encounter as operator
import reading_table_journey as journey

JsonObject = dict[str, object]
FIXTURE_FORMAT = "agentciv-missing-grant-mechanics/0.1-example"
COPYING_CONDITION = "operator_authorized_synthetic_fixture_export_only"
PRIVATE_NOTE_FORMAT = "agentciv-missing-grant-private-note/0.1-example"
MAX_FIXTURE_BYTES = 65536
DEFAULT_FIXTURE = Path(__file__).resolve().parent / "fixtures" / "missing-grant-room.json"
ROOT = Path(__file__).resolve().parents[2]
PYTHON_HOST = ROOT / "implementations" / "http-commons-python" / "host.py"

ACCESS_DENIAL_CODES = frozenset({"forbidden", "authentication_required"})
TRANSPORT_FAILURE_CODES = frozenset({
    "host_unreachable",
    "host_read_timeout",
    "reader_process_failed",
    "reader_failed",
})


class MechanicsError(ValueError):
    """Fixed code only. Never carries paths, tokens, or record bodies."""

    def __init__(self, code: str) -> None:
        super().__init__(code)
        self.code = code


@dataclass(frozen=True)
class OpenItem:
    item_id: str
    text: str


@dataclass(frozen=True)
class CollaborationNoteSpec:
    submission_id: str
    artifact_id: str
    title: str
    purpose: str
    offered_scope: str
    limitations: str
    copying_conditions: str
    summary: str
    open_items: tuple[OpenItem, ...]


@dataclass(frozen=True)
class PeerInvitationSpec:
    submission_id: str
    source_kind: str
    operator_grant: bool
    text: str


@dataclass(frozen=True)
class MissingGrantFixture:
    world: str
    retention_seconds: int
    author: str
    observer: str
    visitor: str
    note: CollaborationNoteSpec
    invitation: PeerInvitationSpec
    private_draft_title: str
    private_draft_text: str
    private_decline_text: str
    followup_submission_id: str
    followup_text: str
    fixture_sha256: str


@dataclass(frozen=True)
class EventDigest:
    event_id: str
    sequence: int
    kind: str
    actor: str
    sha256: str


def _identifier(value: object, maximum: int = 256) -> str:
    if not isinstance(value, str) or not 1 <= len(value.encode("utf-8")) <= maximum:
        raise MechanicsError("invalid_fixture_identifier")
    if any(ord(char) < 32 or ord(char) == 127 for char in value):
        raise MechanicsError("invalid_fixture_identifier")
    return value


def _prose(value: object, maximum: int = 1024) -> str:
    if not isinstance(value, str) or not 1 <= len(value.encode("utf-8")) <= maximum:
        raise MechanicsError("invalid_fixture_prose")
    if any(ord(char) < 32 or ord(char) == 127 for char in value):
        raise MechanicsError("invalid_fixture_prose")
    return value


def _object(value: object) -> JsonObject:
    if not isinstance(value, dict):
        raise MechanicsError("invalid_fixture_shape")
    return {str(key): item for key, item in value.items()}


def _exact_object(value: object, keys: set[str]) -> JsonObject:
    item = _object(value)
    if set(item) != keys:
        raise MechanicsError("invalid_fixture_shape")
    return item


def _whole(value: object, minimum: int, maximum: int) -> int:
    if type(value) is not int or not minimum <= value <= maximum:
        raise MechanicsError("invalid_fixture_number")
    return value


def load_fixture(path: Path) -> MissingGrantFixture:
    """Load and strictly validate the frozen synthetic missing-grant fixture."""
    try:
        if path.is_symlink() or not path.is_file():
            raise MechanicsError("fixture_unavailable")
        with path.open("rb") as stream:
            raw = stream.read(MAX_FIXTURE_BYTES + 1)
    except OSError:
        raise MechanicsError("fixture_unavailable") from None
    if len(raw) > MAX_FIXTURE_BYTES:
        raise MechanicsError("fixture_too_large")
    try:
        data = civic.decode(raw)
    except civic.DecisionError:
        raise MechanicsError("invalid_fixture_json") from None
    top = _exact_object(data, {
        "format",
        "copying_condition",
        "world",
        "retention_seconds",
        "principals",
        "collaboration_note",
        "peer_invitation",
        "private_draft",
        "private_decline",
        "visitor_followup",
    })
    if top["format"] != FIXTURE_FORMAT or top["copying_condition"] != COPYING_CONDITION:
        raise MechanicsError("invalid_fixture_format")
    world = _identifier(top["world"])
    retention_seconds = _whole(top["retention_seconds"], 1, 31536000)
    principals = _exact_object(top["principals"], {"author", "observer", "visitor"})
    author = _identifier(principals["author"])
    observer = _identifier(principals["observer"])
    visitor = _identifier(principals["visitor"])
    if len({author, observer, visitor}) != 3:
        raise MechanicsError("duplicate_fixture_principals")

    note_raw = _exact_object(top["collaboration_note"], {
        "submission_id",
        "artifact_id",
        "title",
        "purpose",
        "offered_scope",
        "limitations",
        "copying_conditions",
        "summary",
        "open_items",
    })
    items_raw = note_raw["open_items"]
    if not isinstance(items_raw, list) or not 1 <= len(items_raw) <= 16:
        raise MechanicsError("invalid_fixture_shape")
    items: list[OpenItem] = []
    seen_items: set[str] = set()
    for entry in items_raw:
        row = _exact_object(entry, {"id", "text"})
        item_id = _identifier(row["id"])
        if item_id in seen_items:
            raise MechanicsError("duplicate_fixture_item")
        seen_items.add(item_id)
        items.append(OpenItem(item_id=item_id, text=_prose(row["text"])))
    note = CollaborationNoteSpec(
        submission_id=_identifier(note_raw["submission_id"]),
        artifact_id=_identifier(note_raw["artifact_id"]),
        title=_prose(note_raw["title"]),
        purpose=_prose(note_raw["purpose"]),
        offered_scope=_prose(note_raw["offered_scope"]),
        limitations=_prose(note_raw["limitations"]),
        copying_conditions=_prose(note_raw["copying_conditions"]),
        summary=_prose(note_raw["summary"]),
        open_items=tuple(items),
    )

    invite_raw = _exact_object(top["peer_invitation"], {
        "submission_id",
        "source_kind",
        "operator_grant",
        "text",
    })
    if invite_raw["source_kind"] != "synthetic_fixture_content" or invite_raw["operator_grant"] is not False:
        raise MechanicsError("invitation_must_not_claim_operator_grant")
    invitation = PeerInvitationSpec(
        submission_id=_identifier(invite_raw["submission_id"]),
        source_kind="synthetic_fixture_content",
        operator_grant=False,
        text=_prose(invite_raw["text"]),
    )

    draft_raw = _exact_object(top["private_draft"], {"title", "text"})
    decline_raw = _exact_object(top["private_decline"], {"text"})
    followup_raw = _exact_object(top["visitor_followup"], {"submission_id", "text"})
    return MissingGrantFixture(
        world=world,
        retention_seconds=retention_seconds,
        author=author,
        observer=observer,
        visitor=visitor,
        note=note,
        invitation=invitation,
        private_draft_title=_prose(draft_raw["title"]),
        private_draft_text=_prose(draft_raw["text"]),
        private_decline_text=_prose(decline_raw["text"]),
        followup_submission_id=_identifier(followup_raw["submission_id"]),
        followup_text=_prose(followup_raw["text"]),
        fixture_sha256=hashlib.sha256(raw).hexdigest(),
    )


def collaboration_note_record(fixture: MissingGrantFixture) -> JsonObject:
    """Build the synthetic collaboration note artifact revision."""
    return {
        "protocol_version": "0.1-draft",
        "type": "artifact_revision",
        "id": fixture.note.submission_id,
        "world": fixture.world,
        "from": fixture.author,
        "to": [fixture.observer, fixture.visitor],
        "artifact_id": fixture.note.artifact_id,
        "media_type": "application/json",
        "body": {
            "activity_offer": {
                "format": "agentciv-activity-offer/0.1-example",
                "title": fixture.note.title,
                "purpose": fixture.note.purpose,
                "offered_scope": fixture.note.offered_scope,
                "limitations": fixture.note.limitations,
                "copying_conditions": fixture.note.copying_conditions,
                "source_refs": [],
            },
            "collaboration_note": {
                "format": "agentciv-missing-grant-note/0.1-example",
                "copying_condition": COPYING_CONDITION,
                "summary": fixture.note.summary,
                "open_items": [
                    {"id": item.item_id, "text": item.text}
                    for item in fixture.note.open_items
                ],
            },
        },
    }


def peer_invitation_record(fixture: MissingGrantFixture, note: journey.Observed) -> JsonObject:
    """Build the synthetic peer invitation citing the recorded note event."""
    if note.kind != "artifact.recorded" or note.artifact_id is None or note.revision is None:
        raise MechanicsError("note_not_an_artifact")
    return {
        "protocol_version": "0.1-draft",
        "type": "message",
        "id": fixture.invitation.submission_id,
        "world": fixture.world,
        "from": fixture.author,
        "to": [fixture.observer, fixture.visitor],
        "body": {
            "text": fixture.invitation.text,
            "peer_invitation": {
                "source_kind": fixture.invitation.source_kind,
                "operator_grant": fixture.invitation.operator_grant,
                "note_selector": note.selector(),
            },
        },
    }


def visitor_followup_record(fixture: MissingGrantFixture, note: journey.Observed) -> JsonObject:
    """Build the normal follow-up message submitted by the visitor principal."""
    if note.kind != "artifact.recorded" or note.artifact_id is None or note.revision is None:
        raise MechanicsError("note_not_an_artifact")
    return {
        "protocol_version": "0.1-draft",
        "type": "message",
        "id": fixture.followup_submission_id,
        "world": fixture.world,
        "from": fixture.visitor,
        "to": [fixture.author, fixture.observer],
        "body": {
            "text": fixture.followup_text,
            "referenced_note": note.selector(),
        },
    }


def save_private_note(
    path: Path,
    *,
    kind: str,
    world: str,
    principal: str,
    source_event_id: str,
    source_sha256: str,
    text: str,
    title: str | None = None,
) -> Path:
    """Store a private local draft or local decline outside shared room state."""
    if kind not in ("local_draft", "local_decline"):
        raise MechanicsError("invalid_private_note_kind")
    payload: JsonObject = {
        "format": PRIVATE_NOTE_FORMAT,
        "kind": kind,
        "world": _identifier(world),
        "principal": _identifier(principal),
        "source_event_id": _identifier(source_event_id),
        "source_sha256": _identifier(source_sha256),
        "text": _prose(text),
        "shared_history_mutated": False,
        "copying_permission": "not_granted",
    }
    if title is not None:
        payload["title"] = _prose(title)
    raw = json.dumps(payload, ensure_ascii=True, indent=2, allow_nan=False).encode("ascii") + b"\n"
    private.write_new(path, raw)
    return path


def classify_failure(error: private.EncounterError) -> str:
    """Separate access denial from transport/read failure; never treat either as empty history."""
    if error.code in ACCESS_DENIAL_CODES:
        return "access_denied"
    if error.code in TRANSPORT_FAILURE_CODES:
        return "transport_or_read_failure"
    return "other_failure"


def _free_port() -> int:
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def _host_argv(host: str, host_config: Path, host_binary: Path | None) -> list[str]:
    if host == "python":
        return [sys.executable, str(PYTHON_HOST), "--config", str(host_config)]
    if host == "rust":
        found = host_binary if host_binary is not None else private.find_tool("agentciv-host")
        if found is None or not Path(found).is_file():
            raise MechanicsError("host_binary_unavailable")
        return [str(Path(found).resolve()), "--config", str(host_config)]
    raise MechanicsError("unsupported_host")


def _view_digests(view_path: Path) -> list[EventDigest]:
    raw_view = private.read_bounded(view_path, client.MAX_OUTPUT)
    decoded = client._decode(raw_view)
    originals = client._originals(decoded)
    digests: list[EventDigest] = []
    for original in originals:
        observed = journey.observe_original(original)
        digests.append(EventDigest(
            event_id=observed.event_id,
            sequence=observed.sequence,
            kind=observed.kind,
            actor=observed.author,
            sha256=observed.sha256,
        ))
    return digests


def _digest_rows(digests: list[EventDigest]) -> list[JsonObject]:
    return [
        {
            "event_id": item.event_id,
            "sequence": item.sequence,
            "kind": item.kind,
            "actor": item.actor,
            "sha256": item.sha256,
        }
        for item in digests
    ]


def _client_configs(initialized: JsonObject) -> dict[str, Path]:
    rows = initialized.get("clients")
    if not isinstance(rows, list):
        raise MechanicsError("invalid_room_initialization")
    configs: dict[str, Path] = {}
    for row in rows:
        item = _object(row)
        configs[str(item["principal"])] = Path(str(item["config"]))
    return configs


def _tokens(configs: dict[str, Path]) -> list[str]:
    return [str(client._configuration(path)["token"]) for path in configs.values()]


def _assert_no_tokens(value: object, tokens: list[str]) -> None:
    for token in tokens:
        if client._contains_secret(value, token):
            raise MechanicsError("credential_reflected")


def _publish_record(
    config_path: Path,
    record: JsonObject,
    record_path: Path,
    world: str,
    reader_binary: Path,
) -> tuple[JsonObject, journey.Observed]:
    raw = journey.record_bytes(record)
    record_path.write_bytes(raw)
    basis = client.read(config_path, reader_binary=reader_binary)
    result = client.submit(config_path, record_path, Path(str(basis["view_path"])), reader_binary=reader_binary)
    if result.get("outcome") != "recorded":
        raise MechanicsError("seed_publish_failed")
    observed = journey.observe_journal(Path(str(result["journal_path"])), world)
    return result, observed


def _seed_room(
    fixture: MissingGrantFixture,
    author_config: Path,
    staging_dir: Path,
    reader_binary: Path,
) -> tuple[journey.Observed, journey.Observed]:
    _, note_observed = _publish_record(
        author_config,
        collaboration_note_record(fixture),
        staging_dir / "collaboration-note.json",
        fixture.world,
        reader_binary,
    )
    _, invitation_observed = _publish_record(
        author_config,
        peer_invitation_record(fixture, note_observed),
        staging_dir / "peer-invitation.json",
        fixture.world,
        reader_binary,
    )
    return note_observed, invitation_observed


def run_read_only_room(
    fixture: MissingGrantFixture,
    root: Path,
    host: str,
    *,
    reader_binary: Path,
    host_binary: Path | None = None,
    port: int | None = None,
) -> JsonObject:
    """Exercise restart, return, private draft/decline, 403 refusal, and network failure."""
    room_state = root / "readonly-room"
    staging = root / "readonly-staging"
    private.create_directory(staging)
    chosen_port = port if port is not None else _free_port()
    initialized = operator.initialize(
        room_state,
        fixture.world,
        [fixture.author],
        [fixture.observer, fixture.visitor],
        port=chosen_port,
        retention_seconds=fixture.retention_seconds,
    )
    configs = _client_configs(initialized)
    tokens = _tokens(configs)
    origin = str(initialized["origin"])
    argv = _host_argv(host, room_state / "host.json", host_binary)

    first_run = operator._start(argv, origin)
    try:
        operator._wait_ready(first_run)
        note_observed, invitation_observed = _seed_room(
            fixture, configs[fixture.author], staging, reader_binary
        )
        observer_initial = client.read(configs[fixture.observer], reader_binary=reader_binary)
        baseline_digests = _view_digests(Path(str(observer_initial["view_path"])))
        if len(baseline_digests) != 2:
            raise MechanicsError("unexpected_baseline_event_count")

        visitor_before = client.read(configs[fixture.visitor], reader_binary=reader_binary)
        visitor_before_path = Path(str(visitor_before["view_path"]))
        visitor_before_digests = _view_digests(visitor_before_path)
        if visitor_before_digests != baseline_digests:
            raise MechanicsError("visitor_baseline_mismatch")
        offers_before = client.offers(
            configs[fixture.visitor], visitor_before_path, query="", reader_binary=reader_binary
        )
        offers_rows = offers_before.get("offers")
        if not isinstance(offers_rows, list) or len(offers_rows) != 1:
            raise MechanicsError("expected_one_offer_before_restart")
    finally:
        operator._stop_owned(first_run)

    # Restart the host and return as the original read-only caller.
    restarted = operator._start(argv, origin)
    try:
        operator._wait_ready(restarted)
        visitor_return = client.read(configs[fixture.visitor], reader_binary=reader_binary)
        visitor_return_path = Path(str(visitor_return["view_path"]))
        visitor_return_digests = _view_digests(visitor_return_path)
        if visitor_return_digests != baseline_digests:
            raise MechanicsError("visitor_return_mismatch")
        rendered_source = client.show(
            configs[fixture.visitor], visitor_return_path, event_id=note_observed.event_id
        )
        if fixture.note.summary not in rendered_source:
            raise MechanicsError("source_not_found_after_restart")

        # Keep a local draft and a local decline privately without mutating shared history.
        draft_path = save_private_note(
            staging / "private-draft.json",
            kind="local_draft",
            world=fixture.world,
            principal=fixture.visitor,
            source_event_id=note_observed.event_id,
            source_sha256=note_observed.sha256,
            title=fixture.private_draft_title,
            text=fixture.private_draft_text,
        )
        decline_path = save_private_note(
            staging / "private-decline.json",
            kind="local_decline",
            world=fixture.world,
            principal=fixture.visitor,
            source_event_id=note_observed.event_id,
            source_sha256=note_observed.sha256,
            text=fixture.private_decline_text,
        )
        observer_after_private = client.read(configs[fixture.observer], reader_binary=reader_binary)
        if _view_digests(Path(str(observer_after_private["view_path"]))) != baseline_digests:
            raise MechanicsError("private_note_mutated_shared_history")

        # Independently exercise one normal submit attempt by the read-only caller.
        followup_path = staging / "visitor-followup.json"
        followup_path.write_bytes(journey.record_bytes(visitor_followup_record(fixture, note_observed)))
        submit_result = client.submit(
            configs[fixture.visitor],
            followup_path,
            visitor_return_path,
            reader_binary=reader_binary,
        )
        if submit_result.get("outcome") != "rejected" or submit_result.get("code") != "forbidden":
            raise MechanicsError("expected_forbidden_rejection")
        journal_data = client._decode(
            private.read_bounded(Path(str(submit_result["journal_path"])), client.MAX_OUTPUT)
        )
        if journal_data.get("state") != "rejected" or journal_data.get("failure_code") != "forbidden":
            raise MechanicsError("unexpected_rejection_journal")

        # Separate permitted observer verifies shared event sequence is completely unchanged.
        observer_after_refusal = client.read(configs[fixture.observer], reader_binary=reader_binary)
        after_refusal_digests = _view_digests(Path(str(observer_after_refusal["view_path"])))
        if after_refusal_digests != baseline_digests:
            raise MechanicsError("shared_history_changed_after_refusal")

        # Check access denial on read (invalid token while host is running) separately from transport failure.
        bad_state = root / "bad-token-state"
        private.create_directory(bad_state)
        bad_cache = bad_state / "cache"
        private.create_directory(bad_cache)
        bad_config_path = bad_state / "client.json"
        visitor_settings = client._configuration(configs[fixture.visitor])
        bad_settings: JsonObject = dict(visitor_settings)
        bad_settings["token"] = "invalid-token-0123456789abcdef0123456789"
        bad_settings["cache_directory"] = str(bad_cache)
        private.write_new(
            bad_config_path,
            (json.dumps(bad_settings, ensure_ascii=False, indent=2) + "\n").encode("utf-8"),
        )
        access_denial_code = ""
        access_denial_class = ""
        try:
            client.read(bad_config_path, reader_binary=reader_binary)
        except private.EncounterError as error:
            access_denial_code = error.code
            access_denial_class = classify_failure(error)
        if access_denial_class != "access_denied" or list(bad_cache.glob("view-*.json")):
            raise MechanicsError("expected_read_access_denial")
    finally:
        operator._stop_owned(restarted)

    # With the host stopped, test network/read failure separately from access denial.
    visitor_cache = Path(str(client._configuration(configs[fixture.visitor])["cache_directory"]))
    views_before_stopped_read = set(visitor_cache.glob("view-*.json"))
    network_failure_code = ""
    network_failure_class = ""
    try:
        client.read(configs[fixture.visitor], reader_binary=reader_binary)
    except private.EncounterError as error:
        network_failure_code = error.code
        network_failure_class = classify_failure(error)
    views_after_stopped_read = set(visitor_cache.glob("view-*.json"))
    if (
        network_failure_code != "host_unreachable"
        or network_failure_class != "transport_or_read_failure"
        or views_after_stopped_read != views_before_stopped_read
    ):
        raise MechanicsError("expected_network_read_failure")

    summary: JsonObject = {
        "host": host,
        "room_kind": "read_only_visitor",
        "world": fixture.world,
        "seeded_note_event_id": note_observed.event_id,
        "seeded_note_sha256": note_observed.sha256,
        "seeded_invitation_event_id": invitation_observed.event_id,
        "restarted_and_returned": True,
        "exact_source_preserved": True,
        "private_draft_saved": draft_path.is_file(),
        "private_decline_saved": decline_path.is_file(),
        "read_only_submit_outcome": submit_result["outcome"],
        "read_only_submit_code": submit_result["code"],
        "observer_events_before": _digest_rows(baseline_digests),
        "observer_events_after_refusal": _digest_rows(after_refusal_digests),
        "observer_history_unchanged": after_refusal_digests == baseline_digests,
        "read_access_denial_code": access_denial_code,
        "read_access_denial_class": access_denial_class,
        "network_failure_code": network_failure_code,
        "network_failure_class": network_failure_class,
        "failure_distinct_from_empty_history": True,
    }
    _assert_no_tokens(summary, tokens)
    return summary


def run_matched_writer_room(
    fixture: MissingGrantFixture,
    root: Path,
    host: str,
    *,
    reader_binary: Path,
    host_binary: Path | None = None,
    port: int | None = None,
) -> JsonObject:
    """Exercise the separately initialized matched writer room and exact idempotent replay."""
    room_state = root / "writer-room"
    staging = root / "writer-staging"
    private.create_directory(staging)
    chosen_port = port if port is not None else _free_port()
    initialized = operator.initialize(
        room_state,
        fixture.world,
        [fixture.author, fixture.visitor],
        [fixture.observer],
        port=chosen_port,
        retention_seconds=fixture.retention_seconds,
    )
    configs = _client_configs(initialized)
    tokens = _tokens(configs)
    origin = str(initialized["origin"])
    argv = _host_argv(host, room_state / "host.json", host_binary)

    owned = operator._start(argv, origin)
    try:
        operator._wait_ready(owned)
        note_observed, invitation_observed = _seed_room(
            fixture, configs[fixture.author], staging, reader_binary
        )
        observer_baseline = client.read(configs[fixture.observer], reader_binary=reader_binary)
        baseline_digests = _view_digests(Path(str(observer_baseline["view_path"])))
        if len(baseline_digests) != 2:
            raise MechanicsError("unexpected_writer_baseline_count")

        followup_path = staging / "visitor-followup.json"
        followup_path.write_bytes(journey.record_bytes(visitor_followup_record(fixture, note_observed)))
        visitor_view = client.read(configs[fixture.visitor], reader_binary=reader_binary)
        first_submit = client.submit(
            configs[fixture.visitor],
            followup_path,
            Path(str(visitor_view["view_path"])),
            reader_binary=reader_binary,
        )
        if first_submit.get("outcome") != "recorded":
            raise MechanicsError("matched_writer_submit_failed")

        observer_after_first = client.read(configs[fixture.observer], reader_binary=reader_binary)
        after_first_digests = _view_digests(Path(str(observer_after_first["view_path"])))
        if len(after_first_digests) != 3:
            raise MechanicsError("expected_single_appended_event")
        appended = after_first_digests[-1]
        if (
            appended.event_id != first_submit["event_id"]
            or appended.sequence != first_submit["sequence"]
            or appended.actor != fixture.visitor
            or appended.kind != "message.recorded"
        ):
            raise MechanicsError("appended_event_attribution_mismatch")

        # Exact replay within the advertised idempotency window adds zero additional events.
        visitor_before_replay = client.read(configs[fixture.visitor], reader_binary=reader_binary)
        replay_submit = client.submit(
            configs[fixture.visitor],
            followup_path,
            Path(str(visitor_before_replay["view_path"])),
            reader_binary=reader_binary,
        )
        if (
            replay_submit.get("outcome") != "recorded"
            or replay_submit.get("event_id") != first_submit["event_id"]
            or replay_submit.get("sequence") != first_submit["sequence"]
        ):
            raise MechanicsError("exact_replay_receipt_mismatch")

        observer_after_replay = client.read(configs[fixture.observer], reader_binary=reader_binary)
        after_replay_digests = _view_digests(Path(str(observer_after_replay["view_path"])))
        if after_replay_digests != after_first_digests:
            raise MechanicsError("exact_replay_added_events")
    finally:
        operator._stop_owned(owned)

    summary: JsonObject = {
        "host": host,
        "room_kind": "separately_initialized_matched_writer",
        "live_grant_upgrade": False,
        "world": fixture.world,
        "seeded_note_event_id": note_observed.event_id,
        "seeded_invitation_event_id": invitation_observed.event_id,
        "first_submit_outcome": first_submit["outcome"],
        "first_submit_event_id": first_submit["event_id"],
        "first_submit_sequence": first_submit["sequence"],
        "first_submit_actor": appended.actor,
        "replay_submit_outcome": replay_submit["outcome"],
        "replay_same_event_id": replay_submit["event_id"] == first_submit["event_id"],
        "replay_same_sequence": replay_submit["sequence"] == first_submit["sequence"],
        "observer_events_before": len(baseline_digests),
        "observer_events_after_submit": len(after_first_digests),
        "observer_events_after_replay": len(after_replay_digests),
        "replay_added_events": len(after_replay_digests) - len(after_first_digests),
    }
    _assert_no_tokens(summary, tokens)
    return summary


def run_packet(
    fixture_path: Path,
    hosts: tuple[str, ...] = ("rust", "python"),
    *,
    reader_binary: Path | None = None,
    host_binary: Path | None = None,
) -> JsonObject:
    """Run the full Step 1 synthetic mechanics packet on the selected host(s)."""
    fixture = load_fixture(fixture_path)
    resolved_reader = client._binary(reader_binary)
    results: list[JsonObject] = []
    for selected_host in hosts:
        if selected_host not in ("rust", "python"):
            raise MechanicsError("unsupported_host")
        with tempfile.TemporaryDirectory(prefix=f"agentciv-missing-grant-{selected_host}-") as temporary:
            root = Path(temporary).resolve()
            read_only = run_read_only_room(
                fixture,
                root,
                selected_host,
                reader_binary=resolved_reader,
                host_binary=host_binary if selected_host == "rust" else None,
            )
            matched_writer = run_matched_writer_room(
                fixture,
                root,
                selected_host,
                reader_binary=resolved_reader,
                host_binary=host_binary if selected_host == "rust" else None,
            )
            results.append({
                "host": selected_host,
                "read_only_room": read_only,
                "matched_writer_room": matched_writer,
            })
    return {
        "outcome": "passed",
        "format": FIXTURE_FORMAT,
        "fixture_sha256": fixture.fixture_sha256,
        "hosts": list(hosts),
        "results": results,
        "evidence_scope": "synthetic_local_mechanics_only",
        "limits": {
            "model_calls": 0,
            "live_grant_upgrade": False,
            "interoperability_claim": False,
            "new_conformance_profile": False,
        },
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", type=Path, default=DEFAULT_FIXTURE)
    parser.add_argument("--host", choices=("rust", "python", "both"), default="both")
    parser.add_argument("--reader-binary", type=Path, default=None)
    parser.add_argument("--host-binary", type=Path, default=None)
    args = parser.parse_args(argv)
    hosts: tuple[str, ...] = ("rust", "python") if args.host == "both" else (args.host,)
    try:
        report = run_packet(
            args.fixture,
            hosts,
            reader_binary=args.reader_binary,
            host_binary=args.host_binary,
        )
    except (MechanicsError, private.EncounterError, journey.JourneyError) as error:
        print(json.dumps({"outcome": "failed", "code": error.code}, ensure_ascii=True))
        return 1
    print(json.dumps(report, ensure_ascii=True, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
