"""Private manual encounter client using existing loopback contracts.

Authored request bytes are never regenerated or executed. Private read caches
are local views, not permission to publish records or an atomic history claim.
"""
from __future__ import annotations

import hashlib
import json
import subprocess
import uuid
from collections.abc import Iterator
from contextlib import contextmanager
from datetime import datetime, timezone
from decimal import Decimal
from pathlib import Path
from urllib.parse import urlsplit

import collaboration as civic
import local_participant as wire
import encounter_private as private

JsonObject = dict[str, object]
CONFIG_FORMAT = "agentciv-local-encounter-client/0.1-example"
VIEW_FORMAT = "agentciv-private-encounter-view/0.1-example"
JOURNAL_FORMAT = "agentciv-private-encounter-request/0.1-example"
CACHE_PERMISSION = "operator_authorized_local_view_only_no_public_export"
MAX_CONFIG = 65536
MAX_OUTPUT = 4 * 1024 * 1024
MAX_CACHE = 16 * 1024 * 1024
MAX_FILES = 16
PREFIX_READ_FAILURES = ("prefix_changed", "prefix_unavailable", "cursor_expired")
BUDGETS: JsonObject = {"max_pages": 32, "max_events": 2048, "max_response_bytes": 262144,
    "max_record_bytes": 16000, "max_total_bytes": 4194304, "seconds": 30}


def _object(value: object) -> JsonObject:
    return civic.object_value(value)


def _json(value: object) -> bytes:
    return json.dumps(value, ensure_ascii=False, allow_nan=False, indent=2).encode("utf-8")


def _decode(raw: bytes) -> JsonObject:
    civic.decode(raw)  # Duplicate members and nonstandard constants are rejected first.
    value: object = json.loads(raw, parse_float=Decimal)
    return _object(value)


def _integer(value: object, minimum: int = 0) -> int | None:
    if isinstance(value, bool) or not isinstance(value, (int, Decimal)):
        return None
    if isinstance(value, Decimal) and (not value.is_finite() or value != value.to_integral_value()):
        return None
    return int(value) if minimum <= value <= 9_007_199_254_740_991 else None


def _text(value: object, maximum: int = 1024) -> bool:
    return isinstance(value, str) and 0 < len(value.encode("utf-8")) <= maximum and not any(ord(char) < 32 or ord(char) == 127 for char in value)


def _configuration(path: Path) -> JsonObject:
    value = civic.decode(private.read_bounded(path, MAX_CONFIG))
    required = {"format", "origin", "world", "principal", "token", "payload_bytes", "cache_directory", "private_cache_permission"}
    # Rooms initialized before the optional member roster remain valid.
    if set(value) not in (required, required | {"members"}):
        raise private.EncounterError("invalid_client_configuration")
    members = value.get("members", [value["principal"]])
    if (not isinstance(members, list) or not 1 <= len(members) <= 16 or len(set(map(str, members))) != len(members)
        or not all(_text(item) for item in members) or value["principal"] not in members):
        raise private.EncounterError("invalid_client_configuration")
    origin = value["origin"]
    if not isinstance(origin, str):
        raise private.EncounterError("invalid_client_configuration")
    parts = urlsplit(origin)
    if (parts.scheme != "http" or parts.hostname != "127.0.0.1" or parts.username is not None or parts.password is not None
        or parts.path or parts.query or parts.fragment or parts.port is None or not 1 <= parts.port <= 65535
        or origin != f"http://127.0.0.1:{parts.port}"):
        raise private.EncounterError("invalid_client_configuration")
    if (value["format"] != CONFIG_FORMAT or not _text(value["world"]) or not _text(value["principal"])
        or not _text(value["token"], 256) or any(char.isspace() for char in str(value["token"]))
        or type(value["payload_bytes"]) is not int or not 1 <= int(str(value["payload_bytes"])) <= 16384
        or value["private_cache_permission"] != CACHE_PERMISSION or not isinstance(value["cache_directory"], str)):
        raise private.EncounterError("invalid_client_configuration")
    private.check_directory(Path(str(value["cache_directory"])))
    return value


def _binary(selected: Path | None) -> Path:
    found = str(selected) if selected is not None else private.find_tool("agentciv-reader")
    if not found or not Path(found).is_file():
        raise private.EncounterError("reader_binary_not_found")
    return Path(found).resolve()


