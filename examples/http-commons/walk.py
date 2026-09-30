"""Run the Milestone 1 curl transcript against the loopback hosts in this repository.

The process under test is the one this script starts. A pass is evidence about that
process. It does not show that a host maintained apart from this repository speaks
the same profile. The collaboration posts are this script. They are not a participant
choosing to revise, object, or decline.
"""

from __future__ import annotations

import ipaddress
import json
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.parse
from collections.abc import Callable, Mapping
from pathlib import Path
from typing import IO

ROOT = Path(__file__).resolve().parents[2]
WORLD = "civ:walk"
TASK_ID = "message:walk-task"
QUESTION_ID = "message:walk-question"
PRESERVE = "preserve-me"
PLAN_ID = "submission:walk-plan"
OBJECTION_ID = "submission:walk-objection"
DECLINE_ID = "submission:walk-decline"
DENIED_ID = "submission:walk-denied"
ARTIFACT_ID = "artifact:walk-plan"


class WalkFailure(RuntimeError):
    pass


JsonObject = dict[str, object]


def as_object(value: object) -> JsonObject | None:
    if not isinstance(value, dict):
        return None
    parsed: JsonObject = {}
    for key, item in value.items():
        if not isinstance(key, str):
            return None
        parsed[key] = item
    return parsed


def curl_binary() -> str:
    for name in ("curl.exe", "curl"):
        found = shutil.which(name)
        if found:
            return found
    raise WalkFailure("curl is not on PATH")


def rust_binary() -> Path:
    for folder in ("debug", "release"):
        for name in ("agentciv-host.exe", "agentciv-host"):
            path = ROOT / "target" / folder / name
            if path.is_file():
                return path
    build = subprocess.run(
        ["cargo", "build", "--locked", "-p", "agentciv-host"],
        cwd=ROOT,
        check=False,
    )
    if build.returncode != 0:
        raise WalkFailure("cargo build of agentciv-host failed")
    for name in ("agentciv-host.exe", "agentciv-host"):
        path = ROOT / "target" / "debug" / name
        if path.is_file():
            return path
    raise WalkFailure("agentciv-host binary was not produced")


def message(
    message_id: str,
    sender: str,
    recipient: str,
    text: str,
    extra: Mapping[str, object] | None = None,
) -> JsonObject:
    record: JsonObject = {
        "protocol_version": "0.1-draft",
        "type": "message",
        "id": message_id,
        "world": WORLD,
        "from": sender,
        "to": [recipient],
        "body": {"text": text},
    }
    if extra:
        record.update(extra)
    return record


def write_config(directory: Path) -> Path:
    config = {
        "world_id": WORLD,
        "title": "Walk",
        "database_path": str(directory / "world.sqlite"),
        "listen": "127.0.0.1:0",
        "visibility": "members",
        "retention_seconds": 86400,
        "max_payload_bytes": 4096,
        "credentials": [
            {"principal": "agent:walk-a", "token": "walk-token-a", "read": True, "write": True},
            {"principal": "agent:walk-b", "token": "walk-token-b", "read": True, "write": True},
            {"principal": "agent:walk-c", "token": "walk-token-c", "read": True, "write": False},
        ],
    }
    path = directory / "host.json"
    path.write_text(json.dumps(config), encoding="utf-8")
    return path


class RunningHost:
    def __init__(self, process: subprocess.Popen[str]) -> None:
        self.process = process
        self.ready = threading.Event()
        self.lock = threading.Lock()
        self.log: list[str] = []
        self.readers: list[threading.Thread] = []
        self.origin = ""

    def add_log(self, line: str) -> None:
        with self.lock:
            self.log.append(line)

    def text(self) -> str:
        with self.lock:
            return "".join(self.log)

    def note_discovery(self, line: str) -> None:
        self.add_log(line)
        origin = advertised_origin(line)
        if origin is None:
            return
        with self.lock:
            self.origin = origin
        self.ready.set()

    def current_origin(self) -> str:
        with self.lock:
            return self.origin


