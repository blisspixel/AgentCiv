"""Loopback HTTP Commons host.

This process implements the draft profile in PROTOCOL.md for one world.
It does not complete that profile claim, and a passing public report from
this process does not show interoperability.
"""

from __future__ import annotations

import argparse
import hashlib
import hmac
import ipaddress
import json
import math
import secrets
import socket
import socketserver
import sqlite3
import sys
import threading
import time
import urllib.parse
from collections.abc import Callable
from dataclasses import dataclass
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import TypeGuard

JsonObject = dict[str, object]

PAGE_LIMIT = 100
MAX_PAYLOAD_CAP = 8 * 1024 * 1024
PROTOCOL_VERSION = "0.1-draft"
PROFILE = "http-commons/0.1-draft"
# Container nesting the Rust host's serde_json parser accepts; deeper bodies are malformed.
MAX_JSON_DEPTH = 127
MAX_CHUNK_LINE = 1024

REASONS = {
    200: "OK",
    400: "Bad Request",
    401: "Unauthorized",
    403: "Forbidden",
    404: "Not Found",
    405: "Method Not Allowed",
    409: "Conflict",
    410: "Gone",
    413: "Payload Too Large",
    415: "Unsupported Media Type",
    422: "Unprocessable Content",
    500: "Internal Server Error",
}


class ConfigError(Exception):
    pass


class Conflict(Exception):
    pass


class UnknownTarget(Exception):
    """The cited revision is missing, or the caller cannot see it."""


class NotAuthor(Exception):
    """A visible revision can be withdrawn only by the principal who submitted it."""


class StorageFailure(Exception):
    pass


class InvalidCursor(Exception):
    pass


class ReadForbidden(Exception):
    def __init__(self, title: str = "Read access is required") -> None:
        self.title = title


class CursorExpired(Exception):
    pass


@dataclass(frozen=True)
class Credential:
    principal: str
    token: str
    read: bool
    write: bool


@dataclass(frozen=True)
class HostConfig:
    world_id: str
    title: str
    database_path: Path
    listen: tuple[str, int]
    visibility: str
    retention_seconds: int
    max_payload_bytes: int
    credentials: tuple[Credential, ...]


def parse_listen(value: str) -> tuple[str, int]:
    if value.startswith("["):
        end = value.find("]")
        if end < 0 or not value[end:].startswith("]:"):
            raise ConfigError("listen address is invalid")
        host = value[1:end]
        port_text = value[end + 2 :]
    else:
        host, separator, port_text = value.rpartition(":")
        if not separator or not host:
            raise ConfigError("listen address is invalid")
    try:
        port = int(port_text)
        ipaddress.ip_address(host)
    except ValueError as error:
        raise ConfigError("listen address is invalid") from error
    if port < 0 or port > 65535:
        raise ConfigError("listen address is invalid")
    return host, port


def checked_config(config: HostConfig) -> HostConfig:
    if not config.world_id or not config.title:
        raise ConfigError("world id and title are required")
    if config.visibility not in {"sender_only", "addressed", "members"}:
        raise ConfigError("visibility is invalid")
    try:
        loopback = ipaddress.ip_address(config.listen[0]).is_loopback
    except ValueError as error:
        raise ConfigError("listen address is invalid") from error
    if not loopback:
        raise ConfigError("listen address must be loopback")
    if config.retention_seconds < 1 or config.max_payload_bytes < 1024:
        raise ConfigError("retention and payload limit are below the profile minimum")
    if config.max_payload_bytes > MAX_PAYLOAD_CAP:
        raise ConfigError("payload limit exceeds what this process will accept")
    if not config.credentials:
        raise ConfigError("at least one credential is required")
    if not config.database_path.parent.exists():
        raise ConfigError("database directory does not exist")
    principals: list[str] = []
    tokens: list[str] = []
    for credential in config.credentials:
        if (
            not credential.principal
            or not credential.token
            or (not credential.read and not credential.write)
        ):
            raise ConfigError(
                "each credential needs a principal, token, and a read or write grant"
            )
        if credential.principal in principals or credential.token in tokens:
            raise ConfigError("credentials must use distinct principals and tokens")
        principals.append(credential.principal)
        tokens.append(credential.token)
    return config


def load_config(path: Path) -> HostConfig:
    try:
        file = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ConfigError("configuration file could not be read") from error
    if not isinstance(file, dict):
        raise ConfigError("configuration file is not valid JSON")
    try:
        listen = parse_listen(file["listen"])
        credentials = []
        for item in file["credentials"]:
            if not isinstance(item, dict):
                raise ConfigError("configuration file is not valid JSON")
            if not isinstance(item.get("read"), bool) or not isinstance(item.get("write"), bool):
                raise ConfigError("configuration file is not valid JSON")
            if not isinstance(item.get("principal"), str) or not isinstance(item.get("token"), str):
                raise ConfigError("configuration file is not valid JSON")
            credentials.append(
                Credential(
                    principal=item["principal"],
                    token=item["token"],
                    read=item["read"],
                    write=item["write"],
                )
            )
        config = HostConfig(
            world_id=file["world_id"],
            title=file["title"],
            database_path=Path(file["database_path"]),
            listen=listen,
            visibility=file["visibility"],
            retention_seconds=file["retention_seconds"],
            max_payload_bytes=file["max_payload_bytes"],
            credentials=tuple(credentials),
        )
    except ConfigError:
        raise
    except (KeyError, TypeError, ValueError) as error:
        raise ConfigError("configuration file is not valid JSON") from error
    if not isinstance(file["world_id"], str) or not isinstance(file["title"], str):
        raise ConfigError("configuration file is not valid JSON")
    if not isinstance(file["database_path"], str) or not isinstance(file["visibility"], str):
        raise ConfigError("configuration file is not valid JSON")
    if not isinstance(file["retention_seconds"], int) or isinstance(file["retention_seconds"], bool):
        raise ConfigError("configuration file is not valid JSON")
    if not isinstance(file["max_payload_bytes"], int) or isinstance(file["max_payload_bytes"], bool):
        raise ConfigError("configuration file is not valid JSON")
    return checked_config(config)