def _process(binary: Path, command: str, path: Path, previous: Path | None = None) -> JsonObject:
    try:
        arguments = [str(binary), command, str(path)] + ([""] if command == "offers" else [])
        if previous is not None:
            arguments.append(str(previous))
        result = subprocess.run(arguments, capture_output=True, timeout=40, check=False)
    except (OSError, subprocess.SubprocessError):
        raise private.EncounterError("reader_process_failed") from None
    if result.returncode != 0 and command in ("read", "sync") and len(result.stderr) <= 1024:
        try:
            diagnostic = civic.decode(result.stderr)
        except civic.DecisionError:
            diagnostic = {}
        if diagnostic == {"outcome": "failed", "code": "transport_failed"}:
            raise private.EncounterError("host_unreachable")
        if diagnostic == {"outcome": "failed", "code": "deadline_exceeded"}:
            raise private.EncounterError("host_read_timeout")
        for code in (*PREFIX_READ_FAILURES, "prefix_validation_unsupported", "prefix_validation_failed", "invalid_resume_state", "forbidden", "authentication_required"):
            if diagnostic == {"outcome": "failed", "code": code}:
                raise private.EncounterError(code)
    if result.returncode != 0 or len(result.stdout) > MAX_OUTPUT:
        raise private.EncounterError("reader_failed")
    try:
        return civic.decode(result.stdout)
    except civic.DecisionError:
        raise private.EncounterError("invalid_reader_output") from None


@contextmanager
def _temporary(directory: Path, prefix: str) -> Iterator[Path]:
    path = directory / f".{prefix}-{uuid.uuid4().hex}.json"
    try:
        yield path
    finally:
        if path.exists():
            private.check_file(path)
            path.unlink()


def _native_read(config: JsonObject, binary: Path, *, incremental: bool = False, previous: JsonObject | None = None) -> JsonObject:
    directory = Path(str(config["cache_directory"]))
    with _temporary(directory, "reader") as path, _temporary(directory, "resume") as saved:
        settings: JsonObject = {"origin": config["origin"], "world": config["world"], "token": config["token"],
            "traversal": "all", "budgets": BUDGETS}
        if incremental:
            settings["prefix_validation"] = True
        private.write_new(path, _json(settings))
        if previous is not None:
            private.write_new(saved, _json({"snapshot": previous["snapshot"], "report": previous["transport"], "resume": previous.get("resume")}))
            return _process(binary, "sync", path, saved)
        return _process(binary, "read", path)


def _project(native: JsonObject, binary: Path, directory: Path) -> JsonObject:
    with _temporary(directory, "projection") as path:
        private.write_new(path, _json(native))
        return _process(binary, "offers", path)


def _originals(view: JsonObject) -> list[str]:
    snapshot = _object(view.get("snapshot"))
    rows = snapshot.get("records")
    if not isinstance(rows, list) or not all(isinstance(row, str) for row in rows):
        raise private.EncounterError("invalid_private_view")
    return [str(row) for row in rows]


def _contains_secret(value: object, secret: str) -> bool:
    if isinstance(value, str):
        return secret in value
    if isinstance(value, dict):
        return any(_contains_secret(key, secret) or _contains_secret(item, secret) for key, item in value.items())
    if isinstance(value, list):
        return any(_contains_secret(item, secret) for item in value)
    return False


def _validate_view(view: JsonObject, config: JsonObject) -> None:
    transport = _object(view.get("transport"))
    snapshot = _object(view.get("snapshot"))
    originals = _originals(view)
    projection = _object(view.get("offers"))
    projection_report = _object(projection.get("report"))
    if (view.get("format") != VIEW_FORMAT or view.get("world") != config["world"] or view.get("principal") != config["principal"]
        or snapshot.get("world") != config["world"] or transport.get("reached_end") is not True
        or transport.get("scope") != "current_caller_view" or transport.get("copying_permission") != "not_granted"
        or type(transport.get("events")) is not int or transport.get("events") != len(originals)
        or view.get("copying_permission") != "not_granted" or view.get("private_cache_permission") != CACHE_PERMISSION
        or projection.get("format") != "agentciv-offer-view/0.1-example" or projection.get("world") != config["world"]
        or projection_report.get("truncated") is not False or projection_report.get("reached_end") is not True):
        raise private.EncounterError("invalid_private_view")
    if _contains_secret(view, str(config["token"])):
        raise private.EncounterError("credential_reflected")


