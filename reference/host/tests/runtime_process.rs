//! Independent OS processes exercise the shared admission lock and crash recovery.
//! The deterministic callback records a marker, not a model-behavior observation.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use agentciv_host::runtime::{Decision, DispatchRequest, Gate, LaunchStatus, Scope};

const WAIT: Duration = Duration::from_secs(10);

fn scope() -> Scope {
    Scope {
        participant: "participant-A".to_owned(),
        activity: "gathering".to_owned(),
    }
}

fn request(id: &str, generation: u64) -> DispatchRequest {
    DispatchRequest {
        generation,
        invitation_id: id.to_owned(),
        payload_identity: "prepared-fixture-input".to_owned(),
        history_available: true,
        launch_budget: 1,
    }
}

fn wait_file(path: &Path) {
    let start = Instant::now();
    while !path.exists() {
        assert!(
            start.elapsed() < WAIT,
            "process marker did not arrive: {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(5));
    }
}

fn wait_child(child: &mut Child) {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "helper process failed: {status}");
            return;
        }
        if start.elapsed() >= WAIT {
            let _ = child.kill();
            let _ = child.wait();
            panic!("helper process timed out");
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn child(directory: &Path, mode: &str, id: &str, generation: u64) -> Child {
    Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "runtime_process_helper", "--nocapture"])
        .env("AGENTCIV_RUNTIME_PROCESS_TEST", mode)
        .env("AGENTCIV_RUNTIME_PROCESS_DIR", directory)
        .env("AGENTCIV_RUNTIME_PROCESS_ID", id)
        .env(
            "AGENTCIV_RUNTIME_PROCESS_GENERATION",
            generation.to_string(),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap()
}

fn fixture() -> (tempfile::TempDir, Gate, u64) {
    let directory = tempfile::tempdir().unwrap();
    let gate =
        Gate::initialize(&directory.path().join("runtime.sqlite"), "runtime", "world").unwrap();
    gate.register_scope(&scope()).unwrap();
    let generation = gate.claim_coordinator("initial").unwrap();
    gate.control(&scope(), "ready", 0, Decision::Resume, generation)
        .unwrap();
    (directory, gate, generation)
}

#[test]
fn runtime_process_helper() {
    let Ok(mode) = std::env::var("AGENTCIV_RUNTIME_PROCESS_TEST") else {
        return;
    };
    let directory = PathBuf::from(std::env::var_os("AGENTCIV_RUNTIME_PROCESS_DIR").unwrap());
    let id = std::env::var("AGENTCIV_RUNTIME_PROCESS_ID").unwrap();
    let generation = std::env::var("AGENTCIV_RUNTIME_PROCESS_GENERATION")
        .unwrap()
        .parse()
        .unwrap();
    let mark =
        |name: &str| std::fs::write(directory.join(format!("{id}-{name}")), b"fixture").unwrap();
    if mode == "before_reservation" {
        mark("entered");
        wait_file(&directory.join(format!("{id}-release")));
    }
    if mode == "stop" {
        mark("attempt");
    }
    let gate = Gate::open(&directory.join("runtime.sqlite"), "runtime", "world").unwrap();
    match mode.as_str() {
        "stop" | "stop_after_commit" | "stop_before_commit" => {
            if mode == "stop_before_commit" {
                mark("attempt");
            }
            let receipt = gate.control(&scope(), &id, 1, Decision::Stop, 0).unwrap();
            assert!(receipt.state.stopped);
            mark("committed");
            if mode == "stop_after_commit" {
                wait_file(&directory.join(format!("{id}-release")));
            }
            mark("ack");
        }
        "before_reservation" | "before_launch" | "after_launch" | "race" => {
            gate.dispatch(&scope(), &request(&id, generation), || {
                if mode == "before_launch" {
                    mark("entered");
                    wait_file(&directory.join(format!("{id}-release")));
                }
                mark("launched");
                if mode == "after_launch" || mode == "race" {
                    mark("entered");
                    wait_file(&directory.join(format!("{id}-release")));
                }
                Ok(())
            })
            .unwrap();
        }
        _ => panic!("unknown helper mode"),
    }
}

#[test]
fn stop_and_dispatch_serialize_across_processes_and_stop_first_blocks() {
    let (directory, gate, generation) = fixture();
    let mut dispatcher = child(directory.path(), "race", "race", generation);
    wait_file(&directory.path().join("race-entered"));
    let mut stopper = child(directory.path(), "stop", "race-stop", generation);
    wait_file(&directory.path().join("race-stop-attempt"));
    thread::sleep(Duration::from_millis(100));
    assert!(!directory.path().join("race-stop-ack").exists());
    std::fs::write(directory.path().join("race-release"), b"release").unwrap();
    wait_child(&mut dispatcher);
    wait_child(&mut stopper);
    assert!(directory.path().join("race-launched").exists());
    assert!(directory.path().join("race-stop-ack").exists());
    assert!(gate.inspect(&scope()).unwrap().stopped);
    let mut blocked = child(directory.path(), "after_launch", "after-ack", generation);
    wait_child(&mut blocked);
    assert!(!directory.path().join("after-ack-launched").exists());
    let replacement = gate.claim_coordinator("replacement").unwrap();
    let outcome = gate
        .dispatch(
            &scope(),
            &request("replacement", replacement),
            || -> Result<(), _> { panic!("stop was bypassed") },
        )
        .unwrap();
    assert_eq!(outcome.receipt.reason, "stopped");
}

