"""Inspect one private request journal against a saved caller observation offline.

Copied files do not authenticate origin or correlate an attempt without a receipt.
Even receipt correlation does not establish retry permission or current history.
"""
from __future__ import annotations

import hashlib
import json
import re
import subprocess
import uuid
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path
from typing import NoReturn

import encounter_client as client
import encounter_private as private
import encounter_return as returns
import collaboration as civic

JsonObject = dict[str, object]
FORMAT = "agentciv-private-request-inspection/0.1-example"
MAX_RESULT_BYTES = 512 * 1024
BASE_KEYS = {"format", "world", "principal", "record_id", "record_type", "request_utf8",
    "request_sha256", "endpoint_kind", "state", "copying_permission"}
RECEIPT_KEYS = {"protocol_version", "type", "status", "world", "record_id", "event_id", "sequence"}
REJECTION_CODES = {"malformed_json", "authentication_required", "forbidden", "id_conflict",
    "payload_too_large", "unsupported_media_type", "invalid_record", "unsupported_version",
    "unsupported_record_type", "wrong_world", "unknown_target"}
UNCERTAIN_CODES = {"publication_outcome_unverified", *client.PREFIX_READ_FAILURES}


def _invalid() -> NoReturn:
    raise private.EncounterError("invalid_request_journal")


def _receipt(value: object, record: JsonObject, settings: JsonObject) -> JsonObject:
    receipt = client._object(value)
    keys = RECEIPT_KEYS | ({"artifact_id", "revision"} if record["type"] == "artifact_revision" else set())
    if set(receipt) != keys:
        _invalid()
    # The existing writer normalizes receipt integers and stores no extensions.
    # Reject edited Decimal values before serializing this normalized small object.
    if type(receipt.get("sequence")) is not int or (record["type"] == "artifact_revision" and type(receipt.get("revision")) is not int):
        _invalid()
    try:
        return client._receipt(client._json(receipt), record, settings)
    except private.EncounterError:
        _invalid()


def _selected_matches(event: JsonObject, receipt: JsonObject, record: JsonObject) -> bool:
    return (event.get("id") == receipt["event_id"] and client._integer(event.get("sequence")) == receipt["sequence"]
        and event.get("world") == record["world"] and event.get("actor") == record["from"])


def _tombstone(event: JsonObject) -> bool:
    return event.get("kind") == "artifact.withdrawn" and event.get("body") == {}


