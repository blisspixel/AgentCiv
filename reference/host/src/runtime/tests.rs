use std::sync::{Arc, Barrier};

use super::*;

fn scope() -> Scope {
    Scope {
        participant: "participant-A".to_owned(),
        activity: "open-gathering".to_owned(),
    }
}

fn fixture() -> (tempfile::TempDir, Gate, Scope, u64) {
    let directory = tempfile::tempdir().unwrap();
    let gate = Gate::initialize(
        &directory.path().join("runtime.sqlite"),
        "runtime-A",
        "world-A",
    )
    .unwrap();
    let scope = scope();
    let generation = gate.claim_coordinator("coordinator-A").unwrap();
    assert!(gate.register_scope(&scope).unwrap().stopped);
    (directory, gate, scope, generation)
}

fn request(id: &str, generation: u64) -> DispatchRequest {
    DispatchRequest {
        generation,
        invitation_id: id.to_owned(),
        payload_identity: "fixture-input-sha256".to_owned(),
        history_available: true,
        launch_budget: 1,
    }
}

fn forbidden() -> Result<(), ExecutorFailure> {
    panic!("blocked dispatch must not invoke executor")
}

#[test]
fn stop_survives_replacement_and_resume_is_cas_and_scope_specific() {
    let (_directory, gate, scope, generation) = fixture();
    let registered = gate.register_scope(&scope).unwrap();
    assert_eq!(registered.revision, 0);
    assert!(!registered.ready);
    let mut first = request("initial-stopped", generation);
    assert_eq!(
        gate.dispatch(&scope, &first, forbidden)
            .unwrap()
            .receipt
            .reason,
        "stopped"
    );
    let resume = gate
        .control(&scope, "resume-1", 0, Decision::Resume, generation)
        .unwrap();
    assert!(resume.state.ready);
    assert!(!resume.state.stopped);
    assert_eq!(resume.policy, "future_launches_only");
    first.invitation_id = "first-launch".to_owned();
    assert_eq!(
        gate.dispatch(&scope, &first, || Ok(42))
            .unwrap()
            .executor_value,
        Some(42)
    );
    assert!(
        gate.dispatch(&scope, &first, forbidden)
            .unwrap()
            .executor_value
            .is_none()
    );
    let stop = gate
        .control(&scope, "stop-1", 1, Decision::Stop, 0)
        .unwrap();
    assert_eq!(stop.state.revision, 2);
    assert!(stop.state.stopped);
    assert!(!stop.state.ready);
    let next = gate.claim_coordinator("replacement-B").unwrap();
    assert_eq!(next, generation + 1);
    assert!(gate.inspect(&scope).unwrap().stopped);
    assert_eq!(
        gate.control(&scope, "resume-delayed", 1, Decision::Resume, next),
        Err(Error::StaleRevision)
    );
    assert_eq!(
        gate.control(
            &scope,
            "resume-old-coordinator",
            2,
            Decision::Resume,
            generation
        ),
        Err(Error::StaleGeneration)
    );
    assert_eq!(
        gate.control(&scope, "resume-1", 0, Decision::Resume, generation)
            .unwrap(),
        resume
    );
    assert!(!gate.inspect(&scope).unwrap().ready);
    assert_eq!(
        gate.dispatch(&scope, &request("stopped-replacement", next), forbidden)
            .unwrap()
            .receipt
            .reason,
        "stopped"
    );
    let volunteer = Scope {
        participant: "participant-B".to_owned(),
        activity: scope.activity.clone(),
    };
    gate.register_scope(&volunteer).unwrap();
    gate.control(&volunteer, "volunteer-return", 0, Decision::Resume, next)
        .unwrap();
    assert_eq!(
        gate.dispatch(&volunteer, &request("independent", next), || Ok(()))
            .unwrap()
            .receipt
            .status,
        LaunchStatus::Launched
    );
    assert!(gate.inspect(&scope).unwrap().stopped);
    gate.control(&scope, "resume-2", 2, Decision::Resume, next)
        .unwrap();
    assert_eq!(
        gate.dispatch(&scope, &request("explicit-return", next), || Ok(()))
            .unwrap()
            .receipt
            .status,
        LaunchStatus::Launched
    );
}