def message_error(record: object) -> str | None:
    """Return a profile error code, or None when the record is a message.

    A present but unsupported protocol_version or type is reported before
    other schema failures. A missing world is an invalid record. A present
    world string is checked against the host afterward.
    """

    if not isinstance(record, dict):
        return "invalid_record"
    version = record.get("protocol_version")
    if isinstance(version, str) and version != PROTOCOL_VERSION:
        return "unsupported_version"
    kind = record.get("type")
    if isinstance(kind, str) and kind != "message":
        return "unsupported_record_type"
    required = ("protocol_version", "type", "id", "world", "from", "to", "body")
    if any(key not in record for key in required):
        return "invalid_record"
    if version != PROTOCOL_VERSION or kind != "message":
        return "invalid_record"
    if not _nonempty_string(record.get("id")):
        return "invalid_record"
    if not _nonempty_string(record.get("world")) or not _nonempty_string(record.get("from")):
        return "invalid_record"
    recipients = record.get("to")
    if not isinstance(recipients, list) or not recipients:
        return "invalid_record"
    seen: list[str] = []
    for recipient in recipients:
        if not _nonempty_string(recipient) or recipient in seen:
            return "invalid_record"
        seen.append(recipient)
    if not isinstance(record.get("body"), dict):
        return "invalid_record"
    return None


def collaboration_error(record: object) -> str | None:
    """Return a profile error code, or None when the record is a collaboration act."""

    if not isinstance(record, dict):
        return "invalid_record"
    version = record.get("protocol_version")
    if isinstance(version, str) and version != PROTOCOL_VERSION:
        return "unsupported_version"
    kind = record.get("type")
    if isinstance(kind, str) and kind not in {
        "artifact_revision",
        "objection",
        "decline",
        "withdrawal",
    }:
        return "unsupported_record_type"
    if kind not in {"artifact_revision", "objection", "decline", "withdrawal"}:
        return "invalid_record"
    if version != PROTOCOL_VERSION:
        return "invalid_record"
    if not _nonempty_string(record.get("id")) or not _nonempty_string(record.get("world")):
        return "invalid_record"
    if not _nonempty_string(record.get("from")):
        return "invalid_record"
    if kind == "artifact_revision":
        if "revision" in record:
            return "invalid_record"
        if not _nonempty_string(record.get("artifact_id")) or not _nonempty_string(
            record.get("media_type")
        ):
            return "invalid_record"
        if not _audience(record.get("to")) or not isinstance(record.get("body"), dict):
            return "invalid_record"
        if "continuity_note" in record and not _note_shape(record.get("continuity_note")):
            return "invalid_record"
        if "derived_from" in record and not _citation_shape(record.get("derived_from")):
            return "invalid_record"
        return None
    if kind in {"objection", "decline"}:
        if not _nonempty_string(record.get("artifact_id")) or not _nonempty_string(
            record.get("target_from")
        ):
            return "invalid_record"
        if not _positive_int(record.get("revision")):
            return "invalid_record"
        if not _audience(record.get("to")) or not isinstance(record.get("body"), dict):
            return "invalid_record"
        return None
    if not _nonempty_string(record.get("artifact_id")) or not _nonempty_string(
        record.get("target_from")
    ):
        return "invalid_record"
    if not _positive_int(record.get("revision")):
        return "invalid_record"
    body = record.get("body")
    if body is not None and not isinstance(body, dict):
        return "invalid_record"
    return None


def _audience(value: object) -> bool:
    if not isinstance(value, list) or not value:
        return False
    seen: list[str] = []
    for recipient in value:
        if not _nonempty_string(recipient) or recipient in seen:
            return False
        seen.append(recipient)
    return True


def _positive_int(value: object) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and 1 <= value <= 9_007_199_254_740_991


def _note_shape(value: object) -> bool:
    if not isinstance(value, dict):
        return False
    aim = value.get("aim")
    hint = value.get("resume_hint")
    return (
        isinstance(aim, str)
        and isinstance(hint, str)
        and 1 <= len(aim) <= 1024
        and 1 <= len(hint) <= 1024
    )


def _citation_shape(value: object) -> bool:
    if not isinstance(value, dict):
        return False
    return (
        _nonempty_string(value.get("from"))
        and _nonempty_string(value.get("artifact_id"))
        and _positive_int(value.get("revision"))
    )


def _nonempty_string(value: object) -> TypeGuard[str]:
    return isinstance(value, str) and bool(value)


def unix_to_rfc3339(unix: int) -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(unix))


def _rollback(connection: sqlite3.Connection) -> None:
    if connection.in_transaction:
        connection.execute("ROLLBACK")


def _fingerprint(visibility: str, retention_seconds: int, grants: tuple[Credential, ...]) -> str:
    lines = [visibility, str(retention_seconds)]
    ordered = sorted(grants, key=lambda item: item.principal)
    for credential in ordered:
        lines.append(
            f"{credential.principal} {int(credential.read)} {int(credential.write)}"
        )
    return "\n".join(lines)


