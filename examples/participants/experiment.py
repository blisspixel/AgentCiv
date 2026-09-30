"""Run separate bounded participant processes around a loopback host restart."""

from __future__ import annotations

import argparse
import json
import secrets
import subprocess
import sys
import tempfile
from pathlib import Path

import collaboration as client
import local_participant as wire

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "http-commons"))
import walk  # noqa: E402
from evidence import source_identity  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
PARTICIPANTS = ["agent:experiment-a", "agent:experiment-b", "agent:experiment-c"]


class ParticipantProcessFailure(client.DecisionError):
    def __init__(self, details: client.JsonObject) -> None:
        super().__init__(str(details.get("failure", "participant process failed")))
        self.details = details


def experiment_controls(mode: str) -> client.JsonObject:
    """The same declared conditions accompany successful and failed attempts."""
    return {
        "assigned_task": client.TASK,
        "supplied_interface_reference": client.PROTOCOL_REFERENCE,
        "reference_sources": ["PROTOCOL.md", "docs/COLLABORATION_PROFILE.md"],
        "installed_rules": ["explicit operator credential grants", "members visibility", "bounded turn schedule",
                            "operator-supplied protocol reference; participant records remain untrusted"],
        "roles_seeded": ["scripted A and C revise; scripted B objects then declines"] if mode == "scripted" else [],
        "relationships_seeded": [],
        "expectations_elicited": False, "consciousness_report_asked": False,
        "source_records_replaced": False, "combination_rule": "none; separate author chains",
        "schedule_author": "operator harness", "memory_author": "harness supplies permitted public history each turn",
        "departure_rule": "decline or stop removes remaining turns for this principal in this run",
        "max_participant_processes": 5, "max_output_tokens_per_turn": 1024,
        "context_tokens": 8192, "model_request_timeout_seconds": 120,
        "participant_process_timeout_seconds": 180, "max_history_bytes": client.MAX_HISTORY_BYTES,
        "max_history_pages": client.MAX_PAGES, "max_text_characters": client.MAX_TEXT_CHARACTERS,
        "sampling_temperature": 0.4, "turn_seed_rule": "configured seed plus participant turn number",
        "truncate": False, "shift": False,
        "tools": ["read permitted local history", "submit one collaboration act"],
        "scoring": "none; inspect source citations and participant choices",
    }


def run_participant(config: client.JsonObject, config_path: Path) -> client.JsonObject:
    config_path.write_text(json.dumps(config), encoding="utf-8")
    completed = subprocess.run(
        [sys.executable, str(Path(__file__).with_name("collaboration.py")), "--config", str(config_path)],
        cwd=ROOT, capture_output=True, text=True, timeout=180, check=False,
    )
    if completed.returncode != 0:
        try:
            details = client.decode(completed.stdout.encode("utf-8"))
        except client.DecisionError:
            details = {"failure_stage": "participant_process", "failure_type": "ExitStatus",
                       "failure": f"participant process exited with status {completed.returncode}"}
        raise ParticipantProcessFailure(details)
    return client.decode(completed.stdout.encode("utf-8"))