def _capture(config: JsonObject, binary: Path, *, incremental: bool = False, previous: JsonObject | None = None) -> JsonObject:
    started = datetime.now(timezone.utc).isoformat()
    native = _native_read(config, binary, incremental=True, previous=previous) if incremental else _native_read(config, binary)
    if str(config["token"]) in json.dumps(native, ensure_ascii=False):
        raise private.EncounterError("credential_reflected")
    projection = _project(native, binary, Path(str(config["cache_directory"])))
    report = _object(projection.get("report"))
    if (projection.get("format") != "agentciv-offer-view/0.1-example" or projection.get("world") != config["world"]
        or report.get("truncated") is not False or report.get("reached_end") is not True
        or report.get("scope") != "current_caller_view" or report.get("copying_permission") != "not_granted"):
        raise private.EncounterError("invalid_offer_projection")
    view: JsonObject = {"format": VIEW_FORMAT, "world": config["world"], "principal": config["principal"],
        "snapshot": native.get("snapshot"), "transport": native.get("report"), "offers": projection,
        "retrieval_started": started, "retrieval_finished": datetime.now(timezone.utc).isoformat(),
        "copying_permission": "not_granted", "private_cache_permission": CACHE_PERMISSION,
        "freshness": "bounded_full_read_not_atomic", "configured_budgets": BUDGETS}
    if incremental:
        if _object(native.get("report")).get("prefix_validated") is not True or native.get("resume") is None:
            raise private.EncounterError("prefix_validation_failed")
        view["resume"] = native["resume"]
        view["freshness"] = "validated_prefix_observation_not_atomic_with_action"
    _validate_view(view, config)
    if len(_json(view)) > MAX_OUTPUT:
        raise private.EncounterError("private_view_too_large")
    return view


def _capacity(directory: Path, prefix: str, added: int, replaced: Path | None = None) -> None:
    total = 0
    matching = 0
    for path in directory.iterdir():
        if path.name.startswith("."):
            continue
        if not path.is_file():
            raise private.EncounterError("invalid_cache_entry")
        private.check_file(path)
        if path == replaced:
            continue
        total += path.stat().st_size
        matching += path.name.startswith(prefix)
    if total + added > MAX_CACHE or matching >= MAX_FILES:
        raise private.EncounterError("private_cache_full")


def _save_view(view: JsonObject, config: JsonObject, path: Path | None = None) -> Path:
    directory = Path(str(config["cache_directory"]))
    selected = path or directory / f"view-{uuid.uuid4().hex}.json"
    if selected.parent != directory or not selected.name.startswith("view-") or selected.suffix != ".json":
        raise private.EncounterError("invalid_private_view_path")
    raw = _json(view)
    _capacity(directory, "view-", len(raw))
    private.write_new(selected, raw)
    return selected


def read(config: Path, save: Path | None = None, *, reader_binary: Path | None = None) -> JsonObject:
    try:
        settings = _configuration(config)
        with private.lock(Path(str(settings["cache_directory"]))):
            binary = _binary(reader_binary)
            view = _capture(settings, binary)
            path = _save_view(view, settings, save)
        transport = _object(view["transport"])
        return {"outcome": "read", "world": settings["world"], "principal": settings["principal"],
            "events": transport["events"], "pages": transport["pages"], "view_path": str(path), "reader_binary": str(binary),
            "copying_permission": "not_granted", "freshness": "bounded_full_read_not_atomic"}
    except private.EncounterError:
        raise
    # RuntimeError includes parser RecursionError; no nested input enters diagnostics.
    except (ValueError, OSError, KeyError, TypeError, RuntimeError):
        raise private.EncounterError("private_read_failed") from None