def as_object(value: object) -> JsonObject | None:
    if not isinstance(value, dict):
        return None
    parsed: JsonObject = {}
    for key, item in value.items():
        if not isinstance(key, str):
            return None
        parsed[key] = item
    return parsed


class MalformedJson(ValueError):
    pass


def _finite_float(text: str) -> float:
    value = float(text)
    if not math.isfinite(value):
        raise MalformedJson
    return value


def _bounded_int(text: str) -> int:
    value = int(text)
    # The Rust host keeps integers exact only within i64 or u64, and refuses the rest.
    if not -(2**63) <= value <= 2**64 - 1:
        raise MalformedJson
    return value


def _reject_constant(text: str) -> object:
    raise MalformedJson


def _well_formed(value: object, depth: int) -> bool:
    if isinstance(value, str):
        try:
            value.encode("utf-8")
        except UnicodeEncodeError:
            return False
        return True
    if isinstance(value, list):
        return depth <= MAX_JSON_DEPTH and all(_well_formed(item, depth + 1) for item in value)
    if isinstance(value, dict):
        return depth <= MAX_JSON_DEPTH and all(
            _well_formed(key, depth) and _well_formed(item, depth + 1) for key, item in value.items()
        )
    return True


def parse_request_json(body: bytes) -> object:
    """Parse a request body as strict UTF-8 JSON that every in-repository host accepts alike.

    Non-finite or out-of-range numbers, NaN-style literals, lone surrogates, and nesting
    deeper than the Rust host allows are malformed, so no stored event can later make a
    strict reader fail on an event page.
    """
    try:
        value: object = json.loads(
            body.decode("utf-8"),
            parse_float=_finite_float,
            parse_int=_bounded_int,
            parse_constant=_reject_constant,
        )
    except (ValueError, OverflowError, RecursionError):
        raise MalformedJson from None
    if not _well_formed(value, 1):
        raise MalformedJson
    return value


def parse_object(raw: str | bytes) -> JsonObject:
    try:
        parsed: object = json.loads(raw)
    except json.JSONDecodeError as error:
        raise StorageFailure from error
    found = as_object(parsed)
    if found is None:
        raise StorageFailure
    return found


def listen_pair(address: object) -> tuple[str, int]:
    if not isinstance(address, tuple) or len(address) < 2:
        raise StorageFailure
    host = address[0]
    port = address[1]
    if isinstance(host, bytearray):
        host = bytes(host)
    if isinstance(host, bytes):
        try:
            host = host.decode("ascii")
        except UnicodeDecodeError as error:
            raise StorageFailure from error
    if not isinstance(host, str) or isinstance(port, bool) or not isinstance(port, int):
        raise StorageFailure
    return host, port


def http_origin(address: object) -> str:
    host, port = listen_pair(address)
    if ":" in host:
        return f"http://[{host}]:{port}"
    return f"http://{host}:{port}"


def _wall_time() -> int:
    return int(time.time())


def visible_to(visibility: str, principal: str, actor: str, message: JsonObject) -> bool:
    if visibility == "members":
        return True
    if visibility == "sender_only":
        return actor == principal
    if actor == principal:
        return True
    recipients = message.get("to")
    if not isinstance(recipients, list):
        return False
    return any(recipient == principal for recipient in recipients)


