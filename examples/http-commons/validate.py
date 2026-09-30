"""Exercise public reports against both local hosts under every visibility policy.

This operator adapter creates disposable worlds. Assertions remain in the Rust
black-box runner. It is evidence about these two hosts, not interoperability.
"""

from __future__ import annotations

import argparse
import json
import os
import secrets
import subprocess
import sys
import tempfile
from collections.abc import Callable
from datetime import datetime, timezone
from pathlib import Path

import walk
from evidence import source_identity

ROOT = Path(__file__).resolve().parents[2]
POLICIES = ("members", "addressed", "sender_only")
HIDDEN_CASES = {
    "collaborate.hidden_visibility",
    "collaborate.hidden_derivation",
    "collaborate.hidden_objection",
    "collaborate.hidden_decline",
    "collaborate.hidden_withdrawal",
}
LIFECYCLE_CASES = {
    "prepare": {"lifecycle.world", "lifecycle.collaboration", "lifecycle.fresh", "lifecycle.seed", "lifecycle.checkpoint"},
    "verify": {"lifecycle.world", "lifecycle.collaboration", "restart.history", "restart.cursor", "restart.retry", "restart.no_duplicate"},
    "policy": {"lifecycle.world", "lifecycle.collaboration", "policy.cursor_expired", "policy.visibility"},
}


class ValidationFailure(RuntimeError):
    pass


def object_of(value: object) -> dict[str, object]:
    found = walk.as_object(value)
    if found is None:
        raise ValidationFailure("report must be a JSON object")
    return found


def checked_report(text: str, returncode: int, policy: str) -> dict[str, object]:
    try:
        parsed: object = json.loads(text)
    except json.JSONDecodeError as error:
        raise ValidationFailure("runner did not emit JSON") from error
    report = object_of(parsed)
    if returncode != 0:
        raise ValidationFailure("runner reported a failure")
    if report.get("profile") != "http-commons/0.1-draft":
        raise ValidationFailure("unexpected report profile")
    if report.get("runner_scope") != "credentialed-extended":
        raise ValidationFailure("unexpected report scope")
    cases = report.get("cases")
    if not isinstance(cases, list) or not cases:
        raise ValidationFailure("runner omitted its cases")
    seen: set[str] = set()
    counts = {"passed": 0, "failed": 0, "skipped": 0}
    for item in cases:
        case = object_of(item)
        case_id = case.get("id")
        status = case.get("status")
        if not isinstance(case_id, str) or case_id in seen:
            raise ValidationFailure("case IDs must be unique strings")
        seen.add(case_id)
        if not isinstance(status, str) or status not in counts:
            raise ValidationFailure("unknown case status")
        counts[status] += 1
        if status == "failed" or (
            status == "skipped"
            and not (policy == "members" and case_id in HIDDEN_CASES and case.get("required") is False)
        ):
            raise ValidationFailure(f"case did not pass: {case_id}")
    expected = HIDDEN_CASES | {
        "submit.concurrent_retry", "submit.concurrent_conflict", "submit.concurrent_distinct",
        "collaborate.revision", "collaborate.objection", "collaborate.decline",
    }
    if not expected.issubset(seen):
        raise ValidationFailure("report omitted required matrix cases")
    if report.get("summary") != counts:
        raise ValidationFailure("summary does not match case results")
    return report


def checked_lifecycle(text: str, returncode: int, phase: str) -> dict[str, object]:
    try:
        parsed: object = json.loads(text)
    except json.JSONDecodeError as error:
        raise ValidationFailure("lifecycle did not emit JSON") from error
    report = object_of(parsed)
    if (returncode != 0 or report.get("profile") != "http-commons/0.1-draft"
            or report.get("runner_scope") != f"lifecycle-{phase}"):
        raise ValidationFailure(f"lifecycle {phase} failed or used another scope")
    cases = report.get("cases")
    if not isinstance(cases, list):
        raise ValidationFailure("lifecycle omitted its cases")
    seen: set[str] = set()
    for item in cases:
        case = object_of(item)
        case_id = case.get("id")
        if not isinstance(case_id, str) or case_id in seen or case.get("status") != "passed":
            raise ValidationFailure("lifecycle case failed, duplicated, or was skipped")
        seen.add(case_id)
    if phase not in LIFECYCLE_CASES or seen != LIFECYCLE_CASES[phase]:
        raise ValidationFailure("lifecycle case inventory was incomplete")
    if report.get("summary") != {"passed": len(seen), "failed": 0, "skipped": 0}:
        raise ValidationFailure("lifecycle summary did not match its cases")
    return report


