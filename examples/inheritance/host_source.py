"""Scripted raw HTTP fixture origins, followed by a fresh authorized reader.

Submissions are operator fixtures, not model choices. Only permitted event data
is returned. Private configuration, credentials and databases are disposable.
"""

from __future__ import annotations

import copy
import json
import secrets
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "examples" / "http-commons"))
sys.path.insert(0, str(ROOT / "examples" / "participants"))

import collaboration as client  # noqa: E402
import local_participant as wire  # noqa: E402
import walk  # noqa: E402
import oracle  # noqa: E402

JsonObject = dict[str, object]
AUTHORS = ("agent:a", "agent:b", "agent:d")
OBSERVER = "agent:fixture-observer"
NEWCOMER = "agent:newcomer"
WORLD = "civ:inheritance-fixture"


class HostSourceError(RuntimeError):
    """A fixed diagnostic code; never a copied host response or private log."""


def reflected(value: object, tokens: list[str]) -> bool:
    encoded = json.dumps(value, ensure_ascii=False)
    return any(token in encoded for token in tokens)


def endpoint(origin: str) -> str:
    discovery = wire.discover(origin)
    capabilities = discovery.get("capabilities")
    collaborate = client.object_value(discovery.get("endpoints")).get("collaborate")
    if (discovery.get("id") != WORLD or not isinstance(capabilities, list)
        or "collaboration.submit" not in capabilities or not isinstance(collaborate, str)
        or not wire._same_origin(origin, collaborate)):
        raise HostSourceError("invalid_collaboration_discovery")
    return collaborate


def seed_record(event: JsonObject) -> JsonObject:
    kind = event["kind"]
    key = {"artifact.recorded": "artifact_revision", "objection.recorded": "objection",
           "decline.recorded": "decline"}.get(str(kind))
    if key is None:
        raise HostSourceError("unsupported_seed_kind")
    record = copy.deepcopy(client.object_value(client.object_value(event["body"]).get(key)))
    if record.get("world") != WORLD or record.get("from") not in AUTHORS:
        raise HostSourceError("invalid_seed_identity")
    if key == "artifact_revision":
        # Stored fixture revisions are expectations. Only the host assigns them.
        record.pop("revision", None)
    return record


def submit_seed(origin: str, event: JsonObject, token: str) -> JsonObject:
    record = seed_record(event)
    status, payload = wire.exchange("POST", endpoint(origin), token=token,
        body=json.dumps(record, ensure_ascii=False, separators=(",", ":")).encode("utf-8"))
    if status != 200:
        raise HostSourceError("seed_publication_rejected")
    receipt = client.decode(payload)
    if reflected(receipt, [token]):
        raise HostSourceError("credential_reflected")
    if (receipt.get("status") != "recorded" or receipt.get("record_id") != record["id"]
        or receipt.get("world") != WORLD or not isinstance(receipt.get("event_id"), str)
        or type(receipt.get("sequence")) is not int):
        raise HostSourceError("invalid_seed_receipt")
    events = client.history(origin, token)
    matching = [found for found in events if found["id"] == receipt["event_id"]]
    if len(matching) != 1:
        raise HostSourceError("seed_not_read_back")
    found = matching[0]
    expected = copy.deepcopy(record)
    if record["type"] == "artifact_revision":
        fixture = client.object_value(client.object_value(event["body"])["artifact_revision"])
        expected["revision"] = fixture["revision"]
    if (found.get("world") != WORLD or found.get("sequence") != receipt["sequence"]
        or found.get("actor") != record["from"] or found.get("kind") != event["kind"]
        or found.get("body") != {str(record["type"]): expected}):
        raise HostSourceError("seed_readback_mismatch")
    return found


def configuration(directory: Path, credentials: list[JsonObject]) -> Path:
    value: JsonObject = {
        "world_id": WORLD, "title": "Scripted inheritance source fixture",
        "database_path": str(directory / "world.sqlite"), "listen": "127.0.0.1:0",
        "visibility": "members", "retention_seconds": 86400, "max_payload_bytes": 16384,
        "credentials": credentials,
    }
    path = directory / "host.json"
    path.write_text(json.dumps(value), encoding="utf-8")
    return path


def credential(principal: str, token: str, *, write: bool) -> JsonObject:
    return {"principal": principal, "token": token, "read": True, "write": write}


def create_history(directory: Path) -> list[JsonObject]:
    """Record fixed seeds, restart, rotate grants, and return the new reader's history.

    The caller owns an existing disposable directory outside this checkout.
    Temporary child state is deleted on success and failure. This is live-host
    record handling with scripted origins, not a participant-agency observation.
    """
    resolved = directory.resolve()
    if resolved.is_relative_to(ROOT.resolve()) or not resolved.is_dir():
        raise HostSourceError("private_directory_required_outside_checkout")
    seeds = oracle.validate_events(oracle.load_json(Path(__file__).with_name("history.json")))
    tokens = {principal: secrets.token_urlsafe(24) for principal in (*AUTHORS, OBSERVER, NEWCOMER)}
    original_tokens = [tokens[principal] for principal in (*AUTHORS, OBSERVER)]
    try:
        with tempfile.TemporaryDirectory(prefix="agentciv-host-inheritance-", dir=resolved) as temporary:
            private = Path(temporary)
            config = configuration(private, [credential(principal, tokens[principal], write=principal in AUTHORS)
                                             for principal in (*AUTHORS, OBSERVER)])
            argv = walk.python_argv(config)
            first = walk.start_host(argv)
            try:
                origin = walk.wait_until_ready(first)
                accepted: list[JsonObject] = []
                for event in seeds:
                    record = seed_record(event)
                    accepted.append(submit_seed(origin, event, tokens[str(record["from"])]))
                before = client.history(origin, tokens[OBSERVER])
                if before != accepted or reflected(before, list(tokens.values())):
                    raise HostSourceError("seed_history_mismatch")
            finally:
                walk.stop_host(first)
            # Provision the newcomer only after original fixture clients are done.
            # Original grants are removed; no author credential is handed down.
            configuration(private, [credential(NEWCOMER, tokens[NEWCOMER], write=False)])
            restarted = walk.start_host(argv)
            try:
                origin = walk.wait_until_ready(restarted)
                inherited = client.history(origin, tokens[NEWCOMER])
                if inherited != before:
                    raise HostSourceError("restart_history_mismatch")
                for original_token in original_tokens:
                    status, _ = wire.exchange("GET", origin + "/events", token=original_token)
                    if status != 401:
                        raise HostSourceError("original_credential_still_authorized")
                status, _ = wire.exchange("POST", endpoint(origin), token=tokens[NEWCOMER], body=b"{}")
                if status != 403:
                    raise HostSourceError("newcomer_write_not_denied")
                if reflected(inherited, list(tokens.values())):
                    raise HostSourceError("credential_reflected")
                return oracle.validate_events(inherited)
            finally:
                walk.stop_host(restarted)
    except (wire.ParticipantError, client.DecisionError, walk.WalkFailure, OSError) as error:
        raise HostSourceError("host_source_failed") from error
