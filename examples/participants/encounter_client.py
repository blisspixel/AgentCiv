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
BUDGETS: JsonObject = {"max_pages": 20, "max_events": 256, "max_response_bytes": 262144,
    "max_record_bytes": 16000, "max_total_bytes": 1048576, "seconds": 30}


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
    if set(value) != {"format", "origin", "world", "principal", "token", "payload_bytes", "cache_directory", "private_cache_permission"}:
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
    found = str(selected) if selected is not None else private.find_executable("agentciv-reader")
    if not found or not Path(found).is_file():
        raise private.EncounterError("reader_unavailable")
    return Path(found).resolve()


def _process(binary: Path, command: str, path: Path) -> JsonObject:
    try:
        arguments = [str(binary), command, str(path)] + ([""] if command == "offers" else [])
        result = subprocess.run(arguments, capture_output=True, timeout=40, check=False)
    except (OSError, subprocess.SubprocessError):
        raise private.EncounterError("reader_process_failed") from None
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


def _native_read(config: JsonObject, binary: Path) -> JsonObject:
    directory = Path(str(config["cache_directory"]))
    with _temporary(directory, "reader") as path:
        private.write_new(path, _json({"origin": config["origin"], "world": config["world"], "token": config["token"],
            "traversal": "all", "budgets": BUDGETS}))
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
    if str(config["token"]) in json.dumps(view, ensure_ascii=False):
        raise private.EncounterError("credential_reflected")


def _capture(config: JsonObject, binary: Path) -> JsonObject:
    started = datetime.now(timezone.utc).isoformat()
    native = _native_read(config, binary)
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
            view = _capture(settings, _binary(reader_binary))
            path = _save_view(view, settings, save)
        transport = _object(view["transport"])
        return {"outcome": "read", "world": settings["world"], "principal": settings["principal"],
            "events": transport["events"], "pages": transport["pages"], "view_path": str(path),
            "copying_permission": "not_granted", "freshness": "bounded_full_read_not_atomic"}
    except private.EncounterError:
        raise
    # RuntimeError includes parser RecursionError; no nested input enters diagnostics.
    except (ValueError, OSError, KeyError, TypeError, RuntimeError):
        raise private.EncounterError("private_read_failed") from None


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


def submit(config: Path, record: Path, basis: Path | None = None, *, reader_binary: Path | None = None) -> JsonObject:
    try:
        settings = _configuration(config)
        # Authored input may be a reviewed repository file. It is never a credential/cache file.
        maximum = int(str(settings["payload_bytes"]))
        with record.open("rb") as source:
            raw = source.read(maximum + 1)
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
                        result["outcome"] = "rejected"
                else:
                    receipt = _receipt(response, authored, settings)
                    journal["receipt"] = receipt
                    journal["state"] = "receipt_received_readback_pending"
                    private.replace_private(journal_path, _json(journal))
                    after = _capture(settings, binary)
                    span = _verify_readback(authored, receipt, before, after)
                    journal.update({"state": "recorded", "event_record_utf8": span})
                    result.update({"outcome": "recorded", "event_id": receipt["event_id"], "sequence": receipt["sequence"]})
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