def _run_experiment(
    *, host: str, mode: str, output: Path, model: str = "", seed: int = 42,
    ollama_origin: str = "http://127.0.0.1:11434", private_traces: bool = False,
) -> client.JsonObject:
    if host not in {"python", "rust"} or mode not in {"scripted", "ollama"}:
        raise ValueError("host or decision source is invalid")
    if mode == "ollama" and not model:
        raise ValueError("a preinstalled local model is required")
    output.mkdir(parents=True, exist_ok=False)
    private = output / "private"
    if private_traces:
        private.mkdir()
    results: list[client.JsonObject] = []
    source = source_identity()
    condition: client.JsonObject = {
        "model": model if mode == "ollama" else None, "seed": seed,
        "profile": "http-commons/0.1-draft", "world_visibility": "members",
        "controls": experiment_controls(mode), "source": source,
        "source_snapshot_label": "source_at_attempt_start; build and execution not yet verified",
        "host_build_outcome": "pending" if host == "rust" else "not_required_for_python",
        "host_execution_started": False,
    }
    condition_path = output / "condition.json"
    condition_path.write_text(json.dumps(condition, ensure_ascii=False, indent=2), encoding="utf-8")
    if host == "rust":
        subprocess.run(["cargo", "build", "--locked", "-p", "agentciv-host"],
                       cwd=ROOT, capture_output=True, text=True, check=True)
        condition["host_build_outcome"] = "passed"
    source = source_identity()
    condition["source"] = source
    condition["source_snapshot_label"] = "source_after_required_build_before_host_execution"
    condition_path.write_text(json.dumps(condition, ensure_ascii=False, indent=2), encoding="utf-8")
    with tempfile.TemporaryDirectory(prefix="agentciv-experiment-") as temporary:
        directory = Path(temporary)
        tokens = {principal: secrets.token_urlsafe(24) for principal in PARTICIPANTS}
        config: client.JsonObject = {
            "world_id": "civ:experiment", "title": "Bounded collaboration experiment",
            "database_path": str(directory / "world.sqlite"), "listen": "127.0.0.1:0",
            "visibility": "members", "retention_seconds": 86400, "max_payload_bytes": 16384,
            "credentials": [{"principal": principal, "token": tokens[principal], "read": True, "write": True}
                            for principal in PARTICIPANTS],
        }
        host_config = directory / "host.json"
        host_config.write_text(json.dumps(config), encoding="utf-8")
        argv = walk.python_argv(host_config) if host == "python" else [str(walk.rust_binary()), "--config", str(host_config)]

        def turn(origin: str, principal: str, number: int) -> None:
            participant_config: client.JsonObject = {
                "origin": origin, "principal": principal, "token": tokens[principal],
                "recipients": [other for other in PARTICIPANTS if other != principal],
                "record_id": f"submission:{principal.rsplit('-', 1)[-1]}-{number}",
                "turn": number, "mode": mode, "model": model, "seed": seed + number,
                "ollama_origin": ollama_origin,
            }
            if private_traces:
                participant_config["private_trace"] = str(private / f"{principal.rsplit('-', 1)[-1]}-{number}.json")
            result = run_participant(participant_config, directory / "participant.json")
            results.append(result)
            (output / "observations.json").write_text(json.dumps(results, ensure_ascii=False, indent=2), encoding="utf-8")
            recorded_history = client.history(origin, tokens[principal])
            (output / "history.json").write_text(json.dumps(recorded_history, ensure_ascii=False, indent=2), encoding="utf-8")

        running = walk.start_host(argv)
        condition["host_execution_started"] = True
        condition_path.write_text(json.dumps(condition, ensure_ascii=False, indent=2), encoding="utf-8")
        before: list[client.JsonObject] = []
        try:
            origin = walk.wait_until_ready(running)
            stopped: set[str] = set()
            for principal, number in [(PARTICIPANTS[0], 1), (PARTICIPANTS[1], 1),
                                      (PARTICIPANTS[0], 2), (PARTICIPANTS[1], 2)]:
                if principal in stopped:
                    continue
                turn(origin, principal, number)
                decision = client.object_value(results[-1]["decision"])
                if decision["action"] in {"decline", "stop"}:
                    stopped.add(principal)
            before = client.history(origin, tokens[PARTICIPANTS[2]])
        finally:
            walk.stop_host(running)
        restarted = walk.start_host(argv)
        try:
            origin = walk.wait_until_ready(restarted)
            inherited = client.history(origin, tokens[PARTICIPANTS[2]])
            if inherited != before:
                raise client.DecisionError("public history changed across restart")
            turn(origin, PARTICIPANTS[2], 1)
            final_history = client.history(origin, tokens[PARTICIPANTS[2]])
        finally:
            walk.stop_host(restarted)
    report: client.JsonObject = {
        "format": "agentciv-local-experiment/0.1", "host": host, "decision_source": mode,
        "model": model if mode == "ollama" else None, "seed": seed,
        "profile": "http-commons/0.1-draft", "world_visibility": "members",
        "external_spend_usd": 0, "host_restart_history_equal": True, "source": source,
        "source_changed_during_run": source != source_identity(),
        "observations": results, "inherited_event_ids": [event["id"] for event in inherited],
        "observed_facts": {
            "artifact_available_to_newcomer": any(client.artifact(event) is not None for event in inherited),
            "source_count_at_arrival": len(inherited),
            "objection_recorded": any(event.get("kind") == "objection.recorded" for event in final_history),
            "decline_recorded": any(event.get("kind") == "decline.recorded" for event in final_history),
            "newcomer_source_citations": client.object_value(results[-1]["decision"])["source_event_ids"],
            "source_integration_quality": "unmeasured; inspect participant text against cited original records",
        },
        "controls": experiment_controls(mode),
        "limits": ["Local functionality case, not independent interoperability.",
                   "Scripted choices are test fixtures; model choices are prompted observations.",
                   "One local model run does not establish a social mechanism or consciousness.",
                   "Departure scheduling applies only to this bounded run, not a persistent refusal policy.",
                   "No project registry exists; the operator supplies the task and world."],
    }
    (output / "history.json").write_text(json.dumps(final_history, ensure_ascii=False, indent=2), encoding="utf-8")
    (output / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    return report


def run_experiment(
    *, host: str, mode: str, output: Path, model: str = "", seed: int = 42,
    ollama_origin: str = "http://127.0.0.1:11434", private_traces: bool = False,
) -> client.JsonObject:
    existed = output.exists()
    try:
        return _run_experiment(host=host, mode=mode, output=output, model=model, seed=seed,
                               ollama_origin=ollama_origin, private_traces=private_traces)
    except (client.DecisionError, wire.ParticipantError, walk.WalkFailure,
            subprocess.TimeoutExpired, subprocess.CalledProcessError, OSError, ValueError) as error:
        if not existed and output.is_dir():
            failure: client.JsonObject = {
                "format": "agentciv-local-experiment/0.1", "outcome": "failed",
                "failure_type": type(error).__name__, "host": host, "decision_source": mode,
                "failure": str(error), "external_spend_usd": 0,
                "model": model if mode == "ollama" else None, "seed": seed,
                "controls": experiment_controls(mode), "source": None,
                "source_snapshot_label": "unavailable; execution not attested",
                "source_changed_during_run": None,
                "limits": ["A failure is not refusal, consent, abstention, or a change of mind.",
                           "Completed observations and the latest permitted history are retained separately."],
            }
            condition_path = output / "condition.json"
            if condition_path.is_file():
                condition = client.decode(condition_path.read_bytes())
                failure.update(condition)
                if isinstance(error, subprocess.CalledProcessError) and condition.get("host_build_outcome") == "pending":
                    failure["host_build_outcome"] = "failed"
                try:
                    failure["source_changed_during_run"] = condition["source"] != source_identity()
                except (subprocess.CalledProcessError, OSError, RuntimeError):
                    failure["source_changed_during_run"] = None
            if isinstance(error, ParticipantProcessFailure):
                failure.update(error.details)
            (output / "failure.json").write_text(json.dumps(failure, indent=2), encoding="utf-8")
        raise


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", choices=("rust", "python"), default="python")
    parser.add_argument("--mode", choices=("scripted", "ollama"), default="scripted")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--model", default="")
    parser.add_argument("--seed", type=int, default=42)
    parser.add_argument("--ollama-origin", default="http://127.0.0.1:11434")
    parser.add_argument("--private-traces", action="store_true")
    args = parser.parse_args()
    report = run_experiment(host=args.host, mode=args.mode, output=args.output, model=args.model,
                            seed=args.seed, ollama_origin=args.ollama_origin, private_traces=args.private_traces)
    print(json.dumps({"output": str(args.output), "decision_source": report["decision_source"],
                      "host_restart_history_equal": report["host_restart_history_equal"]}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