def sync(config: Path, previous: Path | None = None, save: Path | None = None, *, reader_binary: Path | None = None) -> JsonObject:
    settings = _configuration(config)
    directory = Path(str(settings["cache_directory"]))
    with private.lock(directory):
        prior = None
        if previous is not None:
            if previous.parent != directory or not previous.name.startswith("view-") or previous.suffix != ".json":
                raise private.EncounterError("invalid_private_view_path")
            prior = _decode(private.read_bounded(previous, MAX_OUTPUT))
            _validate_view(prior, settings)
        binary = _binary(reader_binary)
        view = _capture(settings, binary, incremental=True, previous=prior)
        path = _save_view(view, settings, save)
    transport = _object(view["transport"])
    return {"outcome": "synced", "world": settings["world"], "principal": settings["principal"],
        "events": transport["events"], "cached_events": transport.get("cached_events", 0), "pages": transport["pages"],
        "response_bytes": transport["response_bytes"], "view_path": str(path), "freshness": view["freshness"],
        "copying_permission": "not_granted"}


def show(config: Path, path: Path, *, event_id: str | None = None) -> str:
    """Render a saved permitted view; quote untrusted text before terminal output."""
    try:
        settings = _configuration(config)
        view = _decode(private.read_bounded(path, MAX_OUTPUT))
        _validate_view(view, settings)
        lines = [f"Room {json.dumps(settings['world'], ensure_ascii=True)}; saved caller view, not live history."]
        seen: set[str] = set()
        selected = 0
        for original in _originals(view):
            event = _decode(original.encode("utf-8"))
            identifier = event.get("id")
            if not isinstance(identifier, str) or identifier in seen:
                raise private.EncounterError("invalid_private_view")
            seen.add(identifier)
            if _contains_secret(event, str(settings["token"])):
                raise private.EncounterError("credential_reflected")
            if event_id is not None and identifier != event_id:
                continue
            selected += 1
            # Each exact original remains in the view; this is only a readable projection.
            def quote(value: object) -> str:
                if isinstance(value, Decimal):
                    return str(value)
                if isinstance(value, dict):
                    return "{" + ", ".join(f"{quote(key)}: {quote(item)}" for key, item in value.items()) + "}"
                if isinstance(value, list):
                    return "[" + ", ".join(quote(item) for item in value) + "]"
                return json.dumps(value, ensure_ascii=True)
            lines.append(f"#{quote(event.get('sequence'))} {quote(event.get('timestamp'))} {quote(event.get('actor'))} {quote(event.get('kind'))} {quote(event.get('id'))}")
            if event_id is not None:
                lines.append(f"  record_utf8: {json.dumps(original, ensure_ascii=True)}")
                continue
            body = _object(event.get("body"))
            for kind, value in body.items():
                record = _object(value)
                lines.append(f"  {quote(kind)} to {quote(record.get('to', []))}")
                for key in ("artifact_id", "revision", "target_from", "derived_from"):
                    if key in record:
                        lines.append(f"  {key}: {quote(record[key])}")
                if "body" in record:
                    content = _object(record["body"])
                    if "text" in content:
                        lines.append(f"  text: {quote(content['text'])}")
                    other = {key: value for key, value in content.items() if key != "text"}
                    if other:
                        lines.append(f"  data: {quote(other)}")
                if kind == "tombstone":
                    lines.append(f"  {quote(record)}")
        if event_id is not None and selected == 0:
            raise private.EncounterError("event_not_in_saved_view")
        if len(lines) == 1:
            lines.append("No events in this saved view.")
        rendered = "\n".join(lines)
        if len(rendered.encode("utf-8")) > MAX_OUTPUT:
            raise private.EncounterError("private_render_too_large")
        return rendered
    except private.EncounterError:
        raise
    except (ValueError, OSError, KeyError, TypeError, RuntimeError):
        raise private.EncounterError("invalid_private_view") from None


def _authored(raw: bytes, settings: JsonObject) -> JsonObject:
    value = _decode(raw)
    if (value.get("protocol_version") != "0.1-draft" or value.get("world") != settings["world"]
        or value.get("from") != settings["principal"] or not _text(value.get("id"))
        or value.get("type") not in ("message", "artifact_revision", "objection", "decline", "withdrawal")
        or str(settings["token"]) in raw.decode("utf-8") or str(settings["token"]) in str(value)):
        raise private.EncounterError("invalid_authored_record")
    if value["type"] == "artifact_revision" and "revision" in value:
        raise private.EncounterError("invalid_authored_record")
    if value["type"] in ("objection", "decline", "withdrawal") and _integer(value.get("revision"), 1) is None:
        raise private.EncounterError("invalid_authored_record")
    return value