#[test]
fn exact_commands_and_invitations_cannot_be_repurposed() {
    let (_directory, gate, scope, generation) = fixture();
    let stop = gate
        .control(&scope, "stop", 0, Decision::Stop, generation)
        .unwrap();
    assert_eq!(
        gate.control(&scope, "stop", 0, Decision::Stop, generation)
            .unwrap(),
        stop
    );
    assert_eq!(
        gate.control(&scope, "stop", 0, Decision::Resume, generation),
        Err(Error::Conflict)
    );
    let invitation = request("blocked-before-resume", generation);
    let blocked = gate
        .dispatch(&scope, &invitation, forbidden)
        .unwrap()
        .receipt;
    gate.control(&scope, "resume", 1, Decision::Resume, generation)
        .unwrap();
    assert_eq!(
        gate.dispatch(&scope, &invitation, forbidden)
            .unwrap()
            .receipt,
        blocked
    );
    let mut changed = invitation.clone();
    changed.payload_identity = "new bytes".to_owned();
    assert!(matches!(
        gate.dispatch(&scope, &changed, forbidden),
        Err(Error::Conflict)
    ));
    let launched = gate
        .dispatch(&scope, &request("launch", generation), || Ok(()))
        .unwrap();
    assert_eq!(launched.receipt.status, LaunchStatus::Launched);
    assert_eq!(
        gate.dispatch(&scope, &request("launch", generation), forbidden)
            .unwrap()
            .receipt,
        launched.receipt
    );
}

#[test]
fn coordinator_claim_clears_restored_readiness_without_rollback_replay_protection() {
    let (directory, gate, scope, generation) = fixture();
    gate.control(&scope, "initial-resume", 0, Decision::Resume, generation)
        .unwrap();
    let backup = directory.path().join("backup.sqlite");
    std::fs::copy(&gate.path, &backup).unwrap();
    gate.control(&scope, "later-stop", 1, Decision::Stop, generation)
        .unwrap();
    let replacement = gate.claim_coordinator("replacement").unwrap();
    assert_eq!(
        gate.dispatch(&scope, &request("stale-generation", generation), forbidden)
            .unwrap()
            .receipt
            .reason,
        "stale_generation"
    );
    assert_eq!(
        gate.dispatch(&scope, &request("stopped", replacement), forbidden)
            .unwrap()
            .receipt
            .reason,
        "stopped"
    );
    // The claim clears readiness. Supported adapter recovery additionally retires
    // old capabilities. Database generations alone are not rollback detection.
    let restored = Gate::open(&backup, "runtime-A", "world-A").unwrap();
    let recovery = restored.claim_coordinator("recovery").unwrap();
    assert!(!restored.inspect(&scope).unwrap().stopped);
    assert!(!restored.inspect(&scope).unwrap().ready);
    assert_eq!(
        restored
            .dispatch(&scope, &request("restored", recovery), forbidden)
            .unwrap()
            .receipt
            .reason,
        "not_ready"
    );
    restored
        .control(
            &scope,
            "fresh-authorized-reconciliation",
            1,
            Decision::Resume,
            recovery,
        )
        .unwrap();
    assert!(restored.inspect(&scope).unwrap().ready);
}

#[test]
fn structurally_valid_but_mismatched_command_receipts_fail_closed() {
    let (_directory, gate, scope, generation) = fixture();
    let original = gate
        .control(&scope, "stop", 0, Decision::Stop, generation)
        .unwrap();
    let connection = Connection::open(&gate.path).unwrap();
    for (path, value) in [
        ("/state/runtime_id", serde_json::json!("different-runtime")),
        ("/state/world_id", serde_json::json!("different-world")),
        (
            "/state/scope/participant",
            serde_json::json!("different-principal"),
        ),
        (
            "/state/scope/activity",
            serde_json::json!("different-activity"),
        ),
        ("/state/revision", serde_json::json!(2)),
        ("/state/generation", serde_json::json!(2)),
        ("/state/command_id", serde_json::json!("different-command")),
        ("/state/stopped", serde_json::json!(false)),
        ("/state/ready", serde_json::json!(true)),
        ("/decision", serde_json::json!("resume")),
        ("/command_id", serde_json::json!("different-command")),
        ("/policy", serde_json::json!("unbounded")),
    ] {
        let mut altered = serde_json::to_value(&original).unwrap();
        *altered.pointer_mut(path).unwrap() = value;
        connection
            .execute("UPDATE commands SET receipt=?1", [altered.to_string()])
            .unwrap();
        assert_eq!(
            gate.control(&scope, "stop", 0, Decision::Stop, generation),
            Err(Error::StateUnavailable),
            "accepted mismatched receipt field {path}"
        );
    }
    connection
        .execute(
            "UPDATE commands SET receipt=?1",
            [serde_json::to_string(&original).unwrap()],
        )
        .unwrap();
    assert_eq!(
        gate.control(&scope, "stop", 0, Decision::Stop, generation)
            .unwrap(),
        original
    );
}

