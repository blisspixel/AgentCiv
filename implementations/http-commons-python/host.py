"""Loopback HTTP Commons host.

This process implements the draft profile in PROTOCOL.md for one world.
It does not complete that profile claim, and a passing public report from
this process does not show interoperability.
"""

from __future__ import annotations

import argparse
import hmac
import ipaddress
import json
import secrets
import socket
import sqlite3
import sys
import threading
import time
import urllib.parse
from dataclasses import dataclass
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

PAGE_LIMIT = 100
MAX_PAYLOAD_CAP = 8 * 1024 * 1024
PROTOCOL_VERSION = "0.1-draft"
PROFILE = "http-commons/0.1-draft"

REASONS = {
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


def _nonempty_string(value: object) -> bool:
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


def visible_to(visibility: str, principal: str, actor: str, message: dict) -> bool:
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
        self.clock = lambda: int(time.time())
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

    def submit(self, principal: str, body: bytes) -> dict:
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
            except sqlite3.Error as error:
                _rollback(connection)
                raise StorageFailure from error
            finally:
                connection.close()

    def _submit(self, connection: sqlite3.Connection, principal: str, body: bytes) -> dict:
        try:
            message = json.loads(body)
        except json.JSONDecodeError as error:
            raise StorageFailure from error
        if not isinstance(message, dict) or not _nonempty_string(message.get("id")):
            raise StorageFailure
        message_id = message["id"]
        now = int(self.clock())
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
                    receipt = json.loads(saved["receipt_json"])
                    if not isinstance(receipt, dict):
                        raise StorageFailure
                    return receipt
                raise Conflict
            connection.execute(
                "DELETE FROM retries WHERE principal = ? AND message_id = ?",
                (principal, message_id),
            )
        sequence_row = connection.execute("SELECT COALESCE(MAX(sequence), 0) + 1 FROM events").fetchone()
        sequence = int(sequence_row[0])
        event_id = "event:" + secrets.token_hex(16)
        try:
            timestamp = unix_to_rfc3339(now)
        except (OverflowError, OSError, ValueError) as error:
            raise StorageFailure from error
        event = {
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
        receipt = {
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
                json.dumps(event, ensure_ascii=False, separators=(",", ":")),
                json.dumps(message, ensure_ascii=False, separators=(",", ":")),
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
                json.dumps(receipt, ensure_ascii=False, separators=(",", ":")),
                now,
            ),
        )
        return receipt

    def read_page(self, principal: str, after: str | None, can_read: bool) -> dict:
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

    def _read_page(self, connection: sqlite3.Connection, principal: str, after: str | None) -> dict:
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
        visible: list[dict] = []
        scanned_through = start
        last_included = start
        has_more = False
        for row in rows:
            sequence = int(row["sequence"])
            scanned_through = sequence
            try:
                message = json.loads(row["message_json"])
                event = json.loads(row["event_json"])
            except json.JSONDecodeError as error:
                raise StorageFailure from error
            if not isinstance(message, dict) or not isinstance(event, dict):
                raise StorageFailure
            if not visible_to(visibility, principal, row["actor"], message):
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
    if len(left) != len(right):
        return False
    return hmac.compare_digest(left, right)


def authenticate(header: str | None, credentials: tuple[Credential, ...]) -> Credential | None:
    if not header:
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


def problem(status: int, code: str, title: str) -> dict:
    return {
        "type": f"https://agentciv.io/problems/{code}",
        "title": title,
        "status": status,
        "code": code,
    }


class CommonsServer(ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = True

    def __init__(self, config: HostConfig, store: Store) -> None:
        super().__init__(config.listen, Handler)
        self.config = config
        self.store = store
        host, port = self.server_address[:2]
        if ":" in str(host):
            self.origin = f"http://[{host}]:{port}"
        else:
            self.origin = f"http://{host}:{port}"

    def descriptor(self) -> dict:
        return {
            "protocol_version": PROTOCOL_VERSION,
            "profile": PROFILE,
            "type": "world",
            "id": self.config.world_id,
            "title": self.config.title,
            "capabilities": ["events.read", "messages.submit"],
            "endpoints": {
                "events": f"{self.origin}/events",
                "submit": f"{self.origin}/submit",
            },
            "history": {
                "visibility": self.config.visibility,
                "retention_seconds": self.config.retention_seconds,
            },
            "authentication": {"events": "bearer", "submit": "bearer"},
            "limits": {"max_payload_bytes": self.config.max_payload_bytes},
        }


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    server_version = "agentciv-http-commons-python"
    sys_version = ""

    def version_string(self) -> str:
        return self.server_version

    def setup(self) -> None:
        super().setup()
        self.connection.settimeout(10)

    def log_message(self, fmt: str, *args: object) -> None:
        return

    def do_GET(self) -> None:
        self.close_connection = True
        path = urllib.parse.urlsplit(self.path)
        server: CommonsServer = self.server  # type: ignore[assignment]
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
        if path.path != "/submit":
            self._discard_body()
            self._problem(404, "not_found", "Not found")
            return
        self._submit()

    def _events(self, query: str) -> None:
        server: CommonsServer = self.server  # type: ignore[assignment]
        credential = authenticate(self.headers.get("Authorization"), server.config.credentials)
        if credential is None:
            self._problem(401, "authentication_required", "Authentication required")
            return
        params = urllib.parse.parse_qs(query, keep_blank_values=True)
        after = params["after"][0] if "after" in params else None
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
        server: CommonsServer = self.server  # type: ignore[assignment]
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
            record = json.loads(body)
        except json.JSONDecodeError:
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

    def _read_limited(self, limit: int) -> tuple[bytes | None, bool]:
        raw_length = self.headers.get("Content-Length")
        if raw_length is None:
            data = self.rfile.read(limit + 1)
            return (None, True) if len(data) > limit else (data, False)
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

    def _json(self, status: int, body: dict) -> None:
        self._bytes(status, json.dumps(body, ensure_ascii=False).encode("utf-8"), "application/json")

    def _problem(self, status: int, code: str, title: str) -> None:
        extra = None
        if status == 401:
            extra = [("WWW-Authenticate", 'Bearer realm="agentciv"')]
        self._bytes(
            status,
            json.dumps(problem(status, code, title)).encode("utf-8"),
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
    server.serve_thread = thread  # type: ignore[attr-defined]
    deadline = time.monotonic() + 2
    while time.monotonic() < deadline:
        try:
            with socket.create_connection(server.server_address[:2], timeout=0.2):
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
        server.serve_thread.join()  # type: ignore[attr-defined]
    except KeyboardInterrupt:
        server.shutdown()
        server.server_close()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