def advertised_origin(line: str) -> str | None:
    prefix = "discovery "
    suffix = "/.well-known/agentciv"
    stripped = line.strip()
    if not stripped.startswith(prefix) or not stripped.endswith(suffix):
        return None
    origin = stripped[len(prefix) : -len(suffix)]
    parsed = urllib.parse.urlsplit(origin)
    if parsed.scheme != "http" or parsed.hostname is None or parsed.port is None:
        return None
    try:
        loopback = ipaddress.ip_address(parsed.hostname).is_loopback
    except ValueError:
        return None
    if not loopback:
        return None
    return origin


def probe_discovery(origin: str) -> bool:
    parsed = urllib.parse.urlsplit(origin)
    host = parsed.hostname
    port = parsed.port
    if host is None or port is None:
        return False
    payload = (
        b"GET /.well-known/agentciv HTTP/1.1\r\n"
        b"Host: loopback\r\n"
        b"Accept: application/json\r\n"
        b"Connection: close\r\n"
        b"\r\n"
    )
    try:
        with socket.create_connection((host, port), timeout=1) as sock:
            sock.settimeout(1)
            sock.sendall(payload)
            status = b""
            while b"\r\n" not in status and len(status) < 128:
                chunk = sock.recv(128)
                if not chunk:
                    break
                status += chunk
    except OSError:
        return False
    return status.split(b"\r\n", 1)[0].startswith((b"HTTP/1.1 200", b"HTTP/1.0 200"))


def start_host(argv: list[str]) -> RunningHost:
    process = subprocess.Popen(
        argv,
        cwd=ROOT,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    host = RunningHost(process)

    def drain(pipe: IO[str] | None, discovery: bool) -> None:
        if pipe is None:
            return
        try:
            for line in pipe:
                if discovery:
                    host.note_discovery(line)
                else:
                    host.add_log(line)
        finally:
            pipe.close()

    for pipe, discovery in ((process.stdout, True), (process.stderr, False)):
        reader = threading.Thread(target=drain, args=(pipe, discovery), daemon=True)
        reader.start()
        host.readers.append(reader)
    return host


def stop_host(host: RunningHost) -> str:
    process = host.process
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)
    for reader in host.readers:
        reader.join(timeout=2)
    return host.text()[-2000:]


def wait_until_ready(host: RunningHost) -> str:
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        if host.process.poll() is not None:
            raise WalkFailure(f"host exited before discovery: {stop_host(host)[-2000:]}")
        remaining = max(0.0, deadline - time.monotonic())
        if host.ready.wait(min(0.2, remaining)) and probe_discovery(host.current_origin()):
            return host.current_origin()
    state = "running" if host.process.poll() is None else f"exited {host.process.returncode}"
    raise WalkFailure(f"discovery did not answer ({state}): {host.text()[-1500:]}")