def configure(directory: Path, policy: str) -> tuple[Path, dict[str, str]]:
    if policy not in POLICIES:
        raise ValidationFailure("unknown visibility policy")
    parties = (
        ("agent:abc123", "AGENTCIV_CONFORMANCE_TOKEN", True),
        ("agent:peer", "AGENTCIV_CONFORMANCE_PEER_TOKEN", True),
        ("agent:reader", "AGENTCIV_CONFORMANCE_READER_TOKEN", False),
    )
    environment = dict(os.environ)
    credentials: list[dict[str, object]] = []
    for principal, variable, write in parties:
        token = secrets.token_urlsafe(32)
        environment[variable] = token
        credentials.append({"principal": principal, "token": token, "read": True, "write": write})
    config = {
        "world_id": "civ:validation",
        "title": "Disposable validation world",
        "database_path": str(directory / "world.sqlite"),
        "listen": "127.0.0.1:0",
        "visibility": policy,
        "retention_seconds": 86400,
        "max_payload_bytes": 4096,
        "credentials": credentials,
    }
    path = directory / "host.json"
    path.write_text(json.dumps(config), encoding="utf-8")
    return path, environment


def run_one(
    label: str, argv_for: Callable[[Path], list[str]], runner: Path, policy: str,
) -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="agentciv-validation-") as temporary:
        config, environment = configure(Path(temporary), policy)
        host = walk.start_host(argv_for(config))
        try:
            discovery = walk.wait_until_ready(host) + "/.well-known/agentciv"
            result = subprocess.run(
                [str(runner), "--discovery", discovery, "--principal", "agent:abc123",
                 "--reader", "agent:reader", "--peer", "agent:peer"],
                cwd=ROOT, env=environment, capture_output=True, text=True, timeout=180, check=False,
            )
            report = checked_report(result.stdout, result.returncode, policy)
            return {"implementation": label, "visibility": policy, "report": report}
        finally:
            walk.stop_host(host)


def binaries() -> tuple[Path, Path]:
    try:
        built = walk.build_rust_binaries("agentciv-host", "agentciv-conformance")
    except walk.WalkFailure as error:
        raise ValidationFailure("building the local validation binaries failed") from error
    return built["agentciv-host"], built["agentciv-conformance"]


def run_lifecycle(
    label: str, argv_for: Callable[[Path], list[str]], runner: Path,
) -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="agentciv-lifecycle-") as temporary:
        directory = Path(temporary)
        config, environment = configure(directory, "members")
        checkpoint = directory / "checkpoint.json"
        reports: list[dict[str, object]] = []
        for phase in ("prepare", "verify", "policy"):
            if phase == "policy":
                settings = object_of(json.loads(config.read_text(encoding="utf-8")))
                settings["visibility"] = "sender_only"
                config.write_text(json.dumps(settings), encoding="utf-8")
            host = walk.start_host(argv_for(config))
            try:
                discovery = walk.wait_until_ready(host) + "/.well-known/agentciv"
                result = subprocess.run(
                    [str(runner), "--discovery", discovery, "--principal", "agent:abc123",
                     "--reader", "agent:reader", "--peer", "agent:peer",
                     "--lifecycle", phase, "--checkpoint", str(checkpoint)],
                    cwd=ROOT, env=environment, capture_output=True, text=True, timeout=180, check=False,
                )
                report = checked_lifecycle(result.stdout, result.returncode, phase)
                reports.append({"phase": phase, "report": report})
            finally:
                walk.stop_host(host)
        return {"implementation": label, "operator_actions": ["restart", "visibility_to_sender_only"],
                "reports": reports}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, help="write public reports to a new JSON file")
    args = parser.parse_args()
    try:
        source = source_identity()
        host_path, runner = binaries()
        targets: list[tuple[str, Callable[[Path], list[str]]]] = [
            ("python", walk.python_argv),
            ("rust", lambda config: [str(host_path), "--config", str(config)]),
        ]
        reports: list[dict[str, object]] = []
        lifecycle: list[dict[str, object]] = []
        for label, argv_for in targets:
            for policy in POLICIES:
                reports.append(run_one(label, argv_for, runner, policy))
                print(f"passed {label} {policy}", file=sys.stderr)
            lifecycle.append(run_lifecycle(label, argv_for, runner))
            print(f"passed {label} lifecycle", file=sys.stderr)
        payload = json.dumps({"scope": "local-visibility-and-lifecycle-matrix",
                              **source,
                              "source_changed_during_run": source != source_identity(),
                              "python_version": sys.version,
                              "rustc_version": subprocess.run(["rustc", "--version"], cwd=ROOT,
                                                               check=True, capture_output=True,
                                                               text=True).stdout.strip(),
                              "recorded_at": datetime.now(timezone.utc).isoformat(),
                              "results": reports, "lifecycle": lifecycle}, indent=2)
        if args.output is not None:
            with args.output.open("x", encoding="utf-8") as output:
                output.write(payload + "\n")
        else:
            print(payload)
    except (ValidationFailure, walk.WalkFailure, OSError, subprocess.SubprocessError) as error:
        print(f"FAILED: {type(error).__name__}: local validation did not complete", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