def _endpoint(settings: JsonObject, record: JsonObject) -> tuple[str, str]:
    descriptor = wire.discover(str(settings["origin"]))
    if descriptor.get("id") != settings["world"]:
        raise private.EncounterError("world_changed")
    submit_endpoint, _ = wire.world_endpoints(str(settings["origin"]), descriptor)
    endpoints = _object(descriptor.get("endpoints"))
    for endpoint in endpoints.values():
        if not isinstance(endpoint, str) or not wire._same_origin(str(settings["origin"]), endpoint):
            raise private.EncounterError("invalid_discovery_endpoint")
        parts = urlsplit(endpoint)
        if parts.username is not None or parts.password is not None or parts.fragment:
            raise private.EncounterError("invalid_discovery_endpoint")
    if record["type"] == "message":
        return submit_endpoint, "messages.submit"
    capabilities = descriptor.get("capabilities")
    collaborate = endpoints.get("collaborate")
    if not isinstance(capabilities, list) or "collaboration.submit" not in capabilities or not isinstance(collaborate, str):
        raise private.EncounterError("collaboration_unavailable")
    return collaborate, "collaboration.submit"


def _basis(path: Path | None, current: JsonObject, settings: JsonObject, binary: Path, record: JsonObject) -> None:
    if path is not None:
        previous = civic.decode(private.read_bounded(path, MAX_OUTPUT))
        _validate_view(previous, settings)
        # Revalidate the copied syntax too; a cached report is not a current credential grant.
        _project({"snapshot": previous["snapshot"], "report": previous["transport"]}, binary, Path(str(settings["cache_directory"])))
        current_rows = {str(_decode(row.encode())["id"]): row for row in _originals(current)}
        if any(current_rows.get(str(_decode(row.encode())["id"])) != row for row in _originals(previous)):
            raise private.EncounterError("stale_private_basis")
    targets: list[JsonObject] = []
    if record["type"] == "artifact_revision" and "derived_from" in record:
        targets.append(_object(record["derived_from"]))
    if record["type"] in ("objection", "decline", "withdrawal"):
        targets.append({"from": record.get("target_from"), "artifact_id": record.get("artifact_id"), "revision": record.get("revision")})
    for target in targets:
        if _integer(target.get("revision"), 1) is None:
            raise private.EncounterError("invalid_source_reference")
        found = False
        for raw in _originals(current):
            event = _decode(raw.encode())
            if event.get("kind") != "artifact.recorded":
                continue
            artifact = _object(_object(event.get("body")).get("artifact_revision"))
            if all(artifact.get(key) == target.get(key) for key in ("from", "artifact_id", "revision")):
                found = True
        if not found:
            raise private.EncounterError("source_unavailable")


def _receipt(raw: bytes, record: JsonObject, settings: JsonObject) -> JsonObject:
    value = _decode(raw)
    if (value.get("protocol_version") != "0.1-draft" or value.get("type") != "receipt"
        or value.get("status") != "recorded" or value.get("world") != settings["world"]
        or value.get("record_id") != record["id"] or not _text(value.get("event_id"))
        or type(value.get("sequence")) is not int or _integer(value.get("sequence")) is None
        or str(settings["token"]) in str(value)):
        raise private.EncounterError("invalid_receipt")
    keys = ("protocol_version", "type", "status", "world", "record_id", "event_id", "sequence")
    result = {key: value[key] for key in keys}
    if record["type"] == "artifact_revision":
        if value.get("artifact_id") != record.get("artifact_id") or type(value.get("revision")) is not int or _integer(value.get("revision"), 1) is None:
            raise private.EncounterError("invalid_receipt")
        result.update({"artifact_id": value["artifact_id"], "revision": value["revision"]})
    return result


def _json_equal(left: object, right: object) -> bool:
    """Compare JSON values without Python's boolean/integer aliasing."""
    if isinstance(left, bool) or isinstance(right, bool):
        return isinstance(left, bool) and isinstance(right, bool) and left is right
    if isinstance(left, (int, Decimal)) or isinstance(right, (int, Decimal)):
        if not isinstance(left, (int, Decimal)) or not isinstance(right, (int, Decimal)):
            return False
        if isinstance(left, Decimal) and not left.is_finite() or isinstance(right, Decimal) and not right.is_finite():
            return False
        return left == right
    if isinstance(left, dict) and isinstance(right, dict):
        return left.keys() == right.keys() and all(_json_equal(value, right[key]) for key, value in left.items())
    if isinstance(left, list) and isinstance(right, list):
        return len(left) == len(right) and all(_json_equal(a, b) for a, b in zip(left, right, strict=True))
    if isinstance(left, str) and isinstance(right, str):
        return left == right
    return left is None and right is None