class Store:
    def __init__(self, config: HostConfig) -> None:
        self.config = config
        self.clock: Callable[[], int] = _wall_time
        self._lock = threading.Lock()
        self._sync_policy()

    def _connect(self) -> sqlite3.Connection:
        connection = sqlite3.connect(self.config.database_path, timeout=5, isolation_level=None)
        connection.row_factory = sqlite3.Row
        connection.execute("PRAGMA journal_mode=WAL").fetchall()
        connection.execute("PRAGMA synchronous=FULL").fetchall()
        connection.execute("PRAGMA busy_timeout=5000")
        return connection

    def _sync_policy(self) -> None:
        with self._lock:
            connection = self._connect()
            try:
                # executescript commits, so schema setup stays outside the policy transaction.
                self._migrate(connection)
                connection.execute("BEGIN IMMEDIATE")
                stored_world = self._meta(connection, "world_id")
                if stored_world is None:
                    self._put_meta(connection, "world_id", self.config.world_id)
                elif stored_world != self.config.world_id:
                    raise ConfigError("database belongs to a different world")
                current = _fingerprint(
                    self.config.visibility,
                    self.config.retention_seconds,
                    self.config.credentials,
                )
                stored = self._meta(connection, "policy_fingerprint")
                revision_text = self._meta(connection, "policy_revision")
                revision = int(revision_text) if revision_text else 0
                if stored != current:
                    revision += 1
                    self._put_meta(connection, "policy_fingerprint", current)
                    self._put_meta(connection, "policy_revision", str(revision))
                    self._put_meta(connection, "visibility", self.config.visibility)
                    self._put_meta(
                        connection, "retention_seconds", str(self.config.retention_seconds)
                    )
                connection.execute("COMMIT")
            except ConfigError:
                _rollback(connection)
                raise
            except sqlite3.Error as error:
                _rollback(connection)
                raise StorageFailure from error
            finally:
                connection.close()

    def _migrate(self, connection: sqlite3.Connection) -> None:
        connection.executescript(
            """
            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS events (
                sequence INTEGER PRIMARY KEY,
                event_id TEXT NOT NULL UNIQUE,
                actor TEXT NOT NULL,
                event_json TEXT NOT NULL,
                message_json TEXT NOT NULL,
                created_unix INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS retries (
                principal TEXT NOT NULL,
                message_id TEXT NOT NULL,
                request_bytes BLOB NOT NULL,
                receipt_json TEXT NOT NULL,
                created_unix INTEGER NOT NULL,
                PRIMARY KEY (principal, message_id)
            );
            CREATE TABLE IF NOT EXISTS cursors (
                token TEXT PRIMARY KEY,
                principal TEXT NOT NULL,
                last_sequence INTEGER NOT NULL,
                revision INTEGER NOT NULL
            );
            """
        )

    def submit(self, principal: str, body: bytes) -> JsonObject:
        with self._lock:
            connection = self._connect()
            try:
                connection.execute("BEGIN IMMEDIATE")
                receipt = self._submit(connection, principal, body)
                connection.execute("COMMIT")
                return receipt
            except (Conflict, StorageFailure):
                _rollback(connection)
                raise
            except (sqlite3.Error, ValueError) as error:
                _rollback(connection)
                raise StorageFailure from error
            finally:
                connection.close()

    def _submit(self, connection: sqlite3.Connection, principal: str, body: bytes) -> JsonObject:
        message = parse_object(body)
        message_id = message.get("id")
        if not _nonempty_string(message_id):
            raise StorageFailure
        now = self.clock()
        retention = int(self._required_meta(connection, "retention_seconds"))
        saved = connection.execute(
            """
            SELECT request_bytes, receipt_json, created_unix
            FROM retries WHERE principal = ? AND message_id = ?
            """,
            (principal, message_id),
        ).fetchone()
        if saved is not None:
            # Half-open window: [created, created + retention).
            if saved["created_unix"] + retention > now:
                if bytes(saved["request_bytes"]) == body:
                    return parse_object(str(saved["receipt_json"]))
                raise Conflict
            connection.execute(
                "DELETE FROM retries WHERE principal = ? AND message_id = ?",
                (principal, message_id),
            )
        sequence_row = connection.execute(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM events"
        ).fetchone()
        if sequence_row is None:
            raise StorageFailure
        sequence = int(sequence_row[0])
        event_id = "event:" + secrets.token_hex(16)
        try:
            timestamp = unix_to_rfc3339(now)
        except (OverflowError, OSError, ValueError) as error:
            raise StorageFailure from error
        event: JsonObject = {
            "protocol_version": PROTOCOL_VERSION,
            "type": "event",
            "id": event_id,
            "world": self.config.world_id,
            "sequence": sequence,
            "timestamp": timestamp,
            "kind": "message.recorded",
            "actor": principal,
            "body": {"message": message},
        }
        receipt: JsonObject = {
            "protocol_version": PROTOCOL_VERSION,
            "type": "receipt",
            "world": self.config.world_id,
            "record_id": message_id,
            "event_id": event_id,
            "sequence": sequence,
            "status": "recorded",
        }
        connection.execute(
            """
            INSERT INTO events (sequence, event_id, actor, event_json, message_json, created_unix)
            VALUES (?, ?, ?, ?, ?, ?)
            """,
            (
                sequence,
                event_id,
                principal,
                json.dumps(event, ensure_ascii=False, separators=(",", ":"), allow_nan=False),
                json.dumps(message, ensure_ascii=False, separators=(",", ":"), allow_nan=False),
                now,
            ),
        )
        connection.execute(
            """
            INSERT INTO retries (principal, message_id, request_bytes, receipt_json, created_unix)
            VALUES (?, ?, ?, ?, ?)
            """,
            (
                principal,
                message_id,
                body,
                json.dumps(receipt, ensure_ascii=False, separators=(",", ":"), allow_nan=False),
                now,
            ),
        )
        return receipt

    def collaborate(self, principal: str, body: bytes, *, can_read: bool = True) -> JsonObject:
        """Without a read grant a principal sees only its own records, as in an event read."""
        with self._lock:
            connection = self._connect()
            try:
                connection.execute("BEGIN IMMEDIATE")
                receipt = self._collaborate(connection, principal, body, can_read)
                connection.execute("COMMIT")
                return receipt
            except (Conflict, UnknownTarget, NotAuthor, StorageFailure):
                _rollback(connection)
                raise
            except (sqlite3.Error, ValueError) as error:
                _rollback(connection)
                raise StorageFailure from error
            finally:
                connection.close()

    def _collaborate(
        self, connection: sqlite3.Connection, principal: str, body: bytes, can_read: bool
    ) -> JsonObject:
        record = parse_object(body)
        record_id = record.get("id")
        if not _nonempty_string(record_id):
            raise StorageFailure
        now = self.clock()
        retention = int(self._required_meta(connection, "retention_seconds"))
        saved = connection.execute(
            """
            SELECT request_bytes, receipt_json, created_unix
            FROM retries WHERE principal = ? AND message_id = ?
            """,
            (principal, record_id),
        ).fetchone()
        if (
            saved is not None
            and saved["created_unix"] + retention > now
            and bytes(saved["request_bytes"]) == body
        ):
            return parse_object(str(saved["receipt_json"]))
        self._ensure_collaboration_target(connection, principal, can_read, record)
        if saved is not None and saved["created_unix"] + retention > now:
            raise Conflict
        if saved is not None:
            connection.execute(
                "DELETE FROM retries WHERE principal = ? AND message_id = ?",
                (principal, record_id),
            )
        kind = record.get("type")
        receipt: JsonObject
        if kind == "artifact_revision":
            citation = as_object(record.get("derived_from"))
            if citation is not None:
                self._require_visible_revision(connection, principal, can_read, citation, cited_by="from")
            artifact_id = record.get("artifact_id")
            if not _nonempty_string(artifact_id):
                raise StorageFailure
            stored = dict(record)
            stored["revision"] = self._next_revision(connection, principal, artifact_id)
            receipt = self._insert_collaboration(
                connection, principal, stored, "artifact.recorded", "artifact_revision", now
            )
            receipt["artifact_id"] = stored["artifact_id"]
            receipt["revision"] = stored["revision"]
        elif isinstance(kind, str) and kind in {"objection", "decline"}:
            self._require_visible_revision(connection, principal, can_read, record, cited_by="target_from")
            recorded = "objection.recorded" if kind == "objection" else "decline.recorded"
            receipt = self._insert_collaboration(
                connection, principal, record, recorded, kind, now
            )
        elif kind == "withdrawal":
            row = self._require_visible_revision(connection, principal, can_read, record, cited_by="target_from"
            )
            if row["actor"] != principal:
                raise NotAuthor
            event = parse_object(str(row["event_json"]))
            event["kind"] = "artifact.withdrawn"
            event["body"] = {}
            connection.execute(
                "UPDATE events SET event_json = ? WHERE sequence = ?",
                (json.dumps(event, ensure_ascii=False, separators=(",", ":"), allow_nan=False), int(row["sequence"])),
            )
            receipt = {
                "protocol_version": PROTOCOL_VERSION,
                "type": "receipt",
                "world": self.config.world_id,
                "record_id": record_id,
                "event_id": row["event_id"],
                "sequence": int(row["sequence"]),
                "status": "recorded",
            }
        else:
            raise StorageFailure
        connection.execute(
            """
            INSERT INTO retries (principal, message_id, request_bytes, receipt_json, created_unix)
            VALUES (?, ?, ?, ?, ?)
            """,
            (
                principal,
                record_id,
                body,
                json.dumps(receipt, ensure_ascii=False, separators=(",", ":"), allow_nan=False),
                now,
            ),
        )
        return receipt

    def _ensure_collaboration_target(
        self, connection: sqlite3.Connection, principal: str, can_read: bool, record: JsonObject
    ) -> None:
        kind = record.get("type")
        if kind == "artifact_revision":
            citation = as_object(record.get("derived_from"))
            if citation is not None:
                self._require_visible_revision(connection, principal, can_read, citation, cited_by="from")
            return
        if kind in {"objection", "decline"}:
            self._require_visible_revision(connection, principal, can_read, record, cited_by="target_from")
            return
        if kind == "withdrawal":
            row = self._require_visible_revision(connection, principal, can_read, record, cited_by="target_from"
            )
            if row["actor"] != principal:
                raise NotAuthor
            return
        raise StorageFailure

    def _artifact_rows(self, connection: sqlite3.Connection) -> list[sqlite3.Row]:
        return connection.execute(
            """
            SELECT sequence, event_id, actor, event_json, message_json
            FROM events ORDER BY sequence ASC
            """
        ).fetchall()

    def _next_revision(self, connection: sqlite3.Connection, principal: str, artifact_id: str) -> int:
        maximum = 0
        for row in self._artifact_rows(connection):
            event = parse_object(str(row["event_json"]))
            record = parse_object(str(row["message_json"]))
            if event.get("kind") not in {"artifact.recorded", "artifact.withdrawn"}:
                continue
            if record.get("from") == principal and record.get("artifact_id") == artifact_id:
                revision = record.get("revision")
                if isinstance(revision, int) and not isinstance(revision, bool):
                    maximum = max(maximum, revision)
        return maximum + 1

    def _require_visible_revision(
        self,
        connection: sqlite3.Connection,
        principal: str,
        can_read: bool,
        citation: JsonObject,
        cited_by: str,
    ) -> sqlite3.Row:
        target_from = citation.get(cited_by)
        artifact_id = citation.get("artifact_id")
        revision = citation.get("revision")
        visibility = self._required_meta(connection, "visibility")
        for row in self._artifact_rows(connection):
            event = parse_object(str(row["event_json"]))
            record = parse_object(str(row["message_json"]))
            if event.get("kind") not in {"artifact.recorded", "artifact.withdrawn"}:
                continue
            if (
                record.get("from") != target_from
                or record.get("artifact_id") != artifact_id
                or record.get("revision") != revision
            ):
                continue
            actor = row["actor"]
            if not isinstance(actor, str):
                continue
            if (visible_to(visibility, principal, actor, record) if can_read else actor == principal):
                return row
        raise UnknownTarget

    def _insert_collaboration(
        self,
        connection: sqlite3.Connection,
        principal: str,
        record: JsonObject,
        kind: str,
        body_key: str,
        now: int,
    ) -> JsonObject:
        sequence_row = connection.execute(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM events"
        ).fetchone()
        if sequence_row is None:
            raise StorageFailure
        sequence = int(sequence_row[0])
        event_id = "event:" + secrets.token_hex(16)
        try:
            timestamp = unix_to_rfc3339(now)
        except (OverflowError, OSError, ValueError) as error:
            raise StorageFailure from error
        event: JsonObject = {
            "protocol_version": PROTOCOL_VERSION,
            "type": "event",
            "id": event_id,
            "world": self.config.world_id,
            "sequence": sequence,
            "timestamp": timestamp,
            "kind": kind,
            "actor": principal,
            "body": {body_key: record},
        }
        receipt: JsonObject = {
            "protocol_version": PROTOCOL_VERSION,
            "type": "receipt",
            "world": self.config.world_id,
            "record_id": record["id"],
            "event_id": event_id,
            "sequence": sequence,
            "status": "recorded",
        }
        connection.execute(
            """
            INSERT INTO events (sequence, event_id, actor, event_json, message_json, created_unix)
            VALUES (?, ?, ?, ?, ?, ?)
            """,
            (
                sequence,
                event_id,
                principal,
                json.dumps(event, ensure_ascii=False, separators=(",", ":"), allow_nan=False),
                json.dumps(record, ensure_ascii=False, separators=(",", ":"), allow_nan=False),
                now,
            ),
        )
        return receipt

    def read_page(self, principal: str, after: str | None, can_read: bool) -> JsonObject:
        if not can_read:
            raise ReadForbidden
        with self._lock:
            connection = self._connect()
            try:
                connection.execute("BEGIN IMMEDIATE")
                page = self._read_page(connection, principal, after)
                connection.execute("COMMIT")
                return page
            except (InvalidCursor, ReadForbidden, CursorExpired, StorageFailure):
                _rollback(connection)
                raise
            except sqlite3.Error as error:
                _rollback(connection)
                raise StorageFailure from error
            finally:
                connection.close()

    def _read_page(self, connection: sqlite3.Connection, principal: str, after: str | None) -> JsonObject:
        visibility = self._required_meta(connection, "visibility")
        revision = int(self._required_meta(connection, "policy_revision"))
        start = 0
        if after is not None:
            if not after:
                raise InvalidCursor
            row = connection.execute(
                "SELECT principal, last_sequence, revision FROM cursors WHERE token = ?",
                (after,),
            ).fetchone()
            if row is None:
                raise InvalidCursor
            if row["principal"] != principal:
                raise ReadForbidden("Cursor belongs to another principal")
            if int(row["revision"]) != revision:
                raise CursorExpired
            start = int(row["last_sequence"])
        rows = connection.execute(
            """
            SELECT sequence, actor, event_json, message_json
            FROM events WHERE sequence > ? ORDER BY sequence ASC
            """,
            (start,),
        ).fetchall()
        visible: list[JsonObject] = []
        scanned_through = start
        last_included = start
        has_more = False
        for row in rows:
            sequence = int(row["sequence"])
            scanned_through = sequence
            message = parse_object(str(row["message_json"]))
            event = parse_object(str(row["event_json"]))
            actor = row["actor"]
            if not isinstance(actor, str):
                raise StorageFailure
            if not visible_to(visibility, principal, actor, message):
                continue
            if len(visible) == PAGE_LIMIT:
                has_more = True
                break
            visible.append(event)
            last_included = sequence
        last_sequence = last_included if has_more else scanned_through
        token = secrets.token_urlsafe(24)
        connection.execute(
            """
            INSERT INTO cursors (token, principal, last_sequence, revision)
            VALUES (?, ?, ?, ?)
            """,
            (token, principal, last_sequence, revision),
        )
        return {
            "protocol_version": PROTOCOL_VERSION,
            "type": "event_page",
            "world": self.config.world_id,
            "events": visible,
            "next_cursor": token,
            "has_more": has_more,
        }

    def event_count(self) -> int:
        with self._lock:
            connection = self._connect()
            try:
                row = connection.execute("SELECT COUNT(*) FROM events").fetchone()
                return int(row[0])
            finally:
                connection.close()

    def _meta(self, connection: sqlite3.Connection, key: str) -> str | None:
        row = connection.execute("SELECT value FROM meta WHERE key = ?", (key,)).fetchone()
        if row is None:
            return None
        return str(row["value"])

    def _required_meta(self, connection: sqlite3.Connection, key: str) -> str:
        value = self._meta(connection, key)
        if value is None:
            raise StorageFailure
        return value

    def _put_meta(self, connection: sqlite3.Connection, key: str, value: str) -> None:
        connection.execute(
            "INSERT INTO meta (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            (key, value),
        )


