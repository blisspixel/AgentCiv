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
    "prefix.hidden",
    "collaborate.hidden_visibility",
    "collaborate.hidden_derivation",
    "collaborate.hidden_objection",
    "collaborate.hidden_decline",
    "collaborate.hidden_withdrawal",
}
LIFECYCLE_CASES = {
    "prepare": {"lifecycle.world", "lifecycle.collaboration", "lifecycle.fresh", "lifecycle.seed", "lifecycle.checkpoint"},
    "verify": {"lifecycle.world", "lifecycle.collaboration", "restart.history", "restart.cursor", "restart.retry", "restart.no_duplicate", "restart.revision_sequence", "restart.concurrent_revisions"},
    "policy": {"lifecycle.world", "lifecycle.collaboration", "policy.cursor_expired", "policy.visibility"},
    "revoke": {"lifecycle.world", "lifecycle.collaboration", "access.revoked_before_cursor", "access.writer_unchanged"},
    "restore": {"lifecycle.world", "lifecycle.collaboration", "access.restored_cursor_expired", "access.restored_history"},
    "remove": {"lifecycle.world", "lifecycle.collaboration", "access.removed_before_cursor", "access.writer_unchanged"},
    "rotate": {"lifecycle.world", "lifecycle.collaboration", "access.old_credential_rejected", "access.rotation_cursor_preserved", "access.restored_history"},
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
        optional = policy == "members" and case_id in HIDDEN_CASES
        if case.get("required") is not (not optional):
            raise ValidationFailure(f"case used the wrong required flag: {case_id}")
        if status == "failed" or (
            status == "skipped"
            and not (policy == "members" and case_id in HIDDEN_CASES and case.get("required") is False)
        ):
            raise ValidationFailure(f"case did not pass: {case_id}")
    expected = HIDDEN_CASES | {
        "submit.concurrent_retry", "submit.concurrent_conflict", "submit.concurrent_distinct",
        "collaborate.revision", "collaborate.objection", "collaborate.decline",
        "collaborate.revision_sequence", "collaborate.revision_sequence_retry", "collaborate.revision_sequence_rejection",
        "collaborate.concurrent_revisions", "collaborate.concurrent_revisions_retry", "collaborate.concurrent_revisions_isolation",
        "collaborate.concurrent_retry", "collaborate.concurrent_conflict", "collaborate.concurrent_retry_sequence",
        "prefix.baseline", "prefix.append", "prefix.withdrawal",
        "submit.failure_precedence", "submit.content_is_data", "collaborate.failure_precedence",
        "collaborate.retry_namespace", "collaborate.note_boundaries",
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
        if not isinstance(case_id, str) or case_id in seen or case.get("status") != "passed" or case.get("required") is not True:
            raise ValidationFailure("lifecycle case failed, duplicated, or was skipped")
        seen.add(case_id)
    if phase not in LIFECYCLE_CASES or seen != LIFECYCLE_CASES[phase]:
        raise ValidationFailure("lifecycle case inventory was incomplete")
    if report.get("summary") != {"passed": len(seen), "failed": 0, "skipped": 0}:
        raise ValidationFailure("lifecycle summary did not match its cases")
    return report


RETENTION_CASES = {"retention.setup", "retention.recorded", "retention.minimum_history", "retention.retry_inside", "retention.reuse_after", "retention.no_duplicate"}
INTERRUPTION_CASES = {
    "race-prepare": {"collaborate.concurrent_retry", "collaborate.concurrent_conflict", "collaborate.concurrent_retry_sequence", "race_recovery.checkpoint"},
    "race-verify": {"restart.same_id_retry", "restart.same_id_conflict"},
    "crash-prepare": {"crash.acknowledged", "crash.checkpoint", "crash.uncertain_dispatched"},
    "crash-verify": {"crash.acknowledged_history", "crash.uncertain_retry", "crash.no_duplicate"},
}


def checked_bounded(text: str, returncode: int, scope: str, expected: set[str]) -> dict[str, object]:
    try:
        parsed: object = json.loads(text)
    except json.JSONDecodeError as error:
        raise ValidationFailure("bounded observation did not emit JSON") from error
    report = object_of(parsed)
    if returncode != 0 or report.get("profile") != "http-commons/0.1-draft" or report.get("runner_scope") != scope:
        raise ValidationFailure("bounded observation failed or used another profile or scope")
    raw_cases = report.get("cases")
    if not isinstance(raw_cases, list):
        raise ValidationFailure("bounded observation omitted cases")
    seen: set[str] = set()
    for item in raw_cases:
        case = object_of(item)
        case_id = case.get("id")
        if not isinstance(case_id, str) or case_id in seen or case.get("status") != "passed" or case.get("required") is not True:
            raise ValidationFailure("bounded case failed, duplicated, or was skipped")
        seen.add(case_id)
    if seen != expected or report.get("summary") != {"passed": len(seen), "failed": 0, "skipped": 0}:
        raise ValidationFailure("bounded inventory or summary was incomplete")
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
        reader_credential: dict[str, object] | None = None
        for phase in ("prepare", "verify", "rotate", "revoke", "restore", "remove", "restore", "policy"):
            settings = object_of(json.loads(config.read_text(encoding="utf-8")))
            raw_credentials = settings.get("credentials")
            if not isinstance(raw_credentials, list):
                raise ValidationFailure("operator credentials must be a list")
            credentials = [object_of(item) for item in raw_credentials]
            current_reader = next((item for item in credentials if item.get("principal") == "agent:reader"), None)
            if phase == "rotate":
                if current_reader is None:
                    raise ValidationFailure("operator reader was missing")
                environment["AGENTCIV_CONFORMANCE_OLD_READER_TOKEN"] = environment["AGENTCIV_CONFORMANCE_READER_TOKEN"]
                replacement = secrets.token_urlsafe(24)
                environment["AGENTCIV_CONFORMANCE_READER_TOKEN"] = replacement
                current_reader["token"] = replacement
            elif phase == "revoke":
                if current_reader is None:
                    raise ValidationFailure("operator reader was missing")
                current_reader["read"] = False
                current_reader["write"] = True
            elif phase == "remove":
                if current_reader is None:
                    raise ValidationFailure("operator reader was missing")
                reader_credential = current_reader.copy()
                credentials = [item for item in credentials if item is not current_reader]
            elif phase == "restore":
                if current_reader is None:
                    if reader_credential is None:
                        raise ValidationFailure("removed operator reader was not saved")
                    current_reader = reader_credential.copy()
                    credentials.append(current_reader)
                current_reader["read"] = True
                current_reader["write"] = False
            if phase == "policy":
                settings["visibility"] = "sender_only"
            settings["credentials"] = credentials
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
        return {"implementation": label, "operator_actions": ["restart", "rotate_reader_credential", "reader_to_write_only", "restore_read_only", "remove_reader", "restore_reader", "visibility_to_sender_only"],
                "reports": reports}


def run_retention(label: str, argv_for: Callable[[Path], list[str]], runner: Path) -> dict[str, object]:
    with tempfile.TemporaryDirectory(prefix="agentciv-retention-") as temporary:
        config, environment = configure(Path(temporary), "members")
        settings = object_of(json.loads(config.read_text(encoding="utf-8")))
        settings["retention_seconds"] = 8
        config.write_text(json.dumps(settings), encoding="utf-8")
        host = walk.start_host(argv_for(config))
        try:
            discovery = walk.wait_until_ready(host) + "/.well-known/agentciv"
            result = subprocess.run([str(runner), "--discovery", discovery, "--principal", "agent:abc123", "--reader", "agent:reader", "--retention-window-test"], cwd=ROOT, env=environment, capture_output=True, text=True, timeout=60, check=False)
            report = checked_bounded(result.stdout, result.returncode, "bounded-retention", RETENTION_CASES)
            return {"implementation": label, "operator_actions": ["fresh_disposable_world", "retention_seconds_8", "steady_local_clock"], "report": report}
        finally:
            walk.stop_host(host)


def run_interruption(label: str, argv_for: Callable[[Path], list[str]], runner: Path, policy: str, family: str) -> dict[str, object]:
    if family not in ("race", "crash"):
        raise ValidationFailure("unknown interruption condition")
    with tempfile.TemporaryDirectory(prefix="agentciv-race-recovery-") as temporary:
        directory = Path(temporary)
        config, environment = configure(directory, policy)
        checkpoint = directory / "race.json"
        reports: list[dict[str, object]] = []
        for phase in (f"{family}-prepare", f"{family}-verify"):
            host = walk.start_host(argv_for(config))
            try:
                discovery = walk.wait_until_ready(host) + "/.well-known/agentciv"
                result = subprocess.run([str(runner), "--discovery", discovery, "--principal", "agent:abc123", "--reader", "agent:reader", "--peer", "agent:peer", "--lifecycle", phase, "--checkpoint", str(checkpoint)], cwd=ROOT, env=environment, capture_output=True, text=True, timeout=120, check=False)
                report = checked_bounded(result.stdout, result.returncode, phase.replace(f"{family}-", f"{family}-recovery-"), INTERRUPTION_CASES[phase])
                reports.append({"phase": phase, "report": report})
                if phase.endswith("-prepare"):
                    host.process.kill()
                    host.process.wait(timeout=10)
            finally:
                walk.stop_host(host)
        action = "process_kill_after_acknowledged_same_id_races" if family == "race" else "process_kill_after_dispatched_unacknowledged_requests"
        return {"implementation": label, "visibility": policy, "operator_actions": [action, "restart_same_storage_and_policy"], "reports": reports}


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
        retention: list[dict[str, object]] = []
        races: list[dict[str, object]] = []
        crashes: list[dict[str, object]] = []
        for label, argv_for in targets:
            for policy in POLICIES:
                reports.append(run_one(label, argv_for, runner, policy))
                print(f"passed {label} {policy}", file=sys.stderr)
                races.append(run_interruption(label, argv_for, runner, policy, "race"))
                print(f"passed {label} {policy} race recovery", file=sys.stderr)
                crashes.append(run_interruption(label, argv_for, runner, policy, "crash"))
                print(f"passed {label} {policy} uncertain-write recovery", file=sys.stderr)
            lifecycle.append(run_lifecycle(label, argv_for, runner))
            print(f"passed {label} lifecycle", file=sys.stderr)
            retention.append(run_retention(label, argv_for, runner))
            print(f"passed {label} bounded retention", file=sys.stderr)
        payload = json.dumps({"scope": "local-visibility-and-lifecycle-matrix",
                              **source,
                              "source_changed_during_run": source != source_identity(),
                              "python_version": sys.version,
                              "rustc_version": subprocess.run(["rustc", "--version"], cwd=ROOT,
                                                               check=True, capture_output=True,
                                                               text=True).stdout.strip(),
                              "recorded_at": datetime.now(timezone.utc).isoformat(),
                              "results": reports, "lifecycle": lifecycle, "retention": retention, "race_recovery": races, "crash_recovery": crashes}, indent=2)
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