#[test]
fn killed_reservations_remain_uncertain_and_cannot_replay() {
    for mode in ["before_launch", "after_launch"] {
        let (directory, gate, generation) = fixture();
        let mut process = child(directory.path(), mode, mode, generation);
        wait_file(&directory.path().join(format!("{mode}-entered")));
        process.kill().unwrap();
        process.wait().unwrap();
        let reopened =
            Gate::open(&directory.path().join("runtime.sqlite"), "runtime", "world").unwrap();
        let outcome = reopened
            .dispatch(&scope(), &request(mode, generation), || -> Result<(), _> {
                panic!("crashed reservation replayed")
            })
            .unwrap();
        assert_eq!(outcome.receipt.status, LaunchStatus::Uncertain);
        assert_eq!(
            directory.path().join(format!("{mode}-launched")).exists(),
            mode == "after_launch"
        );
        gate.claim_coordinator("recovery").unwrap();
        assert!(!gate.inspect(&scope()).unwrap().ready);
        assert_eq!(
            gate.dispatch(&scope(), &request(mode, generation), || -> Result<(), _> {
                panic!("replacement replayed")
            })
            .unwrap()
            .receipt
            .status,
            LaunchStatus::Uncertain
        );
    }
}

#[test]
fn kill_before_reservation_does_not_invent_an_invitation_or_launch() {
    let (directory, gate, generation) = fixture();
    let mut process = child(directory.path(), "before_reservation", "before", generation);
    wait_file(&directory.path().join("before-entered"));
    process.kill().unwrap();
    process.wait().unwrap();
    assert!(!directory.path().join("before-launched").exists());
    let recovered = gate.claim_coordinator("recovery").unwrap();
    let outcome = gate
        .dispatch(
            &scope(),
            &request("before", recovered),
            || -> Result<(), _> { panic!("missing recovery readiness bypassed") },
        )
        .unwrap();
    assert_eq!(outcome.receipt.status, LaunchStatus::NotLaunched);
    assert_eq!(outcome.receipt.reason, "not_ready");
}

#[test]
fn committed_stop_with_lost_acknowledgement_retries_exactly_after_process_death() {
    let (directory, gate, generation) = fixture();
    let mut process = child(
        directory.path(),
        "stop_after_commit",
        "lost-ack",
        generation,
    );
    wait_file(&directory.path().join("lost-ack-committed"));
    assert!(!directory.path().join("lost-ack-ack").exists());
    process.kill().unwrap();
    process.wait().unwrap();
    let receipt = gate
        .control(&scope(), "lost-ack", 1, Decision::Stop, 0)
        .unwrap();
    assert!(receipt.state.stopped);
    assert_eq!(receipt.state.revision, 2);
    assert_eq!(
        gate.dispatch(
            &scope(),
            &request("after-crash", generation),
            || -> Result<(), _> { panic!("lost ack reversed durable stop") }
        )
        .unwrap()
        .receipt
        .reason,
        "stopped"
    );
}

#[test]
fn killed_stop_before_commit_has_no_ack_and_recovery_requires_readiness() {
    let (directory, gate, generation) = fixture();
    let mut connection =
        rusqlite::Connection::open(directory.path().join("runtime.sqlite")).unwrap();
    // An independent SQLite writer prevents the child's immediate transaction
    // from committing. Stop is never acknowledged while that write is blocked.
    let writer = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let mut process = child(
        directory.path(),
        "stop_before_commit",
        "uncommitted",
        generation,
    );
    wait_file(&directory.path().join("uncommitted-attempt"));
    process.kill().unwrap();
    process.wait().unwrap();
    writer.rollback().unwrap();
    assert!(!directory.path().join("uncommitted-ack").exists());
    assert_eq!(gate.inspect(&scope()).unwrap().revision, 1);
    assert!(!gate.inspect(&scope()).unwrap().stopped);
    let replacement = gate
        .claim_coordinator("recovery-after-unconfirmed-stop")
        .unwrap();
    assert_eq!(
        gate.dispatch(
            &scope(),
            &request("unconfirmed-recovery", replacement),
            || -> Result<(), _> { panic!("recovery readiness bypassed") }
        )
        .unwrap()
        .receipt
        .reason,
        "not_ready"
    );
}
