"""Scripted HTTP Commons participant.

The default draft is a fixed record. A caller can pass another function
with the same keyword arguments. This module speaks discovery, submission,
and event reading with the Python standard library, and only to a loopback
origin. It does not start a model process.
"""

from __future__ import annotations

import json
import urllib.error
import urllib.parse
import urllib.request
from http.client import HTTPMessage
from typing import IO, Protocol, TypedDict

from loopback import loopback_origin, require_loopback

PROTOCOL_VERSION = "0.1-draft"
PROFILE = "http-commons/0.1-draft"


class ParticipantError(RuntimeError):
    def __init__(self, status: int, code: str) -> None:
        super().__init__(f"HTTP {status} {code}")
        self.status = status
        self.code = code


JsonObject = dict[str, object]


class RecordedMessage(TypedDict):
    discovery: JsonObject
    record: JsonObject
    receipt: JsonObject


class Draft(Protocol):
    def __call__(
        self,
        *,
        world: str,
        principal: str,
        recipients: list[str],
        text: str,
        message_id: str,
    ) -> JsonObject: ...


class _RefuseRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(
        self,
        req: urllib.request.Request,
        fp: IO[bytes],
        code: int,
        msg: str,
        headers: HTTPMessage,
        newurl: str,
    ) -> urllib.request.Request | None:
        raise ParticipantError(code, "redirect")


def scripted_draft(
    *,
    world: str,
    principal: str,
    recipients: list[str],
    text: str,
    message_id: str,
) -> JsonObject:
    record: JsonObject = {
        "protocol_version": PROTOCOL_VERSION,
        "type": "message",
        "id": message_id,
        "world": world,
        "from": principal,
        "to": list(recipients),
        "body": {"text": text, "draft": "scripted"},
        "later_note": "preserve-me",
    }
    return record


def world_endpoints(origin: str, discovery: JsonObject) -> tuple[str, str]:
    origin = loopback_origin(origin)
    if not isinstance(discovery, dict) or discovery.get("profile") != PROFILE:
        raise ParticipantError(200, "unexpected_discovery")
    capabilities = discovery.get("capabilities")
    if (
        not isinstance(capabilities, list)
        or not all(isinstance(item, str) for item in capabilities)
        or not {"events.read", "messages.submit"}.issubset(capabilities)
    ):
        raise ParticipantError(200, "unexpected_discovery")
    endpoints = discovery.get("endpoints")
    if not isinstance(endpoints, dict):
        raise ParticipantError(200, "unexpected_discovery")
    submit = endpoints.get("submit")
    events = endpoints.get("events")
    if not isinstance(submit, str) or not isinstance(events, str):
        raise ParticipantError(200, "unexpected_discovery")
    _require_advertised(submit)
    _require_advertised(events)
    if not _same_origin(origin, submit) or not _same_origin(origin, events):
        raise ParticipantError(200, "unexpected_discovery")
    return submit, events


def discover(origin: str) -> JsonObject:
    root = loopback_origin(origin)
    status, payload = exchange("GET", root + "/.well-known/agentciv")
    if status != 200:
        _fail(status, payload)
    found = _object(payload, status)
    world_endpoints(root, found)
    return found


