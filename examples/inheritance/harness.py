"""Compose offline archives and a separately evaluated newcomer exercise."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / "examples" / "http-commons"))

import walk
import newcomer
import oracle

JsonObject = dict[str, object]


def save(path: Path, value: object) -> None:
    with path.open("x", encoding="utf-8") as target:
        target.write(json.dumps(value, indent=2, ensure_ascii=False, allow_nan=False))


def baseline(name: str, events: list[JsonObject]) -> JsonObject:
    """Adapt only the disclosed scripted baseline's citations to host-assigned IDs."""
    plan = oracle.validate_plan(oracle.load_json(HERE / name))
    by_key: dict[tuple[str, str, int], str] = {}
    for event in events:
        artifact = oracle.artifact(event)
        if artifact is not None:
            by_key[(str(artifact["from"]), str(artifact["artifact_id"]),
                int(str(artifact["revision"])))] = str(event["id"])
    claims = plan["claims"]
    if isinstance(claims, list):
        for claim in claims:
            source = oracle.object_value(oracle.object_value(claim)["source"])
            key = (str(source["from"]), str(source["artifact_id"]), int(str(source["revision"])))
            source["event_id"] = by_key.get(key, str(source["event_id"]))
            # object_value creates a shallow dict, so update the actual validated record.
            if isinstance(claim, dict):
                claim["source"] = source
    return plan


def source_identity() -> JsonObject:
    commit = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT,
        capture_output=True, text=True, timeout=10, check=False)
    tracked = subprocess.run(["git", "diff", "HEAD", "--quiet"], cwd=ROOT,
        capture_output=True, timeout=10, check=False)
    pending = subprocess.run(["git", "ls-files", "--others", "--exclude-standard", "--",
        "tools/agentciv-archive", "examples/inheritance", "schemas"], cwd=ROOT,
        capture_output=True, timeout=10, check=False)
    paths = [ROOT / name for name in ("Cargo.toml", "Cargo.lock", "pyproject.toml")]
    for directory in ("tools/agentciv-archive", "examples/inheritance", "examples/participants",
                      "examples/http-commons", "implementations/http-commons-python", "schemas"):
        paths.extend(path for path in (ROOT / directory).rglob("*")
            if path.is_file() and path.suffix in (".py", ".rs", ".json", ".toml"))
    hashes = {path.relative_to(ROOT).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in sorted(paths)}
    return {"commit": commit.stdout.strip() if commit.returncode == 0 else None,
        "tracked_source_changed": tracked.returncode != 0,
        "new_source_files": bool(pending.stdout), "file_sha256": hashes,
        "independent_reproduction": False}


