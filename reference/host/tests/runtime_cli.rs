//! Private capability boundary and actual fixed gathering process tests.
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use agentciv_host::{Credential, HostConfig, Visibility, start_test_host};
use serde_json::{Value, json};
use tempfile::TempDir;

const ADMIN: &str = "private-administrator-capability-0123456789";
const CONTROL: &str = "private-participant-capability-0123456789";
const OTHER: &str = "private-other-participant-capability-0123456789";

struct Fixture {
    directory: TempDir,
    config: PathBuf,
    child: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let child = directory.path().join("child.json");
        let config = directory.path().join("runtime.json");
        let python = Command::new("python")
            .args(["-c", "import sys; print(sys.executable)"])
            .output()
            .unwrap();
        assert!(python.status.success());
        let interpreter = String::from_utf8(python.stdout).unwrap().trim().to_owned();
        fs::write(&config, serde_json::to_vec(&json!({
            "format":"agentciv-local-runtime/0.1-example", "runtime_id":"runtime:test", "world_id":"civ:test",
            "database_path":directory.path().join("runtime.sqlite"), "administrator_token":ADMIN,
            "coordinator_owner":"operator:test", "python_executable":interpreter, "max_child_seconds":10,
            "scopes":[{"participant":"agent:one","activity":"activity:test","control_token":CONTROL,"child_config_path":child},
                {"participant":"agent:two","activity":"activity:test","control_token":OTHER,"child_config_path":directory.path().join("other.json")}]
        })).unwrap()).unwrap();
        Self {
            directory,
            config,
            child,
        }
    }

    fn command(&self, request: Value) -> (bool, Value) {
        let path = self.directory.path().join("request.json");
        fs::write(&path, serde_json::to_vec(&request).unwrap()).unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_agentciv-runtime"))
            .arg("--config")
            .arg(&self.config)
            .arg("--request")
            .arg(&path)
            .output()
            .unwrap();
        assert!(result.stderr.is_empty());
        let stdout = String::from_utf8(result.stdout).unwrap();
        for secret in [
            ADMIN,
            CONTROL,
            OTHER,
            self.directory.path().to_str().unwrap(),
        ] {
            assert!(
                !stdout.contains(secret),
                "private input escaped sanitized result"
            );
        }
        (
            result.status.success(),
            serde_json::from_str(&stdout).unwrap(),
        )
    }

    fn successful(&self, request: Value) -> Value {
        let (success, result) = self.command(request);
        assert!(success, "{result}");
        assert_eq!(result["outcome"], "ok");
        result
    }

    fn prepare(&self) {
        self.successful(json!({"op":"initialize","token":ADMIN}));
        assert_eq!(
            self.successful(json!({"op":"claim","token":ADMIN}))["generation"],
            1
        );
        let registered = self.successful(json!({"op":"register","token":ADMIN,"participant":"agent:one","activity":"activity:test"}));
        assert_eq!(registered["scope"]["stopped"], true);
        assert_eq!(registered["scope"]["revision"], 0);
    }

    fn resume(&self) {
        self.successful(json!({"op":"resume","token":CONTROL,"participant":"agent:one","activity":"activity:test",
            "command_id":"resume:one","expected_revision":0,"generation":1}));
    }

    fn child_config(&self, origin: &str, runtime_world: &str) {
        fs::write(&self.child, serde_json::to_vec(&json!({
            "mode":"scripted", "principal":"agent:one", "runtime_world":runtime_world,
            "origin":origin,"token":"private-world-capability-0123456789", "record_id":"message:one",
            "recipients":["agent:one"],"condition":"open","round":1,"consider":false,
            "return_invitation":false,"scripted_action":"quiet", "scripted_text":""
        })).unwrap()).unwrap();
    }

    fn dispatch(&self, invitation: &str, history: bool) -> Value {
        self.successful(json!({"op":"dispatch","token":ADMIN,"participant":"agent:one","activity":"activity:test",
            "generation":1,"invitation_id":invitation,"history_available":history}))
    }
}