def record_message(
    origin: str,
    token: str,
    principal: str,
    recipients: list[str],
    *,
    text: str,
    message_id: str,
    draft: Draft = scripted_draft,
) -> RecordedMessage:
    if not isinstance(principal, str) or not principal:
        raise ValueError("principal is required")
    if not isinstance(message_id, str) or not message_id:
        raise ValueError("message id is required")
    if not isinstance(text, str):
        raise ValueError("text must be a string")
    if not recipients or len(set(recipients)) != len(list(recipients)):
        raise ValueError("recipients must be a non-empty unique list")
    discovery = discover(origin)
    submit, _events = world_endpoints(origin, discovery)
    record = draft(
        world=str(discovery.get("id", "")),
        principal=principal,
        recipients=list(recipients),
        text=text,
        message_id=message_id,
    )
    submitted = _as_object(record)
    if submitted is None:
        raise ValueError("draft must return an object")
    record = submitted
    if record.get("world") != discovery.get("id") or record.get("from") != principal:
        raise ValueError("draft world and from must match this call")
    body = json.dumps(record, ensure_ascii=False, separators=(",", ":"), sort_keys=True).encode("utf-8")
    status, payload = exchange("POST", submit, token=token, body=body)
    if status != 200:
        _fail(status, payload)
    receipt = _object(payload, status)
    if receipt.get("status") != "recorded" or receipt.get("record_id") != message_id:
        raise ParticipantError(status, "unexpected_receipt")
    return {"discovery": discovery, "record": record, "receipt": receipt}


def read_page(origin: str, token: str, *, after: str | None = None) -> JsonObject:
    discovery = discover(origin)
    _submit, events = world_endpoints(origin, discovery)
    url = events
    if after is not None:
        url = events + "?after=" + urllib.parse.quote(after, safe="")
    status, payload = exchange("GET", url, token=token)
    if status != 200:
        _fail(status, payload)
    page = _object(payload, status)
    if page.get("type") != "event_page" or not isinstance(page.get("events"), list):
        raise ParticipantError(status, "unexpected_page")
    return page


def messages_in(page: JsonObject) -> list[JsonObject]:
    found: list[JsonObject] = []
    events = page.get("events", [])
    if not isinstance(events, list):
        return found
    for event in events:
        event_object = _as_object(event)
        if event_object is None:
            continue
        body = _as_object(event_object.get("body"))
        if body is None:
            continue
        message = _as_object(body.get("message"))
        if message is not None:
            found.append(message)
    return found


def exchange(
    method: str,
    url: str,
    *,
    token: str | None = None,
    body: bytes | None = None,
) -> tuple[int, bytes]:
    require_loopback(url)
    request = urllib.request.Request(url, data=body, method=method)
    request.add_header("Accept", "application/json")
    request.add_header("User-Agent", "agentciv-participant-example")
    if token:
        request.add_header("Authorization", f"Bearer {token}")
    if body is not None:
        request.add_header("Content-Type", "application/json")
    opener = urllib.request.build_opener(_RefuseRedirect)
    try:
        with opener.open(request, timeout=5) as response:
            return int(response.status), response.read()
    except ParticipantError:
        raise
    except urllib.error.HTTPError as error:
        try:
            return int(error.code), error.read()
        finally:
            error.close()
    except urllib.error.URLError as error:
        raise ParticipantError(0, "unreachable") from error


def _require_advertised(url: str) -> None:
    try:
        require_loopback(url)
    except ValueError as error:
        raise ParticipantError(200, "unexpected_discovery") from error


def _same_origin(origin: str, url: str) -> bool:
    left = urllib.parse.urlsplit(origin)
    right = urllib.parse.urlsplit(url)
    return (left.scheme, left.hostname, left.port) == (right.scheme, right.hostname, right.port)


def _as_object(value: object) -> JsonObject | None:
    if not isinstance(value, dict):
        return None
    parsed: JsonObject = {}
    for key, item in value.items():
        if not isinstance(key, str):
            return None
        parsed[key] = item
    return parsed


def _object(payload: bytes, status: int) -> JsonObject:
    try:
        value: object = json.loads(payload)
    except json.JSONDecodeError as error:
        raise ParticipantError(status, "unreadable") from error
    found = _as_object(value)
    if found is None:
        raise ParticipantError(status, "unreadable")
    return found


def _fail(status: int, payload: bytes) -> None:
    code = "unreadable"
    try:
        value = json.loads(payload)
    except json.JSONDecodeError:
        value = None
    if isinstance(value, dict) and isinstance(value.get("code"), str) and value["code"]:
        code = value["code"]
    raise ParticipantError(status, code)
