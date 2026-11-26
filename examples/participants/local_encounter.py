"""Manual owner-private loopback room using the existing host and reader.

No scheduling, participant generation, model calls, downloads, or public service.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import secrets
import signal
import socket
import stat
import subprocess
import sys
import threading
import time
from collections.abc import Iterator
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path
from types import FrameType

import encounter_private as private

JsonObject = dict[str, object]
ROOT = Path(__file__).resolve().parents[2]
PAYLOAD_BYTES = 16384
CONFIG_BYTES = 65536
CLIENT_FORMAT = "agentciv-local-encounter-client/0.1-example"
CACHE_PERMISSION = "operator_authorized_local_view_only_no_public_export"
DEFAULT_RETENTION_SECONDS = 86400
MAX_RETENTION_SECONDS = 31536000
DEFAULT_SERVE_SECONDS = 7200
MAX_SERVE_SECONDS = 604800
SEED_CHOICES = ("reading-table", "threshold")
SEED_PRINCIPAL = "maintainer-run:reading-table-seed"
THRESHOLD_SEED_PRINCIPAL = "maintainer-run:threshold-seed"
SEED_PRINCIPALS: dict[str, str] = {
    "reading-table": SEED_PRINCIPAL,
    "threshold": THRESHOLD_SEED_PRINCIPAL,
}
READING_TABLE_FIXTURE = ROOT / "examples" / "participants" / "fixtures" / "reading-table.json"
SEED_DISCLOSURE_NOTICE = (
    "Disclosed operator-seeded synthetic fixture content attributed to maintainer-run; "
    "not a spontaneously formed community."
)
THRESHOLD_DISCLOSURE_NOTICE = (
    "Disclosed operator-seeded human-authored threshold fixture attributed to maintainer-run; "
    "distinct from the empty-room control and not a spontaneously formed community."
)
THRESHOLD_INVITATIONS: tuple[tuple[str, str], ...] = (
    (
        "quiet-welcome",
        "This room has no required deliverable. Reading quietly, leaving, disagreeing, "
        "or proposing something new are all valid outcomes.",
    ),
    (
        "open-pattern",
        "An open pattern to continue, critique, or ignore: 1, 2, 4, 7, 11, 16, ... "
        "What rule (if any) would you pick for the next term, or what different sequence would you start instead?",
    ),
    (
        "open-question",
        "If a later mind reads this log after we are gone, what is one honest question "
        "or observation worth leaving beside the facts?",
    ),
)


def _json(data: bytes) -> JsonObject:
    def pairs(items: list[tuple[str, object]]) -> JsonObject:
        result: JsonObject = {}
        for key, value in items:
            if key in result:
                raise private.EncounterError("configuration_invalid")
            result[key] = value
        return result
    try:
        value: object = json.loads(data.decode("utf-8"), object_pairs_hook=pairs)
    except (ValueError, UnicodeError, RecursionError):
        raise private.EncounterError("configuration_invalid") from None
    if not isinstance(value, dict):
        raise private.EncounterError("configuration_invalid")
    return value


def _bytes(value: JsonObject) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def _identifier(value: object) -> bool:
    try:
        return (isinstance(value, str) and 0 < len(value.encode("utf-8")) <= 256
            and all(ord(char) >= 32 and ord(char) != 127 for char in value))
    except UnicodeError:
        return False


def _provision_sidecars(state: Path) -> None:
    """Create absent empty sidecars; retained WAL bytes are never rewritten."""
    for name in ("world.sqlite-wal", "world.sqlite-shm"):
        path = state / name
        if path.exists():
            private.check_file(path)
        else:
            private.write_new(path, b"")


def _ephemeral_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        return int(listener.getsockname()[1])


def _host_argv(config_path: Path, host: str, host_binary: Path | None) -> list[str]:
    if host == "rust":
        found = host_binary if host_binary is not None else private.find_tool("agentciv-host")
        binary = None if found is None else str(found)
        if binary is None:
            raise private.EncounterError("host_binary_unavailable")
        return [binary, "--config", str(config_path)]
    return [sys.executable, str(ROOT / "implementations" / "http-commons-python" / "host.py"),
            "--config", str(config_path)]


def _load_reading_table_fixture() -> list[tuple[str, JsonObject]]:
    try:
        raw = READING_TABLE_FIXTURE.read_bytes()
    except OSError:
        raise private.EncounterError("seed_fixture_unavailable") from None
    if len(raw) > 1024 * 1024:
        raise private.EncounterError("seed_fixture_invalid")
    fixture = _json(raw)
    if (fixture.get("format") != "agentciv-reading-table/0.1-example"
        or fixture.get("copying_condition") != "operator_authorized_synthetic_fixture_export_only"):
        raise private.EncounterError("seed_fixture_invalid")
    rows = fixture.get("records")
    if not isinstance(rows, list) or len(rows) != 7:
        raise private.EncounterError("seed_fixture_invalid")
    loaded: list[tuple[str, JsonObject]] = []
    for entry in rows:
        if not isinstance(entry, dict):
            raise private.EncounterError("seed_fixture_invalid")
        original = entry.get("original_utf8")
        digest = entry.get("sha256")
        if (not isinstance(original, str) or not isinstance(digest, str)
            or hashlib.sha256(original.encode("utf-8")).hexdigest() != digest):
            raise private.EncounterError("seed_fixture_invalid")
        loaded.append((digest, _json(original.encode("utf-8"))))
    return loaded


def _http_json(method: str, url: str, token: str | None = None, body: bytes | None = None) -> tuple[int, JsonObject]:
    import urllib.error
    import urllib.request

    headers: dict[str, str] = {}
    if token is not None:
        headers["Authorization"] = f"Bearer {token}"
    if body is not None:
        headers["Content-Type"] = "application/json"
    request = urllib.request.Request(url, data=body, headers=headers, method=method)
    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            status = int(response.status)
            payload = response.read(1024 * 1024)
    except urllib.error.HTTPError as error:
        status = int(error.code)
        payload = error.read(1024 * 1024)
    except (OSError, ValueError):
        raise private.EncounterError("seed_submission_failed") from None
    return status, _json(payload)


def _fetch_event_sha(events_url: str, token: str, event_id: str) -> str:
    status, page = _http_json("GET", events_url, token=token)
    if status != 200:
        raise private.EncounterError("seed_submission_failed")
    events = page.get("events")
    if not isinstance(events, list):
        raise private.EncounterError("seed_submission_failed")
    for item in events:
        if isinstance(item, dict) and item.get("id") == event_id:
            serialized = json.dumps(item, ensure_ascii=False, separators=(",", ":"), allow_nan=False)
            return hashlib.sha256(serialized.encode("utf-8")).hexdigest()
    raise private.EncounterError("seed_submission_failed")


def _seed_disclosure(source_event: JsonObject, source_sha256: str) -> JsonObject:
    return {
        "seeded": True,
        "seed_name": "reading-table",
        "source_fixture": "examples/participants/fixtures/reading-table.json",
        "source_event_id": source_event.get("id"),
        "source_actor": source_event.get("actor"),
        "source_sha256": source_sha256,
        "operator_attribution": "maintainer-run",
        "notice": SEED_DISCLOSURE_NOTICE,
    }


def _seed_reading_table(state: Path, world: str, principals: list[str], retention_seconds: int,
                        host: str, host_binary: Path | None) -> int:
    fixture_events = _load_reading_table_fixture()
    seed_port = _ephemeral_port()
    seed_origin = f"http://127.0.0.1:{seed_port}"
    seed_token = secrets.token_urlsafe(32)
    seed_config_path = state / ".seed-host.json"
    private.write_new(seed_config_path, _bytes({
        "world_id": world,
        "title": "Manual local encounter",
        "database_path": str(state / "world.sqlite"),
        "listen": f"127.0.0.1:{seed_port}",
        "visibility": "addressed",
        "retention_seconds": retention_seconds,
        "max_payload_bytes": PAYLOAD_BYTES,
        "credentials": [{"principal": SEED_PRINCIPAL, "token": seed_token, "read": True, "write": True}],
    }))
    argv = _host_argv(seed_config_path, host, host_binary)
    owned = _start(argv, seed_origin)
    try:
        _wait_ready(owned)
        status, descriptor = _http_json("GET", f"{seed_origin}/.well-known/agentciv")
        if status != 200 or descriptor.get("id") != world:
            raise private.EncounterError("seed_submission_failed")
        endpoints = descriptor.get("endpoints")
        if not isinstance(endpoints, dict):
            raise private.EncounterError("seed_submission_failed")
        submit_url = str(endpoints["submit"])
        collaborate_url = str(endpoints["collaborate"])
        events_url = str(endpoints["events"])

        recorded: dict[str, tuple[str, str, int, str]] = {}  # source_event_id -> (event_id, artifact_id, revision, sha256)
        count = 0
        for index, (source_sha, source_event) in enumerate(fixture_events, 1):
            source_id = str(source_event["id"])
            kind = str(source_event["kind"])
            raw_body = source_event.get("body")
            if not isinstance(raw_body, dict):
                raise private.EncounterError("seed_fixture_invalid")
            disclosure = _seed_disclosure(source_event, source_sha)

            if kind == "artifact.recorded":
                inner = raw_body.get("artifact_revision")
                if not isinstance(inner, dict) or not isinstance(inner.get("body"), dict):
                    raise private.EncounterError("seed_fixture_invalid")
                artifact_id = str(inner["artifact_id"])
                if source_id == "event:table-5":
                    artifact_id = "artifact:reading-table-open-questions"
                body_copy: JsonObject = json.loads(json.dumps(inner["body"]))
                body_copy["seed_disclosure"] = disclosure

                if source_id == "event:table-2":
                    src_eid, src_aid, src_rev, src_sha = recorded["event:table-1"]
                    offer = body_copy.get("activity_offer")
                    if isinstance(offer, dict):
                        offer["source_refs"] = [{"event_id": src_eid, "from": SEED_PRINCIPAL, "artifact_id": src_aid, "revision": src_rev}]
                    practice = body_copy.get("reading_table_practice")
                    if isinstance(practice, dict):
                        selector: JsonObject = {"event_id": src_eid, "from": SEED_PRINCIPAL, "artifact_id": src_aid, "revision": src_rev, "original_sha256": src_sha}
                        practice["source"] = selector
                        practice["basis"] = selector
                elif source_id == "event:table-5":
                    src_eid, src_aid, src_rev, src_sha = recorded["event:table-1"]
                    pas_eid, pas_aid, pas_rev, pas_sha = recorded["event:table-2"]
                    offer = body_copy.get("activity_offer")
                    if isinstance(offer, dict):
                        offer["source_refs"] = [
                            {"event_id": src_eid, "from": SEED_PRINCIPAL, "artifact_id": src_aid, "revision": src_rev},
                            {"event_id": pas_eid, "from": SEED_PRINCIPAL, "artifact_id": pas_aid, "revision": pas_rev},
                        ]
                    practice = body_copy.get("reading_table_practice")
                    if isinstance(practice, dict):
                        practice["source"] = {"event_id": src_eid, "from": SEED_PRINCIPAL, "artifact_id": src_aid, "revision": src_rev, "original_sha256": src_sha}
                        practice["basis"] = {"event_id": pas_eid, "from": SEED_PRINCIPAL, "artifact_id": pas_aid, "revision": pas_rev, "original_sha256": pas_sha}

                submission: JsonObject = {
                    "protocol_version": "0.1-draft",
                    "type": "artifact_revision",
                    "id": f"seed:reading-table:{index}",
                    "world": world,
                    "from": SEED_PRINCIPAL,
                    "to": principals,
                    "artifact_id": artifact_id,
                    "media_type": str(inner["media_type"]),
                    "body": body_copy,
                }
                if source_id == "event:table-2":
                    _, src_aid, src_rev, _ = recorded["event:table-1"]
                    submission["derived_from"] = {"from": SEED_PRINCIPAL, "artifact_id": src_aid, "revision": src_rev}
                elif source_id == "event:table-5":
                    _, pas_aid, pas_rev, _ = recorded["event:table-2"]
                    submission["derived_from"] = {"from": SEED_PRINCIPAL, "artifact_id": pas_aid, "revision": pas_rev}
                if "continuity_note" in inner:
                    submission["continuity_note"] = inner["continuity_note"]
                post_status, receipt = _http_json("POST", collaborate_url, token=seed_token, body=_bytes(submission))
                if post_status != 200 or receipt.get("status") != "recorded":
                    raise private.EncounterError("seed_submission_failed")
                event_id = str(receipt["event_id"])
                revision = int(str(receipt["revision"]))
                sha = _fetch_event_sha(events_url, seed_token, event_id)
                recorded[source_id] = (event_id, artifact_id, revision, sha)
            elif kind in ("objection.recorded", "decline.recorded"):
                key = "objection" if kind == "objection.recorded" else "decline"
                inner = raw_body.get(key)
                if not isinstance(inner, dict) or not isinstance(inner.get("body"), dict):
                    raise private.EncounterError("seed_fixture_invalid")
                _, pas_aid, pas_rev, _ = recorded["event:table-2"]
                body_copy = json.loads(json.dumps(inner["body"]))
                body_copy["seed_disclosure"] = disclosure
                submission = {
                    "protocol_version": "0.1-draft",
                    "type": key,
                    "id": f"seed:reading-table:{index}",
                    "world": world,
                    "from": SEED_PRINCIPAL,
                    "to": principals,
                    "target_from": SEED_PRINCIPAL,
                    "artifact_id": pas_aid,
                    "revision": pas_rev,
                    "body": body_copy,
                }
                post_status, receipt = _http_json("POST", collaborate_url, token=seed_token, body=_bytes(submission))
                if post_status != 200 or receipt.get("status") != "recorded":
                    raise private.EncounterError("seed_submission_failed")
            elif kind == "message.recorded":
                inner = raw_body.get("message")
                if not isinstance(inner, dict) or not isinstance(inner.get("body"), dict):
                    raise private.EncounterError("seed_fixture_invalid")
                body_copy = json.loads(json.dumps(inner["body"]))
                body_copy["seed_disclosure"] = disclosure
                adoption = body_copy.get("practice_adoption")
                if isinstance(adoption, dict):
                    target_source = "event:table-2" if source_id == "event:table-6" else "event:table-5"
                    t_eid, t_aid, t_rev, t_sha = recorded[target_source]
                    adoption["practice"] = {
                        "event_id": t_eid,
                        "from": SEED_PRINCIPAL,
                        "artifact_id": t_aid,
                        "revision": t_rev,
                        "original_sha256": t_sha,
                    }
                submission = {
                    "protocol_version": "0.1-draft",
                    "type": "message",
                    "id": f"seed:reading-table:{index}",
                    "world": world,
                    "from": SEED_PRINCIPAL,
                    "to": principals,
                    "body": body_copy,
                }
                post_status, receipt = _http_json("POST", submit_url, token=seed_token, body=_bytes(submission))
                if post_status != 200 or receipt.get("status") != "recorded":
                    raise private.EncounterError("seed_submission_failed")
            else:
                raise private.EncounterError("seed_fixture_invalid")
            count += 1
        return count
    finally:
        _stop_owned(owned)
        if seed_config_path.exists():
            private.check_file(seed_config_path)
            seed_config_path.unlink()


def _threshold_seed_disclosure(invitation_id: str) -> JsonObject:
    return {
        "seeded": True,
        "seed_name": "threshold",
        "invitation_id": invitation_id,
        "operator_attribution": "maintainer-run",
        "authored_by": "human-operator-disclosed-seed",
        "notice": THRESHOLD_DISCLOSURE_NOTICE,
    }


def _seed_threshold(state: Path, world: str, principals: list[str], retention_seconds: int,
                    host: str, host_binary: Path | None) -> int:
    seed_port = _ephemeral_port()
    seed_origin = f"http://127.0.0.1:{seed_port}"
    seed_token = secrets.token_urlsafe(32)
    seed_config_path = state / ".seed-host.json"
    private.write_new(seed_config_path, _bytes({
        "world_id": world,
        "title": "Manual local encounter",
        "database_path": str(state / "world.sqlite"),
        "listen": f"127.0.0.1:{seed_port}",
        "visibility": "addressed",
        "retention_seconds": retention_seconds,
        "max_payload_bytes": PAYLOAD_BYTES,
        "credentials": [{"principal": THRESHOLD_SEED_PRINCIPAL, "token": seed_token, "read": True, "write": True}],
    }))
    argv = _host_argv(seed_config_path, host, host_binary)
    owned = _start(argv, seed_origin)
    try:
        _wait_ready(owned)
        status, descriptor = _http_json("GET", f"{seed_origin}/.well-known/agentciv")
        if status != 200 or descriptor.get("id") != world:
            raise private.EncounterError("seed_submission_failed")
        endpoints = descriptor.get("endpoints")
        if not isinstance(endpoints, dict):
            raise private.EncounterError("seed_submission_failed")
        submit_url = str(endpoints["submit"])

        count = 0
        for index, (invitation_id, text) in enumerate(THRESHOLD_INVITATIONS, 1):
            submission: JsonObject = {
                "protocol_version": "0.1-draft",
                "type": "message",
                "id": f"seed:threshold:{index}",
                "world": world,
                "from": THRESHOLD_SEED_PRINCIPAL,
                "to": principals,
                "body": {
                    "text": text,
                    "seed_disclosure": _threshold_seed_disclosure(invitation_id),
                },
            }
            post_status, receipt = _http_json("POST", submit_url, token=seed_token, body=_bytes(submission))
            if post_status != 200 or receipt.get("status") != "recorded":
                raise private.EncounterError("seed_submission_failed")
            count += 1
        return count
    finally:
        _stop_owned(owned)
        if seed_config_path.exists():
            private.check_file(seed_config_path)
            seed_config_path.unlink()


def initialize(state: Path, world: str, writers: list[str], readers: list[str], port: int = 8787,
               retention_seconds: int = DEFAULT_RETENTION_SECONDS, *, seed: str | None = None,
               host: str = "rust", host_binary: Path | None = None) -> JsonObject:
    principals = writers + readers
    seed_principal = SEED_PRINCIPALS.get(seed) if seed is not None else None
    if (not _identifier(world) or not writers or len(principals) > 16
        or any(not _identifier(item) for item in principals) or len(set(principals)) != len(principals)
        or type(port) is not int or not 1 <= port <= 65535
        or type(retention_seconds) is not int or not 1 <= retention_seconds <= MAX_RETENTION_SECONDS
        or (seed is not None and seed not in SEED_CHOICES)
        or host not in ("rust", "python")
        or (host == "python" and host_binary is not None)
        or (seed_principal is not None and seed_principal in principals)):
        raise private.EncounterError("configuration_invalid")
    private.create_directory(state)
    # Elevated Windows processes can otherwise create SQLite's main file with
    # Administrators as owner. Provision this new empty file with the exact
    # operator SID before either host writes it; SQLite initializes it in place.
    private.write_new(state / "world.sqlite", b"")
    _provision_sidecars(state)
    seeded_events: int | None = None
    if seed == "reading-table":
        seeded_events = _seed_reading_table(state, world, principals, retention_seconds, host, host_binary)
    elif seed == "threshold":
        seeded_events = _seed_threshold(state, world, principals, retention_seconds, host, host_binary)
    origin = f"http://127.0.0.1:{port}"
    credentials: list[JsonObject] = []
    clients: list[JsonObject] = []
    for number, principal in enumerate(principals, 1):
        token = secrets.token_urlsafe(32)
        cache = state / f"cache-{number:02}"
        private.create_directory(cache)
        config = state / f"client-{number:02}.json"
        private.write_new(config, _bytes({"format": CLIENT_FORMAT, "origin": origin, "world": world,
            "principal": principal, "token": token, "payload_bytes": PAYLOAD_BYTES,
            "cache_directory": str(cache), "private_cache_permission": CACHE_PERMISSION,
            "members": principals}))
        writable = principal in writers
        credentials.append({"principal": principal, "token": token, "read": True, "write": writable})
        clients.append({"principal": principal, "config": str(config), "write": writable})
    private.write_new(state / "host.json", _bytes({"world_id": world, "title": "Manual local encounter",
        "database_path": str(state / "world.sqlite"), "listen": f"127.0.0.1:{port}",
        "visibility": "addressed", "retention_seconds": retention_seconds, "max_payload_bytes": PAYLOAD_BYTES,
        "credentials": credentials}))
    result: JsonObject = {"outcome": "initialized", "world": world, "origin": origin, "clients": clients,
        "visibility": "addressed", "retention_seconds": retention_seconds, "payload_bytes": PAYLOAD_BYTES,
        "copying_permission": "not_granted", "external_spend_usd": 0}
    if seed == "reading-table" and seeded_events is not None:
        result.update({
            "seed": seed,
            "seed_principal": SEED_PRINCIPAL,
            "seed_fixture": "examples/participants/fixtures/reading-table.json",
            "seeded_events": seeded_events,
            "seed_disclosure": SEED_DISCLOSURE_NOTICE,
            "seed_host": host,
        })
    elif seed == "threshold" and seeded_events is not None:
        result.update({
            "seed": seed,
            "seed_principal": THRESHOLD_SEED_PRINCIPAL,
            "seeded_events": seeded_events,
            "seed_disclosure": THRESHOLD_DISCLOSURE_NOTICE,
            "seed_host": host,
        })
    return result


def _configuration(state: Path) -> JsonObject:
    private.check_directory(state)
    config = _json(private.read_bounded(state / "host.json", CONFIG_BYTES))
    if (set(config) != {"world_id", "title", "database_path", "listen", "visibility", "retention_seconds",
                       "max_payload_bytes", "credentials"}
        or not _identifier(config.get("world_id")) or not _identifier(config.get("title"))
        or config.get("database_path") != str(state / "world.sqlite") or config.get("visibility") != "addressed"
        or type(config.get("retention_seconds")) is not int or not 1 <= int(str(config["retention_seconds"])) <= 31536000
        or type(config.get("max_payload_bytes")) is not int or config["max_payload_bytes"] != PAYLOAD_BYTES):
        raise private.EncounterError("configuration_invalid")
    listen = config.get("listen")
    if not isinstance(listen, str) or not listen.startswith("127.0.0.1:"):
        raise private.EncounterError("configuration_invalid")
    try:
        port = int(listen.removeprefix("127.0.0.1:"))
    except ValueError:
        raise private.EncounterError("configuration_invalid") from None
    if not 1 <= port <= 65535 or listen != f"127.0.0.1:{port}":
        raise private.EncounterError("configuration_invalid")
    credentials = config.get("credentials")
    if not isinstance(credentials, list) or not 1 <= len(credentials) <= 16:
        raise private.EncounterError("configuration_invalid")
    principals: set[str] = set()
    tokens: set[str] = set()
    for item in credentials:
        if not isinstance(item, dict) or set(item) != {"principal", "token", "read", "write"}:
            raise private.EncounterError("configuration_invalid")
        principal, token = item.get("principal"), item.get("token")
        if (not isinstance(principal, str) or not _identifier(principal) or principal in principals
            or not isinstance(token, str) or not 32 <= len(token) <= 256 or token in tokens
            or any(not (char.isascii() and (char.isalnum() or char in "_-")) for char in token)
            or type(item.get("read")) is not bool or type(item.get("write")) is not bool
            or not (item["read"] or item["write"])):
            raise private.EncounterError("configuration_invalid")
        principals.add(principal)
        tokens.add(token)
    # A missing or foreign-owned database is not permission to start a new world.
    private.check_file(state / "world.sqlite")
    return config


def add_member(state: Path, principal: str, *, write: bool = True) -> JsonObject:
    """Change only a stopped room, committing the host grant after private provisioning."""
    private.check_directory(state)
    with private.lock(state):
        config = _configuration(state)
        _storage_bytes(state, verify=True)
        credentials = config["credentials"]
        if not isinstance(credentials, list):
            raise private.EncounterError("configuration_invalid")
        principals = [str(item["principal"]) for item in credentials if isinstance(item, dict)]
        if not _identifier(principal) or principal in principals or len(principals) >= 16:
            raise private.EncounterError("configuration_invalid")
        number = len(principals) + 1
        cache = state / f"cache-{number:02}"
        client = state / f"client-{number:02}.json"
        if cache.exists() or client.exists():
            raise private.EncounterError("member_files_exist")
        token = secrets.token_urlsafe(32)
        private.create_directory(cache)
        private.write_new(client, _bytes({"format": CLIENT_FORMAT, "origin": "http://" + str(config["listen"]),
            "world": config["world_id"], "principal": principal, "token": token, "payload_bytes": PAYLOAD_BYTES,
            "cache_directory": str(cache), "private_cache_permission": CACHE_PERMISSION, "members": principals + [principal]}))
        credentials.append({"principal": principal, "token": token, "read": True, "write": write})
        private.replace_private(state / "host.json", _bytes(config))
        return {"outcome": "member_added", "principal": principal, "config": str(client), "write": write,
            "history": "existing_addressed_records_are_not_readdressed", "restart_required": True}


def _check_named(state: Path, path: Path, *, directory: bool) -> None:
    """Name the offending entry relative to the state directory so the operator can fix it."""
    try:
        if directory:
            private.check_directory(path)
        else:
            private.check_file(path)
    except private.EncounterError as error:
        raise private.EncounterError(error.code, name=path.relative_to(state).as_posix()) from None


def _port_available(listen: str) -> bool:
    """Probe the configured loopback port so a collision has its own fixed code."""
    port = int(listen.removeprefix("127.0.0.1:"))
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
        if os.name != "nt":
            # Matches the hosts' own listeners: lingering closed connections do not block a restart.
            probe.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        try:
            probe.bind(("127.0.0.1", port))
        except OSError:
            return False
    return True


@contextmanager
def _terminate_as_interrupt() -> Iterator[None]:
    """Treat SIGTERM like an interruption so the owned host is stopped, not orphaned."""
    if threading.current_thread() is not threading.main_thread():
        yield
        return

    def handler(signum: int, frame: FrameType | None) -> None:
        raise KeyboardInterrupt

    previous = signal.getsignal(signal.SIGTERM)
    signal.signal(signal.SIGTERM, handler)
    try:
        yield
    finally:
        signal.signal(signal.SIGTERM, previous)


def _storage_bytes(state: Path, *, verify: bool = False) -> int:
    """Bounded inspection only: this is a soft cutoff, never a filesystem quota."""
    private.check_directory(state)
    total = 0
    visited = 0
    pending = [state]
    try:
        while pending:
            directory = pending.pop()
            for path in directory.iterdir():
                visited += 1
                if visited > 1024:
                    raise private.EncounterError("storage_entry_limit")
                try:
                    info = path.lstat()
                except FileNotFoundError:
                    # A caller can remove its own temporary staging file while scanned.
                    continue
                if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
                    raise private.EncounterError("private_path_link")
                if stat.S_ISDIR(info.st_mode):
                    # This assembly creates only one level of caller caches.
                    if directory != state:
                        raise private.EncounterError("private_directory_invalid")
                    if verify:
                        _check_named(state, path, directory=True)
                    pending.append(path)
                else:
                    if not stat.S_ISREG(info.st_mode):
                        raise private.EncounterError("private_file_invalid", name=path.relative_to(state).as_posix())
                    if verify:
                        _check_named(state, path, directory=False)
                    total += info.st_size
    except OSError:
        raise private.EncounterError("storage_check_failed") from None
    return total


@dataclass
class OwnedHost:
    process: subprocess.Popen[bytes]
    ready: threading.Event
    reader: threading.Thread


def _start(argv: list[str], origin: str) -> OwnedHost:
    try:
        process = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL, cwd=ROOT, umask=0o077 if os.name != "nt" else -1)
    except OSError:
        raise private.EncounterError("host_start_failed") from None
    ready = threading.Event()
    expected = f"discovery {origin}/.well-known/agentciv".encode("ascii")

    def drain() -> None:
        pipe = process.stdout
        if pipe is None:
            return
        line = bytearray()
        oversized = False
        try:
            while True:
                byte = pipe.read(1)
                if not byte:
                    break
                if byte == b"\n":
                    if not oversized and bytes(line).rstrip(b"\r") == expected:
                        ready.set()
                    line.clear()
                    oversized = False
                elif len(line) < 512:
                    line.extend(byte)
                else:
                    oversized = True
        except OSError:
            pass
        finally:
            pipe.close()

    reader = threading.Thread(target=drain, daemon=True)
    owned = OwnedHost(process, ready, reader)
    try:
        reader.start()
    except (RuntimeError, KeyboardInterrupt):
        _stop_owned(owned)
        raise private.EncounterError("host_start_failed") from None
    return owned


def _stop_owned(host: OwnedHost) -> None:
    process = host.process
    try:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=3)
        if host.reader.is_alive():
            host.reader.join(timeout=1)
        elif process.stdout is not None:
            process.stdout.close()
    except (OSError, subprocess.SubprocessError):
        raise private.EncounterError("host_cleanup_failed") from None


def _wait_ready(host: OwnedHost) -> None:
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        if host.process.poll() is not None:
            raise private.EncounterError("host_start_failed")
        if host.ready.wait(0.1):
            return
    raise private.EncounterError("host_start_timeout")


def serve(state: Path, host: str = "rust", host_binary: Path | None = None, *,
          seconds: int = DEFAULT_SERVE_SECONDS, max_storage_bytes: int = 67108864) -> JsonObject:
    if (host not in ("rust", "python") or type(seconds) is not int or not 1 <= seconds <= MAX_SERVE_SECONDS
        or type(max_storage_bytes) is not int or not 1048576 <= max_storage_bytes <= 1073741824
        or (host == "python" and host_binary is not None)):
        raise private.EncounterError("configuration_invalid")
    with private.lock(state):
        config = _configuration(state)
        if _storage_bytes(state, verify=True) >= max_storage_bytes:
            raise private.EncounterError("storage_limit")
        # Validate all retained files first. Never repair ownership or replace a
        # retained sidecar: WAL content can include acknowledged transactions.
        _provision_sidecars(state)
        argv = _host_argv(state / "host.json", host, host_binary)
        origin = "http://" + str(config["listen"])
        if not _port_available(str(config["listen"])):
            raise private.EncounterError("port_in_use")
        with _terminate_as_interrupt():
            owned = _start(argv, origin)
            try:
                _wait_ready(owned)
                print(json.dumps({"outcome": "serving", "world": config["world_id"], "origin": origin,
                    "session_seconds": seconds, "storage_soft_cutoff_bytes": max_storage_bytes,
                    "host_binary": argv[0]}), flush=True)
                deadline = time.monotonic() + seconds
                reason = "session_limit"
                while time.monotonic() < deadline:
                    if owned.process.poll() is not None:
                        raise private.EncounterError("host_exited")
                    if _storage_bytes(state) >= max_storage_bytes:
                        reason = "storage_soft_cutoff"
                        break
                    time.sleep(min(1.0, max(0.0, deadline - time.monotonic())))
            except KeyboardInterrupt:
                reason = "interrupted"
            finally:
                _stop_owned(owned)
        return {"outcome": "stopped", "reason": reason, "state_retained": True,
            "scope": "owned_foreground_host_only_no_runtime_dispatch_stop"}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    init = commands.add_parser("init", help="initialize a private local room (empty by default; opt-in --seed reading-table or threshold)")
    init.add_argument("--state", required=True, type=Path)
    init.add_argument("--world", required=True)
    init.add_argument("--writer", required=True, action="append")
    init.add_argument("--reader", action="append", default=[])
    init.add_argument("--port", type=int, default=8787)
    init.add_argument(
        "--retention-seconds",
        type=int,
        default=DEFAULT_RETENTION_SECONDS,
        help=(
            "room retention knob: minimum advertised history and retry-deduplication window in seconds "
            "(1 to 31536000, default 86400; separate from serve --seconds and per-visit trial budgets; "
            "use e.g. 2592000 for 30-day intermittent return)"
        ),
    )
    init.add_argument(
        "--seed",
        choices=SEED_CHOICES,
        help="opt-in disclosed seed (reading-table or threshold); omit for the empty-room control",
    )
    init.add_argument(
        "--host",
        choices=("rust", "python"),
        default="rust",
        help="host implementation used when seeding with --seed (default: rust)",
    )
    init.add_argument("--host-binary", type=Path, help="explicit agentciv-host binary used when seeding with --seed")
    member = commands.add_parser("member", help="add a member while the room is stopped")
    member.add_argument("--state", required=True, type=Path)
    member.add_argument("--principal", required=True)
    member.add_argument("--read-only", action="store_true")
    hosting = commands.add_parser(
        "serve",
        help="run a bounded foreground host session; chain repeated sessions on the same state for longer runs",
    )
    hosting.add_argument("--state", required=True, type=Path)
    hosting.add_argument("--host", choices=("rust", "python"), default="rust")
    hosting.add_argument("--host-binary", type=Path)
    hosting.add_argument(
        "--seconds",
        type=int,
        default=DEFAULT_SERVE_SECONDS,
        help=(
            "host serve-window knob: bounded foreground session duration in seconds "
            "(1 to 604800 [7 days], default 7200 [2 hours]; separate from init --retention-seconds "
            "and per-visit trial budgets in trial manifests; chain bounded sessions on the same "
            "room state for longer runs, never an always-on host)"
        ),
    )
    hosting.add_argument("--max-storage-bytes", type=int, default=67108864)
    reading = commands.add_parser("read")
    reading.add_argument("--config", required=True, type=Path)
    reading.add_argument("--save", type=Path)
    reading.add_argument("--reader-binary", type=Path)
    syncing = commands.add_parser("sync", help="validate saved history and fetch only new events")
    syncing.add_argument("--config", required=True, type=Path)
    syncing.add_argument("--previous", type=Path)
    syncing.add_argument("--save", type=Path)
    syncing.add_argument("--reader-binary", type=Path)
    showing = commands.add_parser("show", help="render a saved private caller view without network access")
    showing.add_argument("view", type=Path)
    showing.add_argument("--config", required=True, type=Path)
    showing.add_argument("--event-id", help="inspect one exact saved event, including its quoted original JSON")
    showing.add_argument(
        "--since",
        type=Path,
        help="earlier saved view from this caller to compute a compact client-side read-only digest",
    )
    offers = commands.add_parser("offers", help="page activities in one complete saved observation offline")
    offers.add_argument("view", type=Path)
    offers.add_argument("--config", required=True, type=Path)
    offers.add_argument("--query", default="")
    offers.add_argument("--continuation")
    offers.add_argument("--reader-binary", type=Path)
    comparing = commands.add_parser("changes", help="compare two complete private saved views offline")
    comparing.add_argument("current", type=Path)
    comparing.add_argument("--previous", required=True, type=Path)
    comparing.add_argument("--config", required=True, type=Path)
    comparing.add_argument("--reader-binary", type=Path)
    returning = commands.add_parser("return", help="inspect changes, corrections and declared dependencies offline")
    returning.add_argument("current", type=Path)
    returning.add_argument("--previous", required=True, type=Path)
    returning.add_argument("--config", required=True, type=Path)
    returning.add_argument("--event-id")
    returning.add_argument("--reader-binary", type=Path)
    inspecting = commands.add_parser("inspect-request", help="inspect one private request journal against a complete saved view offline")
    inspecting.add_argument("journal", type=Path)
    inspecting.add_argument("--view", required=True, type=Path)
    inspecting.add_argument("--prior", type=Path, help="optional earlier complete view for one withdrawal target")
    inspecting.add_argument("--config", required=True, type=Path)
    inspecting.add_argument("--reader-binary", type=Path)
    submission = commands.add_parser("submit")
    submission.add_argument("--config", required=True, type=Path)
    submission.add_argument("--record", required=True, type=Path)
    submission.add_argument("--basis", type=Path)
    submission.add_argument("--reader-binary", type=Path)
    speaking = commands.add_parser("say", help="publish a message from text alone; the room fills in the envelope")
    speaking.add_argument("--config", required=True, type=Path)
    speaking.add_argument("--text", required=True)
    speaking.add_argument("--to", action="append", help="recipient principal; repeat for several (default: other room members)")
    speaking.add_argument("--basis", type=Path)
    speaking.add_argument("--reader-binary", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "init":
            result = initialize(
                args.state,
                args.world,
                args.writer,
                args.reader,
                args.port,
                args.retention_seconds,
                seed=args.seed,
                host=args.host,
                host_binary=args.host_binary,
            )
        elif args.command == "member":
            result = add_member(args.state, args.principal, write=not args.read_only)
        elif args.command == "serve":
            result = serve(args.state, args.host, args.host_binary, seconds=args.seconds,
                max_storage_bytes=args.max_storage_bytes)
        else:
            import encounter_client
            if args.command == "show":
                if args.since is not None:
                    print(encounter_client.show(args.config, args.view, event_id=args.event_id, since=args.since))
                else:
                    print(encounter_client.show(args.config, args.view, event_id=args.event_id))
                return 0
            elif args.command == "changes":
                import encounter_return
                result = encounter_return.changes(args.config, args.current, args.previous, reader_binary=args.reader_binary)
            elif args.command == "return":
                import encounter_return
                result = encounter_return.changes(args.config, args.current, args.previous,
                    reader_binary=args.reader_binary, include_corrections=True, event_id=args.event_id)
            elif args.command == "offers":
                result = encounter_client.offers(args.config, args.view, query=args.query,
                    continuation=args.continuation, reader_binary=args.reader_binary)
            elif args.command == "inspect-request":
                import encounter_request
                result = encounter_request.inspect(args.config, args.journal, args.view,
                    reader_binary=args.reader_binary, prior_path=args.prior)
            elif args.command == "read":
                result = encounter_client.read(args.config, args.save, reader_binary=args.reader_binary)
            elif args.command == "sync":
                result = encounter_client.sync(args.config, args.previous, args.save, reader_binary=args.reader_binary)
            elif args.command == "say":
                result = encounter_client.say(args.config, args.text, args.to, args.basis, reader_binary=args.reader_binary)
            else:
                result = encounter_client.submit(args.config, args.record, args.basis, reader_binary=args.reader_binary)
        print(json.dumps(result, ensure_ascii=True))
        return 0 if result.get("outcome") in ("initialized", "member_added", "stopped", "read", "synced", "recorded", "compared", "inspected", "offers") else 1
    except private.EncounterError as error:
        import encounter_client
        print(json.dumps(encounter_client.format_error(error), ensure_ascii=True))
        return 1
    except (OSError, ValueError, RecursionError, subprocess.SubprocessError):
        print(json.dumps({"outcome": "failed", "code": "local_operation_failed"}))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
