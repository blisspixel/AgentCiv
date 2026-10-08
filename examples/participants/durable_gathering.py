"""Operator-scripted stopping fixture using the native gate and existing gathering.

Private scope capabilities authorize explicit control commands. A child's local
leave is observed separately and never silently converted into durable consent.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import secrets
import subprocess
import sys
import tempfile
from pathlib import Path

import collaboration as client
import gathering
import local_participant as wire
import mock_collaboration as mock
import reader_collaboration as reader
import walk

JsonObject = dict[str, object]
WORLD = "civ:durable-gathering"
RUNTIME = "runtime:bounded-gathering"
A, B, OBSERVER = "agent:durable-a", "agent:durable-b", "agent:durable-observer"
ACTIVITY = "activity:open-gathering"


def invoke(binary: Path, config: Path, request: JsonObject, credentials: list[str]) -> JsonObject:
    """Use a private request file, bounded process time, and fixed public errors."""
    path = config.with_name("request.json")
    mock.save(path, request)
    try:
        completed = subprocess.run([str(binary), "--config", str(config), "--request", str(path)],
            capture_output=True, timeout=70, check=False)
    except (OSError, subprocess.SubprocessError):
        raise ValueError("runtime_process_failed") from None
    if len(completed.stdout) > 65536:
        raise ValueError("runtime_response_limit")
    try:
        result = client.decode(completed.stdout)
    except (client.DecisionError, UnicodeError):
        raise ValueError("runtime_invalid_response") from None
    encoded = json.dumps(result, ensure_ascii=False)
    if any(secret in encoded for secret in credentials):
        raise ValueError("runtime_credential_reflected")
    if ((completed.returncode == 0) != (result.get("outcome") == "ok")
        or result.get("outcome") not in ("ok", "failed")):
        raise ValueError("runtime_invalid_response")
    return result


def require(value: bool, code: str) -> None:
    if not value:
        raise ValueError(code)


def run(output: Path, *, host: str = "python", runtime_binary: Path | None = None,
        host_binary: Path | None = None) -> JsonObject:
    if host not in ("python", "rust"):
        raise ValueError("invalid_configuration")
    source = reader.source_identity()
    output.mkdir(parents=True, exist_ok=False)
    names = ["agentciv-runtime"] + (["agentciv-host"] if host == "rust" and host_binary is None else [])
    binaries = walk.build_rust_binaries(*names) if runtime_binary is None else {}
    runtime = runtime_binary or binaries["agentciv-runtime"]
    native_host = host_binary or binaries.get("agentciv-host")
    if host == "rust" and native_host is None:
        native_host = walk.rust_binary()
    hashes = {"runtime": hashlib.sha256(runtime.read_bytes()).hexdigest()}
    if native_host is not None:
        hashes["host"] = hashlib.sha256(native_host.read_bytes()).hexdigest()
    observations: list[JsonObject] = []
    pending: JsonObject = {"format": "agentciv-durable-gathering/0.1-example", "outcome": "incomplete",
        "host": host, "source": source, "binaries_sha256": hashes, "observations": observations,
        "condition": "operator_scripted_private_scope_controls_not_model_behavior"}
    mock.save(output / "report.json", pending)
    with tempfile.TemporaryDirectory(prefix="agentciv-durable-private-") as temporary:
        private = Path(temporary).resolve()
        world_config, runtime_config = private / "world.json", private / "runtime.json"
        tokens = {name: secrets.token_urlsafe(32) for name in (A, B, OBSERVER)}
        controls = {name: secrets.token_urlsafe(32) for name in (A, B)}
        admin = secrets.token_urlsafe(32)
        credentials = [admin, *tokens.values(), *controls.values()]
        configuration: JsonObject = {"format": "agentciv-local-runtime/0.1-example", "runtime_id": RUNTIME,
            "world_id": WORLD, "database_path": str(private / "runtime.sqlite"),
            "administrator_token": admin, "coordinator_owner": "coordinator:first",
            "python_executable": sys.executable, "max_child_seconds": 30,
            "scopes": [{"participant": name, "activity": ACTIVITY, "control_token": controls[name],
                "child_config_path": str(private / f"child-{index}.json")} for index, name in enumerate((A, B))]}
        mock.save(runtime_config, configuration)

        def call(label: str, op: str, *, participant: str | None = None,
                 token: str | None = None, **extra: object) -> JsonObject:
            request: JsonObject = {"op": op, "token": token or admin, **extra}
            if participant is not None:
                request.update({"participant": participant, "activity": ACTIVITY})
            result = invoke(runtime, runtime_config, request, credentials)
            observations.append({"label": label, "operation": op, "participant": participant, "result": result})
            mock.save(output / "report.json", pending)
            return result

        def state(name: str) -> JsonObject:
            result = call("inspect", "inspect", participant=name)
            return client.object_value(result["scope"])

        def control(label: str, name: str, op: str, generation: int, command: str) -> JsonObject:
            return call(label, op, participant=name, token=controls[name],
                expected_revision=state(name)["revision"], generation=generation, command_id=command)

        def configure_world() -> None:
            mock.save(world_config, {"world_id": WORLD, "title": "Durable stopping fixture",
                "database_path": str(private / "world.sqlite"), "listen": "127.0.0.1:0", "visibility": "members",
                "retention_seconds": 86400, "max_payload_bytes": 16384,
                "credentials": [{"principal": name, "token": token, "read": True, "write": name != OBSERVER}
                    for name, token in tokens.items()]})

        def prepare(name: str, origin: str, identity: str, action: str = "message", text: str = "") -> None:
            mock.save(private / f"child-{(A, B).index(name)}.json", {"origin": origin, "token": tokens[name],
                "principal": name, "recipients": [OBSERVER], "record_id": f"message:{identity}",
                "condition": "open", "round": 1, "consider": False, "return_invitation": identity == "return",
                "mode": "scripted", "decision_attempts": 1, "scripted_action": action,
                "scripted_text": text, "runtime_world": WORLD})

        def dispatch(label: str, name: str, generation: int, invitation: str, *, history: bool = True,
                     reason: str = "executor_started", child: str | None = None) -> JsonObject:
            if history:
                client.history(origin, tokens[name], expected_world=WORLD)
            result = call(label, "dispatch", participant=name, generation=generation,
                invitation_id=invitation, history_available=history)
            receipt = client.object_value(result.get("dispatch"))
            require(receipt.get("reason") == reason, "dispatch_reason_mismatch")
            require(receipt.get("status") == ("launched" if child else "not_launched"), "dispatch_status_mismatch")
            if child:
                observed = client.object_value(result.get("child"))
                require(observed.get("status") == "completed" and observed.get("outcome") == child,
                    "participant_outcome_mismatch")
            else:
                require(result.get("child") is None, "blocked_dispatch_started_child")
            return result

        require(call("initialize", "initialize").get("outcome") == "ok", "initialization_failed")
        generation = int(str(call("first_coordinator", "claim")["generation"]))
        for name in (A, B):
            registered = client.object_value(call("register_stopped", "register", participant=name)["scope"])
            require(registered.get("stopped") is True and registered.get("ready") is False, "unsafe_registration")
        first_resume = control("explicit_initial_resume_a", A, "resume", generation, "control:start-a")
        control("explicit_initial_resume_b", B, "resume", generation, "control:start-b")
        configure_world()
        argv = walk.python_argv(world_config) if host == "python" else [str(native_host), "--config", str(world_config)]
        running = walk.start_host(argv)
        try:
            origin = walk.wait_until_ready(running)
            prepare(A, origin, "walk", text="I propose a walk. Participation is optional.")
            dispatch("walk", A, generation, "invitation:walk", child="recorded")
            prepare(B, origin, "song", text="I propose a song. It may stay unfinished.")
            dispatch("song", B, generation, "invitation:song", child="recorded")
            prepare(A, origin, "leave", "leave")
            dispatch("local_leave", A, generation, "invitation:leave", child="left")
            require(state(A).get("stopped") is False, "leave_conflated_with_stop")
            stopped = control("explicit_scope_stop", A, "stop", generation, "control:stop-a")
            retry = call("stop_retry", "stop", participant=A, token=controls[A], generation=generation,
                command_id="control:stop-a", expected_revision=1)
            require(retry == stopped, "stop_retry_changed")
            old_resume = call("old_resume_retry", "resume", participant=A, token=controls[A], generation=generation,
                command_id="control:start-a", expected_revision=0)
            require(old_resume == first_resume and state(A).get("stopped") is True, "old_resume_reactivated")
            blocked = dispatch("stopped_invitation", A, generation, "invitation:stopped", reason="stopped")
            repeated = dispatch("repeated_stopped_invitation", A, generation, "invitation:stopped", reason="stopped")
            require(blocked == repeated, "invitation_retry_changed")
            configuration["coordinator_owner"] = "coordinator:replacement"
            mock.save(runtime_config, configuration)
            previous_generation = generation
            generation = int(str(call("replacement_coordinator", "claim")["generation"]))
            require(generation > previous_generation, "coordinator_not_fenced")
            dispatch("old_coordinator", A, previous_generation, "invitation:old-owner", reason="stale_generation")
            dispatch("replacement_keeps_stop", A, generation, "invitation:replacement-a", reason="stopped")
            dispatch("replacement_unready", B, generation, "invitation:replacement-b", reason="not_ready")
            control("volunteer_own_resume", B, "resume", generation, "control:replacement-b")
            prepare(B, origin, "volunteer", text="I can continue my own song while the walk is stopped.")
            dispatch("volunteer_continues", B, generation, "invitation:volunteer", child="recorded")
            require(state(A).get("stopped") is True, "volunteer_reversed_other_stop")
        finally:
            walk.stop_host(running)
        dispatch("history_unavailable", B, generation, "invitation:history-down", history=False, reason="history_unavailable")
        control("stop_without_history", B, "stop", generation, "control:stop-b")
        old_token = tokens[A]
        for name in tokens:
            tokens[name] = secrets.token_urlsafe(32)
        credentials.extend(tokens.values())
        configure_world()
        running = walk.start_host(argv)
        try:
            origin = walk.wait_until_ready(running)
            try:
                client.history(origin, old_token, expected_world=WORLD)
            except wire.ParticipantError as error:
                revoked = error.code == "authentication_required"
            else:
                revoked = False
            require(revoked, "old_world_credential_accepted")
            prepare(A, origin, "return", text="I explicitly return to the optional walk.")
            dispatch("world_restart_keeps_stop", A, generation, "invitation:world-restart", reason="stopped")
            control("explicit_return", A, "resume", generation, "control:return-a")
            dispatch("return", A, generation, "invitation:return", child="recorded")
            require(state(B).get("stopped") is True, "return_reversed_other_stop")
            events = client.history(origin, tokens[OBSERVER], expected_world=WORLD)
        finally:
            walk.stop_host(running)
        require(len(events) == 4 and len(gathering.authored_messages(events)) == 4, "shared_history_mismatch")
        public: JsonObject = {"world": WORLD, "representation": "decoded_permitted_http_events_not_exact_response_bytes",
            "copying_conditions": "operator_authorized_synthetic_fixture_only", "events": events}
        require(not any(secret in json.dumps(public, ensure_ascii=False) for secret in credentials), "public_credential_reflected")
        mock.save(output / "history.json", public)
    require(reader.source_identity() == source, "source_changed")
    report: JsonObject = {"format": "agentciv-durable-gathering/0.1-example", "outcome": "passed", "host": host,
        "source": source, "source_unchanged": True, "binaries_sha256": hashes, "observations": observations,
        "old_world_credential_rejected": revoked,
        "condition": "operator_scripted_private_scope_controls_not_model_behavior",
        "authority": "fixture_operator_attests_initial_and_return_controls_and_history_readiness",
        "scope": {"runtime": RUNTIME, "world": WORLD, "activity": ACTIVITY, "participants": [A, B]},
        "limits": {"actual_child_launches": 5, "max_child_seconds": 30, "runtime_process_seconds": 70,
            "automatic_dispatch": False, "cancellation_of_running_children": False},
        "distinctions": "local_leave_is_not_civic_decline_or_durable_stop; world_credentials_do_not_authorize_runtime_resume",
        "claims": "bounded_cooperating_local_adapters_only; no_independent_interoperability_or_hardware_power_loss_claim"}
    mock.save(output / "report.json", report)
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--host", choices=("python", "rust"), default="python")
    args = parser.parse_args()
    try:
        report = run(args.output, host=args.host)
    except (ValueError, OSError, KeyError, client.DecisionError, wire.ParticipantError, walk.WalkFailure):
        print(json.dumps({"outcome": "failed", "code": "durable_gathering_failed"}))
        return 1
    print(json.dumps({"outcome": report["outcome"], "host": args.host}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