def _verify_readback(record: JsonObject, receipt: JsonObject, before: JsonObject, after: JsonObject) -> str:
    found = [(event, raw) for raw in _originals(after) if (event := _decode(raw.encode())).get("id") == receipt["event_id"]]
    if len(found) != 1:
        raise private.EncounterError("stored_record_unavailable")
    event, raw = found[0]
    if event.get("world") != record["world"] or event.get("sequence") != receipt["sequence"] or event.get("actor") != record["from"]:
        raise private.EncounterError("stored_record_changed")
    if record["type"] == "withdrawal":
        prior = next((_decode(span.encode()) for span in _originals(before) if _decode(span.encode()).get("id") == receipt["event_id"]), {})
        target = _object(_object(prior.get("body")).get("artifact_revision"))
        if (event.get("kind") != "artifact.withdrawn" or event.get("body") != {} or prior.get("sequence") != event.get("sequence")
            or prior.get("timestamp") != event.get("timestamp") or target.get("from") != record.get("target_from")
            or target.get("artifact_id") != record.get("artifact_id") or target.get("revision") != record.get("revision")):
            raise private.EncounterError("stored_record_changed")
    else:
        kind = {"message": "message.recorded", "artifact_revision": "artifact.recorded", "objection": "objection.recorded", "decline": "decline.recorded"}[str(record["type"])]
        stored = _object(_object(event.get("body")).get(str(record["type"])))
        expected = dict(record)
        if record["type"] == "artifact_revision":
            expected["revision"] = receipt["revision"]
        if event.get("kind") != kind or not _json_equal(stored, expected):
            raise private.EncounterError("stored_record_changed")
    return raw


def _rejection(status: int, raw: bytes, capability: str) -> str | None:
    """A malformed or contradictory error cannot establish nonpublication."""
    problem = _decode(raw)
    problem_type = problem.get("type")
    title = problem.get("title")
    code = problem.get("code")
    if (not isinstance(problem_type, str) or not problem_type.isascii()
        or not isinstance(title, str) or not title
        or type(problem.get("status")) is not int or problem["status"] != status
        or not 400 <= status <= 599 or not isinstance(code, str) or not code
        or "detail" in problem and not isinstance(problem["detail"], str)):
        return None
    uri = urlsplit(problem_type)
    if not uri.scheme or uri.scheme in ("http", "https") and (not uri.hostname or uri.port is not None and not 0 <= uri.port <= 65535):
        return None
    uri_characters = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~:/?#[]@!$&'()*+,;=%"
    if any(char not in uri_characters for char in problem_type):
        return None
    for index, char in enumerate(problem_type):
        if char == "%" and (index + 2 >= len(problem_type) or any(part not in "0123456789ABCDEFabcdef" for part in problem_type[index + 1:index + 3])):
            return None
    submission_codes = {
        400: {"malformed_json"},
        401: {"authentication_required"},
        403: {"forbidden"},
        409: {"id_conflict"},
        413: {"payload_too_large"},
        415: {"unsupported_media_type"},
        422: {"invalid_record", "unsupported_version", "unsupported_record_type", "wrong_world"},
    }
    if capability == "collaboration.submit":
        submission_codes[422].add("unknown_target")
    return code if code in submission_codes.get(status, set()) else None