#[test]
fn private_capabilities_bind_exact_scope_without_administrator_resume_override() {
    let fixture = Fixture::new();
    fixture.prepare();
    for token in [ADMIN, OTHER, "claimed-from-agent-one"] {
        let (success, result) = fixture.command(json!({"op":"resume","token":token,"participant":"agent:one",
            "activity":"activity:test","command_id":"resume:denied","expected_revision":0,"generation":1}));
        assert!(!success);
        assert_eq!(result["code"], "unauthorized");
    }
    let (success, result) = fixture.command(json!({"op":"register","token":CONTROL,"participant":"agent:two","activity":"activity:test"}));
    assert!(!success);
    assert_eq!(result["code"], "unauthorized");
    fixture.resume();
    let (success, result) = fixture.command(
        json!({"op":"stop","token":CONTROL,"participant":"agent:one",
        "activity":"activity:test","command_id":CONTROL,"expected_revision":1,"generation":1}),
    );
    assert!(!success);
    assert_eq!(result["code"], "invalid_request");
    let state = fixture.successful(json!({"op":"inspect","token":CONTROL,"participant":"agent:one","activity":"activity:test"}));
    assert_eq!(state["scope"]["ready"], true);
    let stop = json!({"op":"stop","token":CONTROL,"participant":"agent:one","activity":"activity:test",
        "command_id":"stop:one","expected_revision":1,"generation":1});
    let receipt = fixture.successful(stop.clone());
    assert_eq!(receipt["control"]["policy"], "future_launches_only");
    assert_eq!(fixture.successful(stop), receipt);
    assert_eq!(
        fixture.successful(json!({"op":"claim","token":ADMIN}))["generation"],
        2
    );
    let state = fixture.successful(
        json!({"op":"inspect","token":ADMIN,"participant":"agent:one","activity":"activity:test"}),
    );
    assert_eq!(state["scope"]["stopped"], true);
    assert_eq!(state["scope"]["ready"], false);
    let database = fs::read(fixture.directory.path().join("runtime.sqlite")).unwrap();
    for secret in [ADMIN, CONTROL, OTHER] {
        assert!(
            !database
                .windows(secret.len())
                .any(|bytes| bytes == secret.as_bytes())
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fixed_actual_child_runs_once_and_history_denial_never_spawns() {
    let fixture = Fixture::new();
    let host = start_test_host(HostConfig {
        world_id: "civ:test".to_owned(),
        title: "Test".to_owned(),
        database_path: fixture.directory.path().join("world.sqlite"),
        listen: "127.0.0.1:0".parse().unwrap(),
        visibility: Visibility::Members,
        retention_seconds: 86400,
        max_payload_bytes: 4096,
        credentials: vec![Credential {
            principal: "agent:one".to_owned(),
            token: "private-world-capability-0123456789".to_owned(),
            read: true,
            write: true,
        }],
    })
    .await
    .unwrap();
    let origin = host
        .discovery
        .strip_suffix("/.well-known/agentciv")
        .unwrap();
    fixture.prepare();
    fixture.resume();
    fixture.child_config(origin, "civ:test");
    let denied = fixture.dispatch("invitation:no-history", false);
    assert_eq!(denied["dispatch"]["status"], "not_launched");
    assert!(denied["child"].is_null());
    let launched = fixture.dispatch("invitation:one", true);
    assert_eq!(launched["dispatch"]["status"], "launched");
    assert_eq!(launched["child"]["status"], "completed");
    assert_eq!(launched["child"]["outcome"], "quiet");
    assert_eq!(launched["child"]["action"], "quiet");
    let retry = fixture.dispatch("invitation:one", true);
    assert_eq!(retry["dispatch"], launched["dispatch"]);
    assert!(retry["child"].is_null());
    fixture.successful(
        json!({"op":"stop","token":CONTROL,"participant":"agent:one","activity":"activity:test",
        "command_id":"stop:one","expected_revision":1,"generation":1}),
    );
    let stopped = fixture.dispatch("invitation:stopped", true);
    assert_eq!(stopped["dispatch"]["status"], "not_launched");
    assert!(stopped["child"].is_null());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_discovered_world_fails_child_without_silent_retry() {
    let fixture = Fixture::new();
    let host = start_test_host(HostConfig {
        world_id: "civ:other".to_owned(),
        title: "Other".to_owned(),
        database_path: fixture.directory.path().join("world.sqlite"),
        listen: "127.0.0.1:0".parse().unwrap(),
        visibility: Visibility::Members,
        retention_seconds: 86400,
        max_payload_bytes: 4096,
        credentials: vec![Credential {
            principal: "agent:one".to_owned(),
            token: "private-world-capability-0123456789".to_owned(),
            read: true,
            write: true,
        }],
    })
    .await
    .unwrap();
    fixture.prepare();
    fixture.resume();
    fixture.child_config(
        host.discovery
            .strip_suffix("/.well-known/agentciv")
            .unwrap(),
        "civ:test",
    );
    let launched = fixture.dispatch("invitation:wrong-world", true);
    assert_eq!(launched["dispatch"]["status"], "launched");
    assert_eq!(launched["child"]["outcome"], "failed");
    assert!(fixture.dispatch("invitation:wrong-world", true)["child"].is_null());
}

#[test]
fn malformed_requests_and_private_child_configuration_fail_without_echoing_input() {
    let fixture = Fixture::new();
    fixture.prepare();
    let (success, result) = fixture.command(json!({"op":"inspect","token":ADMIN,"participant":"agent:one","activity":"activity:test","from":"agent:one"}));
    assert!(!success);
    assert_eq!(result["code"], "invalid_request");
    fixture.resume();
    fixture.child_config("http://127.0.0.1:1", "civ:wrong");
    let (success, result) = fixture.command(
        json!({"op":"dispatch","token":ADMIN,"participant":"agent:one","activity":"activity:test",
        "generation":1,"invitation_id":"invitation:wrong","history_available":true}),
    );
    assert!(!success);
    assert_eq!(result["code"], "invalid_configuration");
    fixture.child_config("http://127.0.0.1:1", "civ:test");
    let mut child: Value = serde_json::from_slice(&fs::read(&fixture.child).unwrap()).unwrap();
    child["private_candidates"] = json!(fixture.directory.path().join("trace.json"));
    fs::write(&fixture.child, serde_json::to_vec(&child).unwrap()).unwrap();
    let (success, result) = fixture.command(
        json!({"op":"dispatch","token":ADMIN,"participant":"agent:one","activity":"activity:test",
        "generation":1,"invitation_id":"invitation:trace","history_available":true}),
    );
    assert!(!success);
    assert_eq!(result["code"], "invalid_configuration");
}

#[test]
fn supported_backup_recovery_rotates_all_capabilities_before_reusing_generations() {
    let fixture = Fixture::new();
    fixture.prepare();
    fixture.resume();
    // All CLI processes have exited. This copy is a closed, generation-one
    // database whose active scope predates the later stop.
    let database = fixture.directory.path().join("runtime.sqlite");
    let backup = fixture.directory.path().join("before-stop.sqlite");
    fs::copy(&database, &backup).unwrap();
    assert_eq!(
        fixture.successful(json!({"op":"claim","token":ADMIN}))["generation"],
        2
    );
    let pending_resume = json!({"op":"resume","token":CONTROL,"participant":"agent:one",
        "activity":"activity:test","command_id":"resume:pending-before-restore","expected_revision":1,"generation":2});
    fixture.successful(json!({"op":"stop","token":CONTROL,"participant":"agent:one",
        "activity":"activity:test","command_id":"stop:after-backup","expected_revision":1,"generation":2}));

    // Supported restore requires retiring the prior private configuration.
    // Generation numbers alone repeat after rollback and do not revoke this
    // delayed request. Every adapter must use the newly rotated capabilities.
    fs::copy(&backup, &database).unwrap();
    let new_admin = "recovery-administrator-capability-0123456789";
    let new_control = "recovery-participant-capability-0123456789";
    let new_other = "recovery-other-capability-0123456789";
    let mut config: Value = serde_json::from_slice(&fs::read(&fixture.config).unwrap()).unwrap();
    config["administrator_token"] = json!(new_admin);
    config["scopes"][0]["control_token"] = json!(new_control);
    config["scopes"][1]["control_token"] = json!(new_other);
    fs::write(&fixture.config, serde_json::to_vec(&config).unwrap()).unwrap();
    assert_eq!(
        fixture.successful(json!({"op":"claim","token":new_admin}))["generation"],
        2
    );
    let (success, rejected) = fixture.command(pending_resume);
    assert!(!success);
    assert_eq!(rejected["code"], "unauthorized");
    let (success, rejected) = fixture.command(json!({"op":"claim","token":ADMIN}));
    assert!(!success);
    assert_eq!(rejected["code"], "unauthorized");
    let (success, rejected) = fixture.command(
        json!({"op":"inspect","token":OTHER,"participant":"agent:two","activity":"activity:test"}),
    );
    assert!(!success);
    assert_eq!(rejected["code"], "unauthorized");
    let state = fixture.successful(json!({"op":"inspect","token":new_control,"participant":"agent:one","activity":"activity:test"}));
    assert_eq!(state["scope"]["ready"], false);
    // Reconcile the known current stop before asking for a distinct fresh
    // participant-authorized resume. Backup contents cannot establish it.
    let stopped = fixture.successful(json!({"op":"stop","token":new_control,"participant":"agent:one",
        "activity":"activity:test","command_id":"stop:reconciled-after-restore","expected_revision":1,"generation":2}));
    assert_eq!(stopped["control"]["state"]["stopped"], true);
    let resumed = fixture.successful(json!({"op":"resume","token":new_control,"participant":"agent:one",
        "activity":"activity:test","command_id":"resume:fresh-after-reconciliation","expected_revision":2,"generation":2}));
    assert_eq!(resumed["control"]["state"]["ready"], true);
    assert_eq!(resumed["control"]["state"]["revision"], 3);
}