def _journal(raw: bytes, settings: JsonObject) -> tuple[JsonObject, bytes, JsonObject, JsonObject | None]:
    journal = client._decode(raw)
    state = journal.get("state")
    optional: set[str]
    if state == "attempt_pending_outcome_unknown":
        optional = set()
    elif state == "receipt_received_readback_pending":
        optional = {"receipt"}
    elif state == "recorded":
        optional = {"receipt", "event_record_utf8"}
    elif state in ("uncertain", "rejected"):
        optional = {"failure_code"}
        if state == "uncertain" and "receipt" in journal:
            optional.add("receipt")
        permitted = UNCERTAIN_CODES if state == "uncertain" else REJECTION_CODES
        if journal.get("failure_code") not in permitted:
            _invalid()
    else:
        _invalid()
        optional = set()
    if (set(journal) != BASE_KEYS | optional or journal.get("format") != client.JOURNAL_FORMAT
        or journal.get("world") != settings["world"] or journal.get("principal") != settings["principal"]
        or journal.get("copying_permission") != "not_granted" or not isinstance(journal.get("request_utf8"), str)
        or not isinstance(journal.get("request_sha256"), str)
        or re.fullmatch(r"[0-9a-f]{64}", str(journal["request_sha256"])) is None):
        _invalid()
    if client._contains_secret(journal, str(settings["token"])):
        raise private.EncounterError("credential_reflected")
    request = str(journal["request_utf8"]).encode("utf-8")
    if len(request) > int(str(settings["payload_bytes"])):
        raise private.EncounterError("authored_record_too_large")
    if hashlib.sha256(request).hexdigest() != journal["request_sha256"]:
        _invalid()
    record = client._authored(request, settings)
    capability = "messages.submit" if record["type"] == "message" else "collaboration.submit"
    if (journal.get("record_id") != record["id"] or journal.get("record_type") != record["type"]
        or journal.get("endpoint_kind") != capability):
        _invalid()
    if (state == "rejected" and capability == "messages.submit" and journal["failure_code"] == "unknown_target"
        or state == "uncertain" and journal["failure_code"] in client.PREFIX_READ_FAILURES and "receipt" not in journal):
        _invalid()
    receipt = _receipt(journal["receipt"], record, settings) if "receipt" in journal else None
    if state == "recorded":
        original = journal["event_record_utf8"]
        if not isinstance(original, str) or len(original.encode("utf-8")) > 16000 or receipt is None:
            _invalid()
        event = client._decode(str(original).encode("utf-8"))
        if client._contains_secret(event, str(settings["token"])):
            raise private.EncounterError("credential_reflected")
        # This retained local original is a consistency check, never current view
        # evidence or a manufactured prior observation for a withdrawal.
        if (not _selected_matches(event, receipt, record) or event.get("protocol_version") != "0.1-draft"
            or event.get("type") != "event" or not isinstance(event.get("timestamp"), str)):
            _invalid()
        returns._time(event["timestamp"])
        if not (_tombstone(event) if record["type"] == "withdrawal" else client._matches_authored(record, event, receipt.get("revision"))):
            _invalid()
    return journal, request, record, receipt


def _syntax(request: bytes, settings: JsonObject, binary: Path) -> None:
    directory = Path(str(settings["cache_directory"]))
    with client._temporary(directory, "request-syntax") as path:
        private.write_new(path, request)
        try:
            result = subprocess.run([str(binary), "check-request", str(path)],
                capture_output=True, timeout=40, check=False)
        except (OSError, subprocess.SubprocessError):
            raise private.EncounterError("request_validator_unavailable") from None
    if len(result.stdout) > 1024 or len(result.stderr) > 1024:
        raise private.EncounterError("request_validator_unavailable")
    try:
        if result.returncode == 0 and not result.stderr:
            if client._decode(result.stdout) == {"outcome": "valid", "scope": "request_syntax", "acceptance": "not_established"}:
                return
        elif result.returncode != 0 and not result.stdout:
            diagnostic = client._decode(result.stderr)
            if diagnostic in ({"outcome": "failed", "code": "invalid_json"}, {"outcome": "failed", "code": "invalid_request"},
                {"outcome": "failed", "code": "invalid_utf8"}, {"outcome": "failed", "code": "input_too_large"}):
                raise private.EncounterError("request_syntax_unavailable")
    except (ValueError, civic.DecisionError):
        raise private.EncounterError("request_validator_unavailable") from None
    raise private.EncounterError("request_validator_unavailable")


@contextmanager
def _staging(settings: JsonObject) -> Iterator[JsonObject]:
    """No persistent cache lock or inputs: all staging is newly owned and private."""
    parent = Path(str(settings["cache_directory"]))
    private.check_directory(parent)
    directory = parent / (".inspect-request-" + uuid.uuid4().hex)
    private.create_directory(directory)
    try:
        with private.lock(directory):
            yield {**settings, "cache_directory": str(directory)}
    finally:
        private.check_directory(directory)
        if directory.resolve().parent != parent.resolve() or not directory.name.startswith(".inspect-request-"):
            raise private.EncounterError("request_staging_cleanup_failed")
        lock = directory / ".encounter.lock"
        # Never recursively remove an unexpected entry or a link supplied by a
        # failing helper. The private boundary excludes malicious same-owner code.
        if set(path.name for path in directory.iterdir()) - {lock.name}:
            raise private.EncounterError("request_staging_cleanup_failed")
        if lock.exists():
            private.check_file(lock)
            lock.unlink()
        directory.rmdir()