def tokens_equal(left: str, right: str) -> bool:
    """Compare digests so neither the content nor the length of a token affects timing."""
    left_digest = hashlib.sha256(left.encode("utf-8", "surrogatepass")).digest()
    right_digest = hashlib.sha256(right.encode("utf-8", "surrogatepass")).digest()
    return hmac.compare_digest(left_digest, right_digest)


def visible_header(value: str) -> bool:
    return all(character == "\t" or " " <= character <= "~" for character in value)


def authenticate(header: str | None, credentials: tuple[Credential, ...]) -> Credential | None:
    if not header or not visible_header(header):
        return None
    scheme, separator, token = header.partition(" ")
    if not separator or scheme.lower() != "bearer":
        return None
    token = token.strip()
    if not token:
        return None
    for credential in credentials:
        if tokens_equal(credential.token, token):
            return credential
    return None


def json_content_type(value: str | None) -> bool:
    if not value:
        return False
    media = value.split(";", 1)[0].strip()
    return media.lower() == "application/json"


def problem(status: int, code: str, title: str) -> JsonObject:
    detail: JsonObject = {
        "type": f"https://agentciv.io/problems/{code}",
        "title": title,
        "status": status,
        "code": code,
    }
    return detail


class CommonsServer(ThreadingHTTPServer):
    # Closing the host must drain handlers before callers remove or reopen its database.
    daemon_threads = False
    allow_reuse_address = True
    config: HostConfig
    store: Store
    origin: str
    serve_thread: threading.Thread

    def server_bind(self) -> None:
        # HTTPServer.server_bind reverse-resolves the listen address before listen().
        # That lookup can block on a macOS runner until a readiness check has given up.
        socketserver.TCPServer.server_bind(self)
        bound_host, bound_port = listen_pair(self.server_address)
        self.server_name = bound_host
        self.server_port = bound_port

    def __init__(self, config: HostConfig, store: Store) -> None:
        if ":" in config.listen[0]:
            self.address_family = socket.AF_INET6
        super().__init__(config.listen, Handler)
        self.config = config
        self.store = store
        self.origin = http_origin(self.server_address)

    def descriptor(self) -> JsonObject:
        described: JsonObject = {
            "protocol_version": PROTOCOL_VERSION,
            "profile": PROFILE,
            "type": "world",
            "id": self.config.world_id,
            "title": self.config.title,
            "capabilities": ["events.read", "messages.submit", "collaboration.submit"],
            "endpoints": {
                "events": f"{self.origin}/events",
                "submit": f"{self.origin}/submit",
                "collaborate": f"{self.origin}/collaborate",
            },
            "history": {
                "visibility": self.config.visibility,
                "retention_seconds": self.config.retention_seconds,
            },
            "authentication": {
                "events": "bearer",
                "submit": "bearer",
                "collaborate": "bearer",
            },
            "limits": {"max_payload_bytes": self.config.max_payload_bytes},
        }
        return described


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    server_version = "agentciv-http-commons-python"
    sys_version = ""

    def version_string(self) -> str:
        return self.server_version

    def _commons(self) -> CommonsServer:
        if not isinstance(self.server, CommonsServer):
            raise StorageFailure
        return self.server

    def setup(self) -> None:
        super().setup()
        self.connection.settimeout(10)

    def log_message(self, fmt: str, *args: object) -> None:
        return

    def do_GET(self) -> None:
        self.close_connection = True
        path = urllib.parse.urlsplit(self.path)
        server = self._commons()
        if path.path == "/.well-known/agentciv":
            self._json(200, server.descriptor())
            return
        if path.path == "/events":
            self._events(path.query)
            return
        self._problem(404, "not_found", "Not found")

    def do_POST(self) -> None:
        self.close_connection = True
        path = urllib.parse.urlsplit(self.path)
        if path.path == "/submit":
            self._submit()
            return
        if path.path == "/collaborate":
            self._collaborate()
            return
        self._discard_body()
        self._problem(404, "not_found", "Not found")

    def _events(self, query: str) -> None:
        server = self._commons()
        credential = authenticate(self.headers.get("Authorization"), server.config.credentials)
        if credential is None:
            self._problem(401, "authentication_required", "Authentication required")
            return
        params = urllib.parse.parse_qs(query, keep_blank_values=True)
        values = params.get("after")
        # A repeated `after` is an invalid cursor, reported after the read grant check.
        after = None if values is None else values[0] if len(values) == 1 else ""
        try:
            page = server.store.read_page(credential.principal, after, credential.read)
        except ReadForbidden as error:
            self._problem(403, "forbidden", error.title)
            return
        except InvalidCursor:
            self._problem(400, "invalid_cursor", "Cursor is malformed")
            return
        except CursorExpired:
            self._problem(410, "cursor_expired", "Cursor can no longer resume")
            return
        except StorageFailure:
            self._problem(500, "storage_failed", "Events could not be read")
            return
        self._json(200, page)

    def _submit(self) -> None:
        server = self._commons()
        credential = authenticate(self.headers.get("Authorization"), server.config.credentials)
        if credential is None:
            self._discard_body()
            self._problem(401, "authentication_required", "Authentication required")
            return
        if not credential.write:
            self._discard_body()
            self._problem(403, "forbidden", "Write access is required")
            return
        body, too_large = self._read_limited(server.config.max_payload_bytes)
        if too_large or body is None:
            self._problem(413, "payload_too_large", "Body exceeds the published limit")
            return
        if not json_content_type(self.headers.get("Content-Type")):
            self._problem(415, "unsupported_media_type", "JSON is required")
            return
        try:
            record = parse_request_json(body)
        except MalformedJson:
            self._problem(400, "malformed_json", "JSON could not be parsed")
            return
        code = message_error(record)
        if code == "unsupported_version":
            self._problem(422, code, "Unsupported protocol version")
            return
        if code == "unsupported_record_type":
            self._problem(422, code, "Unsupported record type")
            return
        if code == "invalid_record":
            self._problem(422, code, "Invalid record")
            return
        if isinstance(record, dict):
            world = record.get("world")
            if isinstance(world, str) and world != server.config.world_id:
                self._problem(422, "wrong_world", "Message world does not match this host")
                return
            if record.get("from") != credential.principal:
                self._problem(403, "forbidden", "Message sender does not match the credential")
                return
        try:
            receipt = server.store.submit(credential.principal, body)
        except Conflict:
            self._problem(409, "id_conflict", "Message id was already used for different bytes")
            return
        except StorageFailure:
            self._problem(500, "storage_failed", "The message was not recorded")
            return
        self._json(200, receipt)

    def _collaborate(self) -> None:
        server = self._commons()
        credential = authenticate(self.headers.get("Authorization"), server.config.credentials)
        if credential is None:
            self._discard_body()
            self._problem(401, "authentication_required", "Authentication required")
            return
        if not credential.write:
            self._discard_body()
            self._problem(403, "forbidden", "Write access is required")
            return
        body, too_large = self._read_limited(server.config.max_payload_bytes)
        if too_large or body is None:
            self._problem(413, "payload_too_large", "Body exceeds the published limit")
            return
        if not json_content_type(self.headers.get("Content-Type")):
            self._problem(415, "unsupported_media_type", "JSON is required")
            return
        try:
            record = parse_request_json(body)
        except MalformedJson:
            self._problem(400, "malformed_json", "JSON could not be parsed")
            return
        code = collaboration_error(record)
        if code == "unsupported_version":
            self._problem(422, code, "Unsupported protocol version")
            return
        if code == "unsupported_record_type":
            self._problem(422, code, "Unsupported record type")
            return
        if code == "invalid_record":
            self._problem(422, code, "Invalid record")
            return
        if isinstance(record, dict):
            world = record.get("world")
            if isinstance(world, str) and world != server.config.world_id:
                self._problem(422, "wrong_world", "Record world does not match this host")
                return
            if record.get("from") != credential.principal:
                self._problem(403, "forbidden", "Record sender does not match the credential")
                return
        try:
            receipt = server.store.collaborate(credential.principal, body, can_read=credential.read)
        except Conflict:
            self._problem(409, "id_conflict", "Record id was already used for different bytes")
            return
        except UnknownTarget:
            self._problem(422, "unknown_target", "Target revision is not available")
            return
        except NotAuthor:
            self._problem(403, "forbidden", "Only the author can withdraw this revision")
            return
        except StorageFailure:
            self._problem(500, "storage_failed", "The record was not recorded")
            return
        self._json(200, receipt)

    def _read_limited(self, limit: int) -> tuple[bytes | None, bool]:
        raw_length = self.headers.get("Content-Length")
        transfer = self.headers.get("Transfer-Encoding")
        if transfer is not None:
            if raw_length is not None or transfer.strip().lower() != "chunked":
                return b"", False
            return self._read_chunked(limit)
        if raw_length is None:
            # A request without Content-Length or Transfer-Encoding has no body.
            return b"", False
        try:
            length = int(raw_length)
        except ValueError:
            return b"", False
        if length < 0:
            return b"", False
        if length > limit:
            self._discard(min(length, 2_000_000))
            return None, True
        return self.rfile.read(length), False

    def _read_chunked(self, limit: int) -> tuple[bytes | None, bool]:
        data = bytearray()
        while True:
            line = self.rfile.readline(MAX_CHUNK_LINE + 1)
            if len(line) > MAX_CHUNK_LINE or not line.endswith(b"\r\n"):
                return b"", False
            size_text = line[:-2].split(b";", 1)[0].strip()
            if not size_text or any(byte not in b"0123456789abcdefABCDEF" for byte in size_text):
                return b"", False
            size = int(size_text, 16)
            if size == 0:
                for _ in range(64):
                    trailer = self.rfile.readline(MAX_CHUNK_LINE + 1)
                    if trailer == b"\r\n":
                        return bytes(data), False
                    if not trailer or len(trailer) > MAX_CHUNK_LINE:
                        return b"", False
                return b"", False
            if len(data) + size > limit:
                return None, True
            chunk = self.rfile.read(size)
            if len(chunk) != size or self.rfile.read(2) != b"\r\n":
                return b"", False
            data.extend(chunk)

    def _discard_body(self) -> None:
        raw_length = self.headers.get("Content-Length")
        if raw_length is None:
            return
        try:
            length = int(raw_length)
        except ValueError:
            return
        if 0 < length <= 2_000_000:
            self._discard(length)

    def _discard(self, length: int) -> None:
        remaining = length
        while remaining > 0:
            chunk = self.rfile.read(min(65536, remaining))
            if not chunk:
                break
            remaining -= len(chunk)

    def _json(self, status: int, body: JsonObject) -> None:
        self._bytes(status, json.dumps(body, ensure_ascii=False, allow_nan=False).encode("utf-8"), "application/json")

    def _problem(self, status: int, code: str, title: str) -> None:
        extra = None
        if status == 401:
            extra = [("WWW-Authenticate", 'Bearer realm="agentciv"')]
        self._bytes(
            status,
            json.dumps(problem(status, code, title), allow_nan=False).encode("utf-8"),
            "application/problem+json",
            extra,
        )

    def _bytes(
        self,
        status: int,
        payload: bytes,
        content_type: str,
        extra: list[tuple[str, str]] | None = None,
    ) -> None:
        self.send_response(status, REASONS.get(status, "Error"))
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(payload)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("Connection", "close")
        if extra:
            for name, value in extra:
                self.send_header(name, value)
        self.end_headers()
        self.wfile.write(payload)


def start_server(config: HostConfig) -> CommonsServer:
    store = Store(checked_config(config))
    server = CommonsServer(config, store)
    thread = threading.Thread(target=server.serve_forever, name="agentciv-python-host", daemon=True)
    thread.start()
    server.serve_thread = thread
    deadline = time.monotonic() + 2
    while time.monotonic() < deadline:
        try:
            host, port = listen_pair(server.server_address)
            with socket.create_connection((host, port), timeout=0.2):
                return server
        except OSError:
            time.sleep(0.01)
    server.shutdown()
    server.server_close()
    raise StorageFailure


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Serve one loopback HTTP Commons world")
    parser.add_argument("--config", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        config = load_config(args.config)
        server = start_server(config)
    except ConfigError as error:
        print(str(error), file=sys.stderr)
        return 2
    except StorageFailure:
        print("storage could not be opened", file=sys.stderr)
        return 1
    print(f"discovery {server.origin}/.well-known/agentciv", flush=True)
    try:
        server.serve_thread.join()
    except KeyboardInterrupt:
        server.shutdown()
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