#[test]
fn malformed_invitation_status_reason_pairs_fail_closed() {
    let (_directory, gate, scope, generation) = fixture();
    gate.control(&scope, "ready", 0, Decision::Resume, generation)
        .unwrap();
    let invitation = request("launch", generation);
    gate.dispatch(&scope, &invitation, || Ok(())).unwrap();
    let connection = Connection::open(&gate.path).unwrap();
    for reason in ["stopped", "untrusted private database text"] {
        connection
            .execute("UPDATE invitations SET reason=?1", [reason])
            .unwrap();
        assert!(matches!(
            gate.dispatch(&scope, &invitation, forbidden),
            Err(Error::StateUnavailable)
        ));
    }
    connection
        .execute("UPDATE invitations SET reason='executor_started'", [])
        .unwrap();
    assert_eq!(
        gate.dispatch(&scope, &invitation, forbidden)
            .unwrap()
            .receipt
            .status,
        LaunchStatus::Launched
    );
}

#[test]
fn unavailable_history_and_budget_are_distinct_and_stop_still_commits() {
    let (_directory, gate, scope, generation) = fixture();
    gate.control(&scope, "ready", 0, Decision::Resume, generation)
        .unwrap();
    let mut history = request("history", generation);
    history.history_available = false;
    assert_eq!(
        gate.dispatch(&scope, &history, forbidden)
            .unwrap()
            .receipt
            .reason,
        "history_unavailable"
    );
    let mut budget = request("budget", generation);
    budget.launch_budget = 0;
    assert_eq!(
        gate.dispatch(&scope, &budget, forbidden)
            .unwrap()
            .receipt
            .reason,
        "budget_unavailable"
    );
    gate.control(&scope, "stop-without-history", 1, Decision::Stop, 0)
        .unwrap();
    assert!(gate.inspect(&scope).unwrap().stopped);
}