def curl(binary: str, directory: Path, args: list[str]) -> tuple[int, dict[str, str], bytes]:
    header_path = directory / "headers.txt"
    body_path = directory / "body.bin"
    completed = subprocess.run(
        [
            binary,
            "-sS",
            "--noproxy",
            "*",
            "--max-time",
            "10",
            "-D",
            str(header_path),
            "-o",
            str(body_path),
            *args,
        ],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        raise WalkFailure(completed.stderr.strip() or f"curl exited {completed.returncode}")
    headers: dict[str, str] = {}
    for line in header_path.read_text(encoding="utf-8", errors="replace").splitlines():
        if ":" not in line:
            continue
        name, value = line.split(":", 1)
        headers[name.strip().lower()] = value.strip()
    status_line = header_path.read_text(encoding="utf-8", errors="replace").splitlines()[0]
    status = int(status_line.split()[1])
    return status, headers, body_path.read_bytes()


def decode(body: bytes) -> JsonObject:
    try:
        parsed: object = json.loads(body.decode("utf-8"))
    except (UnicodeError, json.JSONDecodeError) as error:
        raise WalkFailure("response JSON was not an object") from error
    found = as_object(parsed)
    if found is None:
        raise WalkFailure("response JSON was not an object")
    return found


def expect(
    status: int, headers: dict[str, str], body: bytes, code: int, problem: str | None
) -> JsonObject:
    if status != code:
        raise WalkFailure(f"expected HTTP {code}, got {status}: {body[:500]!r}")
    parsed: JsonObject = decode(body) if body else {}
    if problem is not None:
        if "problem+json" not in headers.get("content-type", ""):
            raise WalkFailure(f"expected problem+json, got {headers.get('content-type')}")
        if parsed.get("code") != problem:
            raise WalkFailure(f"expected code {problem}, got {parsed.get('code')}")
    if headers.get("cache-control") != "no-store":
        raise WalkFailure(f"expected Cache-Control no-store, got {headers.get('cache-control')}")
    return parsed


def events_of(page: JsonObject) -> list[object]:
    events = page.get("events")
    if not isinstance(events, list):
        raise WalkFailure("event page did not include events")
    return list(events)


def submission_in(event: object) -> tuple[str, str, str]:
    event_object = as_object(event)
    if event_object is None:
        raise WalkFailure("event was not an object")
    kind = event_object.get("kind")
    event_id = event_object.get("id")
    if not isinstance(kind, str) or not isinstance(event_id, str):
        raise WalkFailure("event omitted kind or id")
    body = as_object(event_object.get("body"))
    if body is None:
        raise WalkFailure("event omitted a body")
    for key in ("message", "artifact_revision", "objection", "decline"):
        record = as_object(body.get(key))
        if record is None:
            continue
        record_id = record.get("id")
        if isinstance(record_id, str):
            return event_id, kind, record_id
    raise WalkFailure("event omitted a submission id")


def record_ids(events: list[object]) -> list[str]:
    found: list[str] = []
    for event in events:
        event_object = as_object(event)
        body = as_object(event_object.get("body")) if event_object is not None else None
        if body is None:
            continue
        for key in ("message", "artifact_revision", "objection", "decline"):
            record = as_object(body.get(key))
            if record is None:
                continue
            record_id = record.get("id")
            if isinstance(record_id, str):
                found.append(record_id)
    return found


def message_in(event: object) -> JsonObject:
    event_object = as_object(event)
    body = as_object(event_object.get("body")) if event_object is not None else None
    message_object = as_object(body.get("message")) if body is not None else None
    if message_object is None:
        raise WalkFailure("event page did not include the expected message")
    return message_object


def python_argv(config: Path) -> list[str]:
    return [
        sys.executable,
        str(ROOT / "implementations" / "http-commons-python" / "host.py"),
        "--config",
        str(config),
    ]


def record_of(event: object, key: str) -> JsonObject:
    event_object = as_object(event)
    body = as_object(event_object.get("body")) if event_object is not None else None
    record = as_object(body.get(key)) if body is not None else None
    if record is None:
        raise WalkFailure("collaboration record was incomplete")
    return record


def require_collaboration_page(label: str, events: list[object]) -> list[str]:
    parsed = [submission_in(event) for event in events]
    actual = [(kind, record_id) for _event_id, kind, record_id in parsed]
    expected = [
        ("message.recorded", TASK_ID),
        ("message.recorded", QUESTION_ID),
        ("artifact.recorded", PLAN_ID),
        ("objection.recorded", OBJECTION_ID),
        ("decline.recorded", DECLINE_ID),
    ]
    if actual != expected:
        raise WalkFailure(f"{label} collaboration history was {actual}")
    if message_in(events[0]).get("later_note") != PRESERVE:
        raise WalkFailure(f"{label} collaboration restart dropped the message note")
    plan = record_of(events[2], "artifact_revision")
    note = as_object(plan.get("continuity_note"))
    plan_body = as_object(plan.get("body"))
    if (
        plan.get("revision") != 1
        or plan.get("artifact_id") != ARTIFACT_ID
        or plan_body is None
        or plan_body.get("text") != "Keep the pages addressable for a later reader."
        or note is None
        or note.get("aim") != "Leave work a later participant can resume or reject."
        or note.get("resume_hint") != "Read the objection before choosing."
    ):
        raise WalkFailure(f"{label} did not keep the artifact and its continuity note")
    objection = record_of(events[3], "objection")
    objection_body = as_object(objection.get("body"))
    decline = record_of(events[4], "decline")
    decline_body = as_object(decline.get("body"))
    if (
        objection.get("revision") != 1
        or objection.get("artifact_id") != ARTIFACT_ID
        or objection_body is None
        or objection_body.get("text") != "The pages should stay separable."
        or decline.get("revision") != 1
        or decline_body is None
        or decline_body.get("text") != "I will not take up this revision."
    ):
        raise WalkFailure(f"{label} did not keep the objection and the decline")
    return [event_id for event_id, _kind, _record_id in parsed]


def collaboration_handoff(
    label: str,
    argv_for: Callable[[Path], list[str]],
    curl_bin: str,
    directory: Path,
) -> None:
    denied_path = directory / "denied-collaboration.json"
    plan_path = directory / "plan.json"
    objection_path = directory / "objection.json"
    decline_path = directory / "decline.json"
    denied_record: JsonObject = {
        "protocol_version": "0.1-draft",
        "type": "artifact_revision",
        "id": DENIED_ID,
        "artifact_id": "artifact:walk-denied",
        "world": WORLD,
        "from": "agent:walk-c",
        "to": ["agent:walk-a"],
        "media_type": "application/json",
        "body": {"text": "A reader cannot append."},
    }
    plan_record: JsonObject = {
        "protocol_version": "0.1-draft",
        "type": "artifact_revision",
        "id": PLAN_ID,
        "artifact_id": ARTIFACT_ID,
        "world": WORLD,
        "from": "agent:walk-a",
        "to": ["agent:walk-b", "agent:walk-c"],
        "media_type": "application/json",
        "body": {"text": "Keep the pages addressable for a later reader."},
        "continuity_note": {
            "aim": "Leave work a later participant can resume or reject.",
            "resume_hint": "Read the objection before choosing.",
        },
    }
    objection_record: JsonObject = {
        "protocol_version": "0.1-draft",
        "type": "objection",
        "id": OBJECTION_ID,
        "world": WORLD,
        "from": "agent:walk-b",
        "to": ["agent:walk-a", "agent:walk-c"],
        "artifact_id": ARTIFACT_ID,
        "target_from": "agent:walk-a",
        "revision": 1,
        "body": {"text": "The pages should stay separable."},
    }
    decline_record: JsonObject = {
        "protocol_version": "0.1-draft",
        "type": "decline",
        "id": DECLINE_ID,
        "world": WORLD,
        "from": "agent:walk-b",
        "to": ["agent:walk-a"],
        "artifact_id": ARTIFACT_ID,
        "target_from": "agent:walk-a",
        "revision": 1,
        "body": {"text": "I will not take up this revision."},
    }
    for path, record in (
        (denied_path, denied_record),
        (plan_path, plan_record),
        (objection_path, objection_record),
        (decline_path, decline_record),
    ):
        path.write_text(json.dumps(record), encoding="utf-8")

    def post(origin: str, token: str, path: Path) -> tuple[int, dict[str, str], bytes]:
        return curl(
            curl_bin,
            directory,
            [
                "-H",
                "Content-Type: application/json",
                "-H",
                f"Authorization: Bearer {token}",
                "--data-binary",
                f"@{path}",
                f"{origin}/collaborate",
            ],
        )

    def reader_page(origin: str) -> JsonObject:
        status, headers, body = curl(
            curl_bin,
            directory,
            [
                "-H",
                "Accept: application/json",
                "-H",
                "Authorization: Bearer walk-token-c",
                f"{origin}/events",
            ],
        )
        page = expect(status, headers, body, 200, None)
        if not page.get("next_cursor") or page.get("has_more") is not False:
            raise WalkFailure(f"{label} collaboration page was incomplete")
        return page

    recorded: list[str] | None = None
    process = start_host(argv_for(directory / "host.json"))
    try:
        origin = wait_until_ready(process)
        status, _headers, body = curl(
            curl_bin,
            directory,
            ["-H", "Accept: application/json", f"{origin}/.well-known/agentciv"],
        )
        if status != 200:
            raise WalkFailure(f"{label} discovery returned {status}")
        world = decode(body)
        capabilities = world.get("capabilities")
        endpoints = as_object(world.get("endpoints"))
        collaborate = endpoints.get("collaborate") if endpoints is not None else None
        if (
            not isinstance(capabilities, list)
            or "collaboration.submit" not in capabilities
            or collaborate != f"{origin}/collaborate"
        ):
            raise WalkFailure(f"{label} does not advertise collaboration.submit")
        status, headers, body = post(origin, "walk-token-c", denied_path)
        expect(status, headers, body, 403, "forbidden")
        if DENIED_ID in record_ids(events_of(reader_page(origin))):
            raise WalkFailure(f"{label} stored a denied collaboration")
        status, headers, body = post(origin, "walk-token-a", plan_path)
        receipt = expect(status, headers, body, 200, None)
        if (
            receipt.get("status") != "recorded"
            or receipt.get("record_id") != PLAN_ID
            or receipt.get("artifact_id") != ARTIFACT_ID
            or receipt.get("revision") != 1
            or "aim" in receipt
            or "resume_hint" in receipt
            or "continuity_note" in receipt
        ):
            raise WalkFailure(f"{label} artifact receipt was not a recorded revision")
        plan_event = receipt.get("event_id")
        status, headers, body = post(origin, "walk-token-a", plan_path)
        retry = expect(status, headers, body, 200, None)
        if retry.get("event_id") != plan_event:
            raise WalkFailure(f"{label} artifact retry minted a new event")
        status, headers, body = post(origin, "walk-token-b", objection_path)
        objection = expect(status, headers, body, 200, None)
        if objection.get("record_id") != OBJECTION_ID or objection.get("event_id") == plan_event:
            raise WalkFailure(f"{label} did not record a distinct objection")
        status, headers, body = post(origin, "walk-token-b", decline_path)
        decline = expect(status, headers, body, 200, None)
        if decline.get("record_id") != DECLINE_ID or decline.get("event_id") in {
            plan_event,
            objection.get("event_id"),
        }:
            raise WalkFailure(f"{label} did not record a distinct decline")
        recorded = require_collaboration_page(label, events_of(reader_page(origin)))
    finally:
        stop_host(process)
    if recorded is None:
        raise WalkFailure(f"{label} did not record the collaboration handoff")

    restarted = start_host(argv_for(directory / "host.json"))
    try:
        origin = wait_until_ready(restarted)
        after = require_collaboration_page(label, events_of(reader_page(origin)))
        if after != recorded:
            raise WalkFailure(f"{label} restart did not keep the collaboration history")
    finally:
        stop_host(restarted)


def walk_one(label: str, argv_for: Callable[[Path], list[str]], curl_bin: str, directory: Path) -> None:
    config = write_config(directory)
    task = directory / "task.json"
    question = directory / "question.json"
    task_record = message(
        TASK_ID,
        "agent:walk-a",
        "agent:walk-b",
        "Unfinished task: name the open question and how a later reader would judge it.",
        {"later_note": PRESERVE},
    )
    question_record = message(
        QUESTION_ID,
        "agent:walk-b",
        "agent:walk-a",
        "Question: a later reader should cite both host-assigned event ids.",
    )
    task.write_text(json.dumps(task_record), encoding="utf-8")
    question.write_text(json.dumps(question_record), encoding="utf-8")
    process = start_host(argv_for(config))
    try:
        origin = wait_until_ready(process)
        status, headers, body = curl(
            curl_bin,
            directory,
            ["-H", "Accept: application/json", f"{origin}/.well-known/agentciv"],
        )
        if status != 200:
            raise WalkFailure(f"{label} discovery returned {status}")
        world = decode(body)
        if world.get("profile") != "http-commons/0.1-draft" or world.get("id") != WORLD:
            raise WalkFailure(f"{label} discovery did not describe this world")
        if headers.get("content-type", "").split(";")[0] != "application/json":
            raise WalkFailure(f"{label} discovery content type was {headers.get('content-type')}")

        status, headers, body = curl(
            curl_bin,
            directory,
            [
                "-H",
                "Accept: application/json",
                "-H",
                "Authorization: Bearer walk-token-c",
                f"{origin}/events?after=",
            ],
        )
        expect(status, headers, body, 400, "invalid_cursor")

        status, headers, body = curl(
            curl_bin,
            directory,
            ["-H", "Content-Type: application/json", "--data-binary", f"@{task}", f"{origin}/submit"],
        )
        denied = expect(status, headers, body, 401, "authentication_required")
        if "Bearer" not in headers.get("www-authenticate", ""):
            raise WalkFailure(f"{label} 401 did not challenge Bearer")
        status, headers, body = curl(
            curl_bin,
            directory,
            [
                "-H",
                "Content-Type: application/json",
                "-H",
                "Authorization: Bearer walk-token-c",
                "--data-binary",
                f"@{task}",
                f"{origin}/submit",
            ],
        )
        expect(status, headers, body, 403, "forbidden")

        status, headers, body = curl(
            curl_bin,
            directory,
            [
                "-H",
                "Content-Type: application/json; charset=utf-8",
                "-H",
                "Authorization: Bearer walk-token-a",
                "--data-binary",
                f"@{task}",
                f"{origin}/submit",
            ],
        )
        receipt = expect(status, headers, body, 200, None)
        if receipt.get("status") != "recorded" or receipt.get("record_id") != TASK_ID:
            raise WalkFailure(f"{label} did not record the task")
        event_id = receipt.get("event_id")
        sequence = receipt.get("sequence")

        status, headers, body = curl(
            curl_bin,
            directory,
            [
                "-H",
                "Content-Type: application/json",
                "-H",
                "Authorization: Bearer walk-token-a",
                "--data-binary",
                f"@{task}",
                f"{origin}/submit",
            ],
        )
        retry = expect(status, headers, body, 200, None)
        if retry.get("event_id") != event_id or retry.get("sequence") != sequence:
            raise WalkFailure(f"{label} retry minted a new event")

        conflict_path = directory / "conflict.json"
        conflict = dict(task_record)
        conflict["body"] = {"text": "different bytes"}
        conflict_path.write_text(json.dumps(conflict), encoding="utf-8")
        status, headers, body = curl(
            curl_bin,
            directory,
            [
                "-H",
                "Content-Type: application/json",
                "-H",
                "Authorization: Bearer walk-token-a",
                "--data-binary",
                f"@{conflict_path}",
                f"{origin}/submit",
            ],
        )
        expect(status, headers, body, 409, "id_conflict")

        status, headers, body = curl(
            curl_bin,
            directory,
            [
                "-H",
                "Content-Type: application/json",
                "-H",
                "Authorization: Bearer walk-token-b",
                "--data-binary",
                f"@{question}",
                f"{origin}/submit",
            ],
        )
        second = expect(status, headers, body, 200, None)
        if second.get("event_id") == event_id or second.get("record_id") != QUESTION_ID:
            raise WalkFailure(f"{label} did not record a distinct question")

        def read_events() -> tuple[list[str], list[str]]:
            status, headers, body = curl(
                curl_bin,
                directory,
                [
                    "-H",
                    "Accept: application/json",
                    "-H",
                    "Authorization: Bearer walk-token-c",
                    f"{origin}/events",
                ],
            )
            page = expect(status, headers, body, 200, None)
            events = events_of(page)
            if not page.get("next_cursor") or page.get("has_more") is not False:
                raise WalkFailure(f"{label} event page was not the two-message handoff")
            message_ids: list[str] = []
            assigned_ids: list[str] = []
            for event in events:
                message_id = message_in(event).get("id")
                event_object = as_object(event)
                event_id_value = event_object.get("id") if event_object is not None else None
                if not isinstance(message_id, str) or not isinstance(event_id_value, str):
                    raise WalkFailure(f"{label} event page was not the two-message handoff")
                message_ids.append(message_id)
                assigned_ids.append(event_id_value)
            return message_ids, assigned_ids

        message_ids, assigned = read_events()
        if message_ids != [TASK_ID, QUESTION_ID]:
            raise WalkFailure(f"{label} reader saw {message_ids}")
        if assigned[0] != event_id or len(set(assigned)) != 2:
            raise WalkFailure(f"{label} event ids were not host-assigned")
        # Re-read once through the first page to confirm the preserved field on that response.
        status, headers, body = curl(
            curl_bin,
            directory,
            [
                "-H",
                "Accept: application/json",
                "-H",
                "Authorization: Bearer walk-token-c",
                f"{origin}/events",
            ],
        )
        page = expect(status, headers, body, 200, None)
        first_events = events_of(page)
        if not first_events or message_in(first_events[0]).get("later_note") != PRESERVE:
            raise WalkFailure(f"{label} dropped an unknown message field")
    finally:
        stop_host(process)

    restarted = start_host(argv_for(config))
    try:
        origin = wait_until_ready(restarted)
        status, headers, body = curl(
            curl_bin,
            directory,
            [
                "-H",
                "Accept: application/json",
                "-H",
                "Authorization: Bearer walk-token-c",
                f"{origin}/events",
            ],
        )
        page = expect(status, headers, body, 200, None)
        restarted_events = events_of(page)
        seen: list[str] = []
        texts: list[str] = []
        for event in restarted_events:
            event_object = as_object(event)
            event_id_value = event_object.get("id") if event_object is not None else None
            message_id = message_in(event).get("id")
            if not isinstance(event_id_value, str) or not isinstance(message_id, str):
                raise WalkFailure(f"{label} restart did not keep the same events")
            seen.append(event_id_value)
            texts.append(message_id)
        if texts != [TASK_ID, QUESTION_ID] or seen != assigned:
            raise WalkFailure(f"{label} restart did not keep the same events")
        if not restarted_events or message_in(restarted_events[0]).get("later_note") != PRESERVE:
            raise WalkFailure(f"{label} restart dropped an unknown message field")
        found_cursor = page.get("next_cursor")
        if not isinstance(found_cursor, str) or not found_cursor:
            raise WalkFailure(f"{label} event page did not include a cursor")
        cursor = found_cursor
    finally:
        stop_host(restarted)

    collaboration_handoff(label, argv_for, curl_bin, directory)

    try:
        loaded: object = json.loads(config.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        raise WalkFailure("host config was not an object") from error
    stored = as_object(loaded)
    if stored is None:
        raise WalkFailure("host config was not an object")
    stored["visibility"] = "sender_only"
    config.write_text(json.dumps(stored), encoding="utf-8")
    narrowed = start_host(argv_for(config))
    try:
        origin = wait_until_ready(narrowed)
        quoted = urllib.parse.quote(str(cursor), safe="")
        status, headers, body = curl(
            curl_bin,
            directory,
            [
                "-H",
                "Accept: application/json",
                "-H",
                "Authorization: Bearer walk-token-c",
                f"{origin}/events?after={quoted}",
            ],
        )
        expect(status, headers, body, 410, "cursor_expired")
        status, headers, body = curl(
            curl_bin,
            directory,
            [
                "-H",
                "Accept: application/json",
                "-H",
                "Authorization: Bearer walk-token-c",
                f"{origin}/events",
            ],
        )
        hidden = expect(status, headers, body, 200, None)
        leaked = record_ids(events_of(hidden))
        hidden_ids = {TASK_ID, QUESTION_ID, PLAN_ID, OBJECTION_ID, DECLINE_ID}
        if hidden_ids.intersection(leaked):
            raise WalkFailure(f"{label} sender_only still showed another principal's record")
    finally:
        stop_host(narrowed)
    print(f"passed {label}")


def main() -> int:
    binary = curl_binary()
    host_path = rust_binary()

    def rust_argv(config: Path) -> list[str]:
        return [str(host_path), "--config", str(config)]

    targets: list[tuple[str, Callable[[Path], list[str]]]] = [
        ("python", python_argv),
        ("rust", rust_argv),
    ]
    try:
        for label, argv_for in targets:
            with tempfile.TemporaryDirectory(prefix=f"agentciv-walk-{label}-") as temporary:
                walk_one(label, argv_for, binary, Path(temporary))
    except WalkFailure as error:
        print(f"FAILED: {error}", file=sys.stderr)
        return 1
    print("passed both loopback hosts")
    return 0


if __name__ == "__main__":
    sys.exit(main())