def inspect(config: Path, journal_path: Path, view_path: Path, *, reader_binary: Path | None = None) -> JsonObject:
    """No HTTP, dispatch, request regeneration, retained state updates, or retries."""
    try:
        config_raw = private.read_bounded(config, client.MAX_CONFIG)
        settings = client._configuration(config, raw=config_raw)
        journal_raw = private.read_bounded(journal_path, client.MAX_OUTPUT)
        view_raw = private.read_bounded(view_path, client.MAX_OUTPUT)
        journal, request, record, receipt = _journal(journal_raw, settings)
        with _staging(settings) as validation_settings:
            binary = client._binary(reader_binary)
            _syntax(request, validation_settings, binary)
            observation = returns._load(view_path, validation_settings, binary, raw=view_raw)
        candidates: list[JsonObject] = []
        selected: JsonObject | None = None
        if receipt is not None:
            found = observation.records.get(str(receipt["event_id"]))
            if found is None:
                status = "receipt_event_unavailable"
            else:
                original, event = found
                selected = {**returns._reference(str(event["id"]), event), "original_sha256": hashlib.sha256(original.encode("utf-8")).hexdigest()}
                if not _selected_matches(event, receipt, record):
                    status = "receipt_event_changed"
                elif record["type"] == "withdrawal":
                    status = "withdrawal_target_not_checkable" if _tombstone(event) else "receipt_event_changed"
                elif client._matches_authored(record, event, receipt.get("revision")):
                    status = "receipt_correlated_match"
                else:
                    status = "receipt_event_changed"
        elif record["type"] == "withdrawal":
            status = "withdrawal_target_not_checkable"
        else:
            for identifier, (original, event) in observation.records.items():
                body = event.get("body")
                stored = body.get(str(record["type"])) if isinstance(body, dict) else None
                revision = stored.get("revision") if isinstance(stored, dict) else None
                if client._matches_authored(record, event, revision):
                    candidates.append({**returns._reference(identifier, event), "original_sha256": hashlib.sha256(original.encode("utf-8")).hexdigest()})
            status = "compatible_candidates" if candidates else "no_match_in_observation"
        result: JsonObject = {"outcome": "inspected", "format": FORMAT, "evidence_status": status,
            "world": settings["world"], "principal": settings["principal"], "journal_state": journal["state"],
            "request_sha256": hashlib.sha256(request).hexdigest(), "input_sha256": {
                "config": hashlib.sha256(config_raw).hexdigest(), "journal": hashlib.sha256(journal_raw).hexdigest(),
                "view": hashlib.sha256(view_raw).hexdigest()}, "observation": observation.metadata,
            "receipt_retained": receipt is not None, "selected_event": selected, "compatible_candidates": candidates,
            "origin_authenticated": False, "copying_permission": "not_granted", "journal_state_changed": False,
            "read_boundary_advanced": False, "retry_permission": "not_established",
            "withdrawal_target": "not_checkable_without_prior_view" if record["type"] == "withdrawal" else "not_applicable",
            "scope": "supplied_private_files_and_one_saved_caller_observation_not_live_history",
            "interpretation": "A candidate does not identify this attempt. Absence has no diagnosed cause. A copied receipt does not authenticate its origin. No retry is authorized."}
        if journal["state"] == "rejected":
            result["local_rejection_claim"] = {"code": journal["failure_code"], "original_response_retained": False}
        if client._contains_secret(result, str(settings["token"])):
            raise private.EncounterError("credential_reflected")
        if len(json.dumps(result, ensure_ascii=True).encode("utf-8")) > MAX_RESULT_BYTES:
            raise private.EncounterError("request_inspection_too_large")
        return result
    except private.EncounterError:
        raise
    except (ValueError, OSError, KeyError, TypeError, RuntimeError, RecursionError):
        raise private.EncounterError("invalid_request_inspection") from None