#[test]
fn executor_failure_and_failed_outcome_commit_never_replay() {
    let (_directory, gate, scope, generation) = fixture();
    gate.control(&scope, "ready", 0, Decision::Resume, generation)
        .unwrap();
    let not_started = request("not-started", generation);
    assert_eq!(
        gate.dispatch::<()>(&scope, &not_started, || Err(ExecutorFailure::NotStarted))
            .unwrap()
            .receipt
            .status,
        LaunchStatus::NotLaunched
    );
    assert_eq!(
        gate.dispatch(&scope, &not_started, forbidden)
            .unwrap()
            .receipt
            .reason,
        "executor_not_started"
    );
    let uncertain = request("uncertain", generation);
    assert_eq!(
        gate.dispatch::<()>(&scope, &uncertain, || Err(ExecutorFailure::Uncertain))
            .unwrap()
            .receipt
            .status,
        LaunchStatus::Uncertain
    );
    assert_eq!(
        gate.dispatch(&scope, &uncertain, forbidden)
            .unwrap()
            .receipt
            .reason,
        "executor_uncertain"
    );
    let lost_outcome = request("lost-outcome", generation);
    let outcome = gate.dispatch(&scope,&lost_outcome,|| {
        Connection::open(&gate.path).unwrap().execute_batch("CREATE TRIGGER fail_outcome BEFORE UPDATE ON invitations BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
        Ok("outside side effect happened")
    }).unwrap();
    assert_eq!(outcome.executor_value, Some("outside side effect happened"));
    assert_eq!(outcome.receipt.status, LaunchStatus::Uncertain);
    assert_eq!(outcome.receipt.reason, "outcome_unconfirmed");
    assert_eq!(
        gate.dispatch(&scope, &lost_outcome, forbidden)
            .unwrap()
            .receipt
            .reason,
        "reservation_unconfirmed"
    );
    Connection::open(&gate.path)
        .unwrap()
        .execute_batch("DROP TRIGGER fail_outcome")
        .unwrap();
    gate.claim_coordinator("replacement").unwrap();
    assert_eq!(
        gate.dispatch(&scope, &lost_outcome, forbidden)
            .unwrap()
            .receipt
            .reason,
        "coordinator_replaced"
    );
}

#[test]
fn failed_stop_transaction_has_no_enforced_acknowledgement() {
    let (_directory, gate, scope, generation) = fixture();
    gate.control(&scope, "ready", 0, Decision::Resume, generation)
        .unwrap();
    Connection::open(&gate.path).unwrap().execute_batch("CREATE TRIGGER fail_command BEFORE INSERT ON commands BEGIN SELECT RAISE(ABORT,'injected'); END;").unwrap();
    assert_eq!(
        gate.control(&scope, "stop", 1, Decision::Stop, 0),
        Err(Error::StateUnavailable)
    );
    let state = gate.inspect(&scope).unwrap();
    assert!(!state.stopped);
    assert_eq!(state.revision, 1);
    Connection::open(&gate.path)
        .unwrap()
        .execute_batch("DROP TRIGGER fail_command")
        .unwrap();
    assert!(
        gate.control(&scope, "stop", 1, Decision::Stop, 0)
            .unwrap()
            .state
            .stopped
    );
}

#[test]
fn missing_corrupt_wrong_identity_and_invalid_input_fail_closed() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("runtime.sqlite");
    assert!(matches!(
        Gate::open(&path, "r", "w"),
        Err(Error::StateUnavailable)
    ));
    assert!(!path.exists());
    assert!(matches!(
        Gate::initialize(&path, "", "w"),
        Err(Error::InvalidInput)
    ));
    let gate = Gate::initialize(&path, "r", "w").unwrap();
    assert!(matches!(
        Gate::initialize(&path, "r", "w"),
        Err(Error::StateUnavailable)
    ));
    assert!(matches!(
        Gate::open(&path, "r", "another-world"),
        Err(Error::IdentityMismatch)
    ));
    assert!(matches!(
        Gate::open(&path, "another-runtime", "w"),
        Err(Error::IdentityMismatch)
    ));
    assert_eq!(gate.inspect(&scope()), Err(Error::UnknownScope));
    assert_eq!(
        gate.claim_coordinator("invalid\nowner"),
        Err(Error::InvalidInput)
    );
    assert_eq!(
        gate.register_scope(&Scope {
            participant: "p".repeat(1025),
            activity: "a".to_owned()
        }),
        Err(Error::InvalidInput)
    );
    gate.register_scope(&scope()).unwrap();
    assert_eq!(
        gate.control(&scope(), "x", u64::MAX, Decision::Stop, 0),
        Err(Error::InvalidInput)
    );
    let mut invalid = request("x", 0);
    invalid.payload_identity.clear();
    assert!(matches!(
        gate.dispatch(&scope(), &invalid, forbidden),
        Err(Error::InvalidInput)
    ));
    std::fs::write(&path, "not a database").unwrap();
    assert!(matches!(
        Gate::open(&path, "r", "w"),
        Err(Error::StateUnavailable)
    ));
    assert_eq!(gate.inspect(&scope()), Err(Error::StateUnavailable));
}

#[test]
fn durable_stop_waits_for_actual_executor_under_thread_race() {
    let (_directory, gate, scope, generation) = fixture();
    gate.control(&scope, "ready", 0, Decision::Resume, generation)
        .unwrap();
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let dispatch_gate = Gate::open(&gate.path, "runtime-A", "world-A").unwrap();
    let dispatch_scope = scope.clone();
    let dispatch_entered = entered.clone();
    let dispatch_release = release.clone();
    let dispatch = std::thread::spawn(move || {
        dispatch_gate
            .dispatch(&dispatch_scope, &request("race", generation), || {
                dispatch_entered.wait();
                dispatch_release.wait();
                Ok(())
            })
            .unwrap()
    });
    entered.wait();
    let (send, receive) = std::sync::mpsc::channel();
    let stop_gate = Gate::open(&gate.path, "runtime-A", "world-A");
    // open itself takes the common lock, so avoid blocking this test's releaser.
    assert!(matches!(stop_gate, Err(Error::LockUnavailable)));
    let stopper = gate.clone();
    let stop_scope = scope.clone();
    let stop = std::thread::spawn(move || {
        send.send(stopper.control(&stop_scope, "race-stop", 1, Decision::Stop, 0))
            .unwrap();
    });
    assert!(receive.recv_timeout(Duration::from_millis(50)).is_err());
    release.wait();
    assert_eq!(
        dispatch.join().unwrap().receipt.status,
        LaunchStatus::Launched
    );
    assert!(
        receive
            .recv_timeout(Duration::from_secs(4))
            .unwrap()
            .unwrap()
            .state
            .stopped
    );
    stop.join().unwrap();
    assert_eq!(
        gate.dispatch(&scope, &request("after-ack", generation), forbidden)
            .unwrap()
            .receipt
            .reason,
        "stopped"
    );
}

