"""Run the Milestone 1 curl transcript against the loopback hosts in this repository.

The process under test is the one this script starts. A pass is evidence about that
process. It does not show that a host maintained apart from this repository speaks
the same profile.
"""

from __future__ import annotations

import json
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORLD = "civ:walk"
TASK_ID = "message:walk-task"
QUESTION_ID = "message:walk-question"
PRESERVE = "preserve-me"


class WalkFailure(RuntimeError):
    pass


def curl_binary() -> str:
    for name in ("curl.exe", "curl"):
        found = shutil.which(name)
        if found:
            return found
    raise WalkFailure("curl is not on PATH")


def free_port() -> int:
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return int(probe.getsockname()[1])


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


def message(message_id: str, sender: str, recipient: str, text: str, extra: dict | None = None) -> dict:
    record = {
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


def write_config(directory: Path, port: int) -> Path:
    config = {
        "world_id": WORLD,
        "title": "Walk",
        "database_path": str(directory / "world.sqlite"),
        "listen": f"127.0.0.1:{port}",
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


def start_host(argv: list[str]) -> subprocess.Popen:
    return subprocess.Popen(
        argv,
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )


def stop_host(process: subprocess.Popen) -> str:
    logs: list[str] = []
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)
    if process.stdout is not None:
        logs.extend(process.stdout.readlines())
    if process.stderr is not None:
        logs.extend(process.stderr.readlines())
    return "".join(logs)


def wait_until_ready(origin: str, process: subprocess.Popen) -> None:
    deadline = time.monotonic() + 30
    discovery = f"{origin}/.well-known/agentciv"
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise WalkFailure(f"host exited before discovery: {stop_host(process)[-2000:]}")
        try:
            with urllib.request.urlopen(discovery, timeout=1) as response:
                if response.status == 200:
                    return
        except (urllib.error.URLError, TimeoutError, OSError):
            time.sleep(0.1)
    raise WalkFailure("discovery did not answer")


def curl(binary: str, directory: Path, args: list[str]) -> tuple[int, dict[str, str], bytes]:
    header_path = directory / "headers.txt"
    body_path = directory / "body.bin"
    completed = subprocess.run(
        [binary, "-sS", "--max-time", "10", "-D", str(header_path), "-o", str(body_path), *args],
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


def decode(body: bytes) -> dict:
    parsed = json.loads(body.decode("utf-8"))
    if not isinstance(parsed, dict):
        raise WalkFailure("response JSON was not an object")
    return parsed


def expect(status: int, headers: dict[str, str], body: bytes, code: int, problem: str | None) -> dict:
    if status != code:
        raise WalkFailure(f"expected HTTP {code}, got {status}: {body[:500]!r}")
    parsed = decode(body) if body else {}
    if problem is not None:
        if "problem+json" not in headers.get("content-type", ""):
            raise WalkFailure(f"expected problem+json, got {headers.get('content-type')}")
        if parsed.get("code") != problem:
            raise WalkFailure(f"expected code {problem}, got {parsed.get('code')}")
    if headers.get("cache-control") != "no-store":
        raise WalkFailure(f"expected Cache-Control no-store, got {headers.get('cache-control')}")
    return parsed


def walk_one(label: str, argv_for: callable, curl_bin: str, directory: Path) -> None:
    port = free_port()
    origin = f"http://127.0.0.1:{port}"
    config = write_config(directory, port)
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
        wait_until_ready(origin, process)
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
            events = page.get("events")
            if not isinstance(events, list) or not page.get("next_cursor") or page.get("has_more") is not False:
                raise WalkFailure(f"{label} event page was not the two-message handoff")
            message_ids = []
            assigned = []
            for event in events:
                message_ids.append(event["body"]["message"]["id"])
                assigned.append(event["id"])
            return message_ids, assigned

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
        if page["events"][0]["body"]["message"].get("later_note") != PRESERVE:
            raise WalkFailure(f"{label} dropped an unknown message field")
    finally:
        stop_host(process)

    restarted = start_host(argv_for(config))
    try:
        wait_until_ready(origin, restarted)
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
        seen = [event["id"] for event in page["events"]]
        texts = [event["body"]["message"]["id"] for event in page["events"]]
        if texts != [TASK_ID, QUESTION_ID] or seen != assigned:
            raise WalkFailure(f"{label} restart did not keep the same events")
        if page["events"][0]["body"]["message"].get("later_note") != PRESERVE:
            raise WalkFailure(f"{label} restart dropped an unknown message field")
        cursor = page["next_cursor"]
    finally:
        stop_host(restarted)

    stored = json.loads(config.read_text(encoding="utf-8"))
    stored["visibility"] = "sender_only"
    config.write_text(json.dumps(stored), encoding="utf-8")
    narrowed = start_host(argv_for(config))
    try:
        wait_until_ready(origin, narrowed)
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
        leaked = [event["body"]["message"]["id"] for event in hidden.get("events", [])]
        if TASK_ID in leaked or QUESTION_ID in leaked:
            raise WalkFailure(f"{label} sender_only still showed another principal's message")
    finally:
        stop_host(narrowed)
    print(f"passed {label}")


def main() -> int:
    binary = curl_binary()
    host = rust_binary()
    targets = [
        (
            "python",
            lambda config: [
                sys.executable,
                str(ROOT / "implementations" / "http-commons-python" / "host.py"),
                "--config",
                str(config),
            ],
        ),
        ("rust", lambda config: [str(host), "--config", str(config)]),
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
