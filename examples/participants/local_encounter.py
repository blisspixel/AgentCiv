"""Manual owner-private loopback room using the existing host and reader.

No scheduling, participant generation, model calls, downloads, or public service.
"""

from __future__ import annotations

import argparse
import json
import os
import secrets
import stat
import subprocess
import sys
import threading
import time
from dataclasses import dataclass
from pathlib import Path

import encounter_private as private

JsonObject = dict[str, object]
ROOT = Path(__file__).resolve().parents[2]
PAYLOAD_BYTES = 16384
CONFIG_BYTES = 65536
CLIENT_FORMAT = "agentciv-local-encounter-client/0.1-example"
CACHE_PERMISSION = "operator_authorized_local_view_only_no_public_export"


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


def initialize(state: Path, world: str, writers: list[str], readers: list[str], port: int = 8787) -> JsonObject:
    principals = writers + readers
    if (not _identifier(world) or not writers or len(principals) > 16
        or any(not _identifier(item) for item in principals) or len(set(principals)) != len(principals)
        or type(port) is not int or not 1 <= port <= 65535):
        raise private.EncounterError("configuration_invalid")
    private.create_directory(state)
    # Elevated Windows processes can otherwise create SQLite's main file with
    # Administrators as owner. Provision this new empty file with the exact
    # operator SID before either host writes it; SQLite initializes it in place.
    private.write_new(state / "world.sqlite", b"")
    _provision_sidecars(state)
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
            "cache_directory": str(cache), "private_cache_permission": CACHE_PERMISSION}))
        writable = principal in writers
        credentials.append({"principal": principal, "token": token, "read": True, "write": writable})
        clients.append({"principal": principal, "config": str(config), "write": writable})
    private.write_new(state / "host.json", _bytes({"world_id": world, "title": "Manual local encounter",
        "database_path": str(state / "world.sqlite"), "listen": f"127.0.0.1:{port}",
        "visibility": "addressed", "retention_seconds": 86400, "max_payload_bytes": PAYLOAD_BYTES,
        "credentials": credentials}))
    return {"outcome": "initialized", "world": world, "origin": origin, "clients": clients,
        "visibility": "addressed", "retention_seconds": 86400, "payload_bytes": PAYLOAD_BYTES,
        "copying_permission": "not_granted", "external_spend_usd": 0}


def _configuration(state: Path) -> JsonObject:
    private.check_directory(state)
    config = _json(private.read_bounded(state / "host.json", CONFIG_BYTES))
    if (set(config) != {"world_id", "title", "database_path", "listen", "visibility", "retention_seconds",
                       "max_payload_bytes", "credentials"}
        or not _identifier(config.get("world_id")) or not _identifier(config.get("title"))
        or config.get("database_path") != str(state / "world.sqlite") or config.get("visibility") != "addressed"
        or type(config.get("retention_seconds")) is not int or config["retention_seconds"] != 86400
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
                        private.check_directory(path)
                    pending.append(path)
                else:
                    if not stat.S_ISREG(info.st_mode):
                        raise private.EncounterError("private_file_invalid")
                    if verify:
                        private.check_file(path)
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
          seconds: int = 3600, max_storage_bytes: int = 67108864) -> JsonObject:
    if (host not in ("rust", "python") or type(seconds) is not int or not 1 <= seconds <= 86400
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
        if host == "rust":
            found = host_binary if host_binary is not None else private.find_executable("agentciv-host")
            binary = None if found is None else str(found)
            if binary is None:
                raise private.EncounterError("host_binary_unavailable")
            argv = [binary, "--config", str(state / "host.json")]
        else:
            argv = [sys.executable, str(ROOT / "implementations" / "http-commons-python" / "host.py"),
                    "--config", str(state / "host.json")]
        origin = "http://" + str(config["listen"])
        owned = _start(argv, origin)
        try:
            _wait_ready(owned)
            print(json.dumps({"outcome": "serving", "world": config["world_id"], "origin": origin,
                "session_seconds": seconds, "storage_soft_cutoff_bytes": max_storage_bytes}), flush=True)
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
    init = commands.add_parser("init")
    init.add_argument("--state", required=True, type=Path)
    init.add_argument("--world", required=True)
    init.add_argument("--writer", required=True, action="append")
    init.add_argument("--reader", action="append", default=[])
    init.add_argument("--port", type=int, default=8787)
    hosting = commands.add_parser("serve")
    hosting.add_argument("--state", required=True, type=Path)
    hosting.add_argument("--host", choices=("rust", "python"), default="rust")
    hosting.add_argument("--host-binary", type=Path)
    hosting.add_argument("--seconds", type=int, default=3600)
    hosting.add_argument("--max-storage-bytes", type=int, default=67108864)
    reading = commands.add_parser("read")
    reading.add_argument("--config", required=True, type=Path)
    reading.add_argument("--save", type=Path)
    reading.add_argument("--reader-binary", type=Path)
    submission = commands.add_parser("submit")
    submission.add_argument("--config", required=True, type=Path)
    submission.add_argument("--record", required=True, type=Path)
    submission.add_argument("--basis", type=Path)
    submission.add_argument("--reader-binary", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "init":
            result = initialize(args.state, args.world, args.writer, args.reader, args.port)
        elif args.command == "serve":
            result = serve(args.state, args.host, args.host_binary, seconds=args.seconds,
                max_storage_bytes=args.max_storage_bytes)
        else:
            import encounter_client
            if args.command == "read":
                result = encounter_client.read(args.config, args.save, reader_binary=args.reader_binary)
            else:
                result = encounter_client.submit(args.config, args.record, args.basis, reader_binary=args.reader_binary)
        print(json.dumps(result, ensure_ascii=True))
        return 0 if result.get("outcome") in ("initialized", "stopped", "read", "recorded") else 1
    except private.EncounterError as error:
        print(json.dumps({"outcome": "failed", "code": error.code}))
        return 1
    except (OSError, ValueError, RecursionError, subprocess.SubprocessError):
        print(json.dumps({"outcome": "failed", "code": "local_operation_failed"}))
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