#[test]
fn lock_contention_is_bounded_and_never_invokes_executor() {
    let (_directory, gate, scope, generation) = fixture();
    let mut lock_name = gate.path.as_os_str().to_os_string();
    lock_name.push(".dispatch-lock");
    let file = File::options()
        .read(true)
        .write(true)
        .open(PathBuf::from(lock_name))
        .unwrap();
    file.try_lock().unwrap();
    assert_eq!(
        gate.control(&scope, "stop", 0, Decision::Stop, generation),
        Err(Error::LockUnavailable)
    );
    assert!(matches!(
        gate.dispatch(&scope, &request("locked", generation), forbidden),
        Err(Error::LockUnavailable)
    ));
    drop(file);
    assert!(
        gate.control(&scope, "stop", 0, Decision::Stop, generation)
            .unwrap()
            .state
            .stopped
    );
}

#[test]
fn public_errors_and_status_are_fixed_data_only_values() {
    for error in [
        Error::InvalidInput,
        Error::StateUnavailable,
        Error::IdentityMismatch,
        Error::LockUnavailable,
        Error::UnknownScope,
        Error::Conflict,
        Error::StaleGeneration,
        Error::StaleRevision,
        Error::Capacity,
    ] {
        assert_eq!(error.to_string(), error.code());
        assert_eq!(
            serde_json::from_str::<Error>(&serde_json::to_string(&error).unwrap()).unwrap(),
            error
        );
    }
}

#[test]
fn bounded_storage_capacity_fails_without_new_acknowledgement_or_launch() {
    let (_directory, gate, scope, generation) = fixture();
    gate.control(&scope, "ready", 0, Decision::Resume, generation)
        .unwrap();
    let connection = Connection::open(&gate.path).unwrap();
    connection.execute_batch("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<4095) INSERT INTO commands(participant,activity,command_id,request,receipt) SELECT 'participant-A','open-gathering','filler-'||x,'[]','{}' FROM n;").unwrap();
    assert_eq!(
        gate.control(&scope, "capacity-stop", 1, Decision::Stop, 0),
        Err(Error::Capacity)
    );
    assert_eq!(gate.inspect(&scope).unwrap().revision, 1);
    connection.execute_batch("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<4096) INSERT INTO invitations(participant,activity,invitation_id,request,generation,revision,status,reason) SELECT 'participant-A','open-gathering','filler-'||x,'{}',1,1,'not_launched','fixture' FROM n;").unwrap();
    assert!(matches!(
        gate.dispatch(&scope, &request("capacity", generation), forbidden),
        Err(Error::Capacity)
    ));
    connection.execute_batch("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<255) INSERT INTO scopes(participant,activity,revision,stopped) SELECT 'filler-'||x,'activity',0,1 FROM n;").unwrap();
    assert_eq!(
        gate.register_scope(&Scope {
            participant: "another".to_owned(),
            activity: "activity".to_owned()
        }),
        Err(Error::Capacity)
    );
    // Existing stop receipts remain inspectable and exactly retryable at capacity.
    assert!(
        gate.control(&scope, "ready", 0, Decision::Resume, generation)
            .unwrap()
            .state
            .ready
    );
}

#[test]
fn incomplete_schema_and_malformed_receipts_are_unavailable() {
    let (_directory, gate, scope, generation) = fixture();
    gate.control(&scope, "stop", 0, Decision::Stop, generation)
        .unwrap();
    let connection = Connection::open(&gate.path).unwrap();
    connection
        .execute("UPDATE commands SET receipt='broken'", [])
        .unwrap();
    assert_eq!(
        gate.control(&scope, "stop", 0, Decision::Stop, generation),
        Err(Error::StateUnavailable)
    );
    connection.execute_batch("DROP TABLE invitations").unwrap();
    assert!(matches!(
        Gate::open(&gate.path, "runtime-A", "world-A"),
        Err(Error::StateUnavailable)
    ));
    assert_eq!(
        gate.control(&scope, "other-stop", 1, Decision::Stop, 0),
        Err(Error::StateUnavailable)
    );
}