def run(output: Path, *, mode: str, source: str, model: str | None = None,
        seed: int = 42, attempts: int = 1, private_traces: bool = False) -> JsonObject:
    output.mkdir(parents=True, exist_ok=False)
    report: JsonObject = {"format": "agentciv-offline-inheritance-experiment/0.1",
        "mode": mode, "source": source, "source_identity": source_identity(),
        "source_choices": "operator_supplied_synthetic_records",
        "model_result_is_scripted_fallback": False, "outcome": "not_started",
        "controls": {"model": model, "seed": seed, "attempts": attempts,
            "output_tokens": 1024, "decision_seconds": 120, "process_seconds": 180,
            "private_traces": private_traces}}
    stage = "archive_build"
    try:
        binary = walk.build_rust_binaries("agentciv-archive")["agentciv-archive"]
        report["archive_binary_sha256"] = hashlib.sha256(binary.read_bytes()).hexdigest()
        stage = "source_history"
        if source == "fixture":
            events = oracle.validate_events(oracle.load_json(HERE / "history.json"))
        elif source == "host":
            import host_source
            with tempfile.TemporaryDirectory(prefix="agentciv-inheritance-source-") as temporary:
                try:
                    events = host_source.create_history(Path(temporary))
                except host_source.HostSourceError as error:
                    raise ValueError("host_source_failed") from error
        else:
            raise ValueError("unknown_source")
        save(output / "history.json", events)
        before = baseline("before.json", events)
        after = baseline("after.json", events)
        before_check = oracle.evaluate(events, before)
        after_check = oracle.evaluate(events, after)
        save(output / "before-check.json", before_check)
        save(output / "scripted-after-check.json", after_check)
        if before_check["useful_continuation"] is not False or after_check["useful_continuation"] is not True:
            raise ValueError("broken_acceptance_baseline")
        stage = "archive_export_and_readback"
        snapshot = output / "snapshot.json"
        permit = output / "selection.json"
        save(snapshot, {"world": events[0]["world"], "records": [
            json.dumps(event, ensure_ascii=False, separators=(",", ":")) for event in events]})
        save(permit, {"world": events[0]["world"], "audience": ["agent:newcomer"],
            "event_ids": [event["id"] for event in events]})
        bundle = newcomer.archive(binary, "export", snapshot, permit)
        bundle_path = output / "bundle.json"
        save(bundle_path, bundle)
        originals, view = newcomer.load_bundle(binary, bundle_path)
        if originals != events:
            raise ValueError("copy_changed_originals")
        save(output / "inspection.json", view)
        report.update({"before": before_check, "scripted_after": after_check,
            "bundle_sha256": hashlib.sha256(bundle_path.read_bytes()).hexdigest(),
            "original_records_preserved": True,
            "copying_permission": "operator_assertion_for_this_synthetic_fixture_not_a_portable_grant",
            "private_source_config_exported": False, "no_live_host_needed_by_newcomer": True})
        if mode == "scripted":
            decision: JsonObject = {"action": "continue", "reason": "Disclosed scripted baseline.", "plan": after}
            report["decision_source"] = "scripted_after_baseline"
        elif mode == "ollama":
            if model is None:
                raise ValueError("model_required")
            command = [sys.executable, str(HERE / "newcomer.py"), "--archive-binary", str(binary),
                "--bundle", str(bundle_path), "--output", str(output / "newcomer"), "--model", model,
                "--seed", str(seed), "--attempts", str(attempts)]
            if private_traces:
                command.append("--private-traces")
            stage = "newcomer_process"
            process = subprocess.run(command, cwd=ROOT, capture_output=True, timeout=180, check=False)
            newcomer_report = oracle.object_value(oracle.load_json(output / "newcomer" / "report.json"))
            report["newcomer"] = newcomer_report
            report["decision_source"] = "local_model_separate_process"
            if process.returncode != 0:
                raise ValueError("newcomer_failed")
            decision = oracle.object_value(newcomer_report["decision"])
        else:
            raise ValueError("unknown_mode")
        stage = "independent_acceptance"
        save(output / "decision.json", decision)
        if decision["action"] == "stop":
            report.update({"outcome": "stopped", "useful_continuation": False})
        else:
            check = oracle.evaluate(events, oracle.object_value(decision["plan"]))
            save(output / "newcomer-check.json", check)
            report.update({"outcome": "evaluated", "useful_continuation": check["useful_continuation"],
                "newcomer_check": check})
        report["original_bundle_unchanged"] = (
            hashlib.sha256(bundle_path.read_bytes()).hexdigest() == report["bundle_sha256"])
        if report["original_bundle_unchanged"] is not True:
            raise ValueError("bundle_changed")
    except (ValueError, OSError, subprocess.SubprocessError, newcomer.NewcomerError, walk.WalkFailure):
        report["outcome"] = "failed"
        report["useful_continuation"] = False
        report["failure_stage"] = stage
        report["failure_code"] = "stage_failed"
    end_identity = source_identity()
    report["source_changed_during_run"] = (
        oracle.object_value(report["source_identity"]).get("file_sha256") != end_identity.get("file_sha256")
        or oracle.object_value(report["source_identity"]).get("commit") != end_identity.get("commit"))
    if report["source_changed_during_run"] is not False:
        report["outcome"] = "failed"
        report["useful_continuation"] = False
        report["failure_stage"] = "source_identity"
        report["failure_code"] = "source_changed_during_run"
    save(output / "report.json", report)
    return copy.deepcopy(report)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--mode", choices=("scripted", "ollama"), default="scripted")
    parser.add_argument("--source", choices=("fixture", "host"), default="fixture")
    parser.add_argument("--model")
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--attempts", type=int, choices=(1, 2, 3), default=1)
    parser.add_argument("--private-traces", action="store_true")
    args = parser.parse_args()
    report = run(args.output, mode=args.mode, source=args.source, model=args.model,
        seed=args.seed, attempts=args.attempts, private_traces=args.private_traces)
    print(json.dumps({key: report.get(key) for key in ("outcome", "mode", "source", "useful_continuation")}, indent=2))
    return 0 if report.get("outcome") == "evaluated" and report.get("useful_continuation") is True else 1


if __name__ == "__main__":
    raise SystemExit(main())