def say(config: Path, text: str, to: list[str] | None = None, basis: Path | None = None, *,
        reader_binary: Path | None = None) -> JsonObject:
    """Publish a message from its text alone. The envelope comes from the room configuration.

    Each call creates a new message identifier, so repeating a command can record a second message.
    The exact bytes sent remain in the private request journal.
    """
    try:
        settings = _configuration(config)
        if not isinstance(text, str) or not text.strip():
            raise private.EncounterError("empty_message")
        principal = str(settings["principal"])
        others = [str(item) for item in _members(settings) if item != principal]
        recipients = list(dict.fromkeys(to)) if to else (others or [principal])
        if not all(_text(item) for item in recipients):
            raise private.EncounterError("invalid_recipient")
        record: JsonObject = {"protocol_version": "0.1-draft", "type": "message", "id": f"message:{uuid.uuid4().hex}",
            "world": settings["world"], "from": principal, "to": recipients, "body": {"text": text}}
        raw = (json.dumps(record, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    except private.EncounterError:
        raise
    except (ValueError, UnicodeError):
        raise private.EncounterError("invalid_message_text") from None
    return _publish(settings, raw, basis, reader_binary)


def _members(settings: JsonObject) -> list[object]:
    members = settings.get("members", [settings["principal"]])
    return list(members) if isinstance(members, list) else []


def submit(config: Path, record: Path, basis: Path | None = None, *, reader_binary: Path | None = None) -> JsonObject:
    try:
        settings = _configuration(config)
        # Authored input may be a reviewed repository file. It is never a credential/cache file.
        maximum = int(str(settings["payload_bytes"]))
        with record.open("rb") as source:
            raw = source.read(maximum + 1)
    except private.EncounterError:
        raise
    except OSError:
        raise private.EncounterError("private_submit_failed") from None
    return _publish(settings, raw, basis, reader_binary)


def _publish(settings: JsonObject, raw: bytes, basis: Path | None, reader_binary: Path | None) -> JsonObject:
    try:
        maximum = int(str(settings["payload_bytes"]))
        if len(raw) > maximum:
            raise private.EncounterError("authored_record_too_large")
        authored = _authored(raw, settings)
        directory = Path(str(settings["cache_directory"]))
        binary = _binary(reader_binary)
        with private.lock(directory):
            before = _capture(settings, binary)
            _basis(basis, before, settings, binary, authored)
            endpoint, capability = _endpoint(settings, authored)
            journal_path = directory / f"journal-{uuid.uuid4().hex}.json"
            journal: JsonObject = {"format": JOURNAL_FORMAT, "world": settings["world"], "principal": settings["principal"],
                "record_id": authored["id"], "record_type": authored["type"], "request_utf8": raw.decode("utf-8"),
                "request_sha256": hashlib.sha256(raw).hexdigest(), "endpoint_kind": capability,
                "state": "attempt_pending_outcome_unknown", "copying_permission": "not_granted"}
            _capacity(directory, "journal-", len(_json(journal)) + MAX_OUTPUT)
            private.write_new(journal_path, _json(journal))
            result: JsonObject = {"outcome": "uncertain", "world": settings["world"], "principal": settings["principal"],
                "record_id": authored["id"], "journal_path": str(journal_path)}
            try:
                status, response = wire.exchange("POST", endpoint, token=str(settings["token"]), body=raw)
                if status != 200:
                    rejection = _rejection(status, response, capability)
                    journal.update({"state": "rejected" if rejection is not None else "uncertain",
                        "failure_code": rejection if rejection is not None else "publication_outcome_unverified"})
                    if rejection is not None:
                        result.update({"outcome": "rejected", "code": rejection})
                else:
                    receipt = _receipt(response, authored, settings)
                    journal["receipt"] = receipt
                    journal["state"] = "receipt_received_readback_pending"
                    private.replace_private(journal_path, _json(journal))
                    after = _capture(settings, binary)
                    span = _verify_readback(authored, receipt, before, after)
                    journal.update({"state": "recorded", "event_record_utf8": span})
                    result.update({"outcome": "recorded", "event_id": receipt["event_id"], "sequence": receipt["sequence"]})
            except private.EncounterError as error:
                journal["state"] = "uncertain"
                journal["failure_code"] = error.code if error.code in PREFIX_READ_FAILURES else "publication_outcome_unverified"
                if error.code in PREFIX_READ_FAILURES:
                    result["code"] = error.code
            except (ValueError, OSError, KeyError, TypeError, RuntimeError):
                journal["state"] = "uncertain"
                journal["failure_code"] = "publication_outcome_unverified"
            _capacity(directory, "journal-", len(_json(journal)), replaced=journal_path)
            private.replace_private(journal_path, _json(journal))
            return result
    except private.EncounterError:
        raise
    except (ValueError, OSError, KeyError, TypeError, RuntimeError):
        raise private.EncounterError("private_submit_failed") from None
