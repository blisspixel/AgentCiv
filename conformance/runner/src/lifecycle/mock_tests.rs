use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::thread::{self, JoinHandle};

use super::*;

#[derive(Clone, Copy, Default)]
enum Defect {
    #[default]
    None,
    LostHistory,
    ChangedRetry,
    CursorReset,
    PolicyLeak,
}

#[derive(Default)]
struct State {
    sender_only: bool,
    defect: Defect,
    events: Vec<Value>,
    receipts: HashMap<(String, String), Value>,
}

struct Mock {
    url: String,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    checkpoint: std::path::PathBuf,
}

impl Drop for Mock {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let joined = self.worker.take().unwrap().join();
        if !std::thread::panicking() {
            assert!(joined.is_ok(), "mock worker panicked");
        }
        if self.checkpoint.exists() {
            std::fs::remove_file(&self.checkpoint).unwrap();
        }
    }
}

fn mock(extended: bool) -> Mock {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let url = format!("http://{address}/.well-known/agentciv");
    let state = Arc::new(Mutex::new(State::default()));
    let stop = Arc::new(AtomicBool::new(false));
    let thread_state = state.clone();
    let thread_stop = stop.clone();
    let worker = thread::spawn(move || {
        while !thread_stop.load(Ordering::SeqCst) {
            let (mut stream, _) = match listener.accept() {
                Ok(connection) => connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(error) => panic!("{error}"),
            };
            stream.set_nonblocking(false).unwrap();
            let (headers, bytes) = crate::tests::read_raw(&mut stream);
            let target = headers
                .lines()
                .next()
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap();
            let mut state = thread_state.lock().unwrap();
            let (status, media, body) = if target.starts_with("/.well-known/") {
                let mut world: Value =
                    serde_json::from_str(include_str!("../../../fixtures/valid/world.json"))
                        .unwrap();
                world["id"] = json!("civ:lifecycle-test");
                world["history"]["visibility"] = json!(if state.sender_only {
                    "sender_only"
                } else {
                    "members"
                });
                world["history"]["retention_seconds"] = json!(3600);
                world["endpoints"] = json!({"submit":format!("http://{address}/submit"),"events":format!("http://{address}/events")});
                if extended {
                    world["capabilities"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!("collaboration.submit"));
                    world["endpoints"]["collaborate"] =
                        json!(format!("http://{address}/collaborate"));
                    world["authentication"]["collaborate"] = json!("bearer");
                }
                ("200 OK", "application/json", world)
            } else {
                let principal = if headers.contains("Bearer writer-secret") {
                    "agent:writer"
                } else if headers.contains("Bearer peer-secret") {
                    "agent:peer"
                } else {
                    "agent:reader"
                };
                if target.starts_with("/events") {
                    if target.contains('?') && state.sender_only {
                        (
                            "410 Gone",
                            "application/problem+json",
                            problem(410, "cursor_expired"),
                        )
                    } else {
                        let mut events = state.events.clone();
                        if matches!(state.defect, Defect::LostHistory) && !events.is_empty() {
                            events.remove(0);
                        }
                        if matches!(state.defect, Defect::CursorReset) && target.contains('?') {
                            events.clear();
                        }
                        if state.sender_only && !matches!(state.defect, Defect::PolicyLeak) {
                            events.retain(|event| event["actor"] == principal);
                        }
                        (
                            "200 OK",
                            "application/json",
                            json!({"protocol_version":"0.1-draft","type":"event_page","world":"civ:lifecycle-test","events":events,"next_cursor":"cursor:start","has_more":false}),
                        )
                    }
                } else {
                    let mut record: Value = serde_json::from_slice(&bytes).unwrap();
                    let key = (
                        principal.to_owned(),
                        record["id"].as_str().unwrap().to_owned(),
                    );
                    if let Some(saved) = state.receipts.get(&key) {
                        let mut receipt = saved.clone();
                        if matches!(state.defect, Defect::ChangedRetry) {
                            receipt["event_id"] = json!("event:changed");
                        }
                        ("200 OK", "application/json", receipt)
                    } else {
                        let kind = record["type"].as_str().unwrap().to_owned();
                        let (sequence, event_id) = if kind == "withdrawal" {
                            let source = state
                                .events
                                .iter_mut()
                                .find(|event| {
                                    event["kind"] == "artifact.recorded"
                                        && event["actor"] == principal
                                })
                                .unwrap();
                            source["kind"] = json!("artifact.withdrawn");
                            source["body"] = json!({});
                            (
                                source["sequence"].as_u64().unwrap(),
                                source["id"].as_str().unwrap().to_owned(),
                            )
                        } else {
                            let sequence = state.events.len() as u64 + 1;
                            let event_id = format!("event:{sequence}");
                            let (field, event_kind) = match kind.as_str() {
                                "message" => ("message", "message.recorded"),
                                "artifact_revision" => {
                                    record["revision"] = json!(1);
                                    ("artifact_revision", "artifact.recorded")
                                }
                                "objection" => ("objection", "objection.recorded"),
                                _ => ("decline", "decline.recorded"),
                            };
                            state.events.push(json!({"protocol_version":"0.1-draft","type":"event","id":event_id,"world":"civ:lifecycle-test","sequence":sequence,"timestamp":"2026-09-29T00:00:00Z","kind":event_kind,"actor":principal,"body":{field:record}}));
                            (sequence, event_id)
                        };
                        let mut receipt = json!({"protocol_version":"0.1-draft","type":"receipt","world":"civ:lifecycle-test","record_id":record["id"],"event_id":event_id,"sequence":sequence,"status":"recorded"});
                        if kind == "artifact_revision" {
                            receipt["artifact_id"] = record["artifact_id"].clone();
                            receipt["revision"] = json!(1);
                        }
                        state.receipts.insert(key, receipt.clone());
                        ("200 OK", "application/json", receipt)
                    }
                }
            };
            crate::tests::write_response(
                &mut stream,
                status,
                media,
                "Cache-Control: no-store\r\n",
                &body,
            );
        }
    });
    Mock {
        url,
        state,
        stop,
        worker: Some(worker),
        checkpoint: std::env::temp_dir().join(format!(
            "agentciv-lifecycle-test-{}-{}.json",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        )),
    }
}

fn problem(status: u16, code: &str) -> Value {
    json!({"type":format!("https://agentciv.io/problems/{code}"),"title":"Mock rejection","status":status,"code":code})
}

fn run(mock: &Mock, extended: bool, phase: &str) -> Report {
    run_lifecycle(
        &mock.url,
        ("agent:writer", "writer-secret"),
        ("agent:reader", "reader-secret"),
        extended.then_some(("agent:peer", "peer-secret")),
        phase,
        &mock.checkpoint,
    )
}

#[test]
fn lifecycle_tests_core_and_extension_phases_through_public_http() {
    for extended in [false, true] {
        let host = mock(extended);
        let prepare = run(&host, extended, "prepare");
        assert!(prepare.passed(), "{prepare:?}");
        let saved = std::fs::read_to_string(&host.checkpoint).unwrap();
        for token in ["writer-secret", "reader-secret", "peer-secret"] {
            assert!(!saved.contains(token));
        }
        let verify = run(&host, extended, "verify");
        assert!(verify.passed(), "{verify:?}");
        host.state.lock().unwrap().sender_only = true;
        let policy = run(&host, extended, "policy");
        assert!(policy.passed(), "{policy:?}");
        for report in [prepare, verify, policy] {
            let extension = report
                .cases
                .iter()
                .find(|case| case.id == "lifecycle.collaboration")
                .unwrap();
            assert_eq!(extension.required, extended);
            assert_eq!(
                extension.status,
                if extended {
                    CaseStatus::Passed
                } else {
                    CaseStatus::Skipped
                }
            );
        }
    }
}

#[test]
fn lifecycle_reports_lost_history_cursor_reset_changed_retry_and_policy_leak() {
    let host = mock(true);
    assert!(run(&host, true, "prepare").passed());
    for (defect, id) in [
        (Defect::LostHistory, "restart.history"),
        (Defect::CursorReset, "restart.cursor"),
        (Defect::ChangedRetry, "restart.retry"),
    ] {
        host.state.lock().unwrap().defect = defect;
        let report = run(&host, true, "verify");
        assert!(!report.passed());
        assert!(
            report
                .cases
                .iter()
                .any(|case| case.id == id && case.status == CaseStatus::Failed),
            "{report:?}"
        );
    }
    {
        let mut state = host.state.lock().unwrap();
        state.sender_only = true;
        state.defect = Defect::PolicyLeak;
    }
    let report = run(&host, true, "policy");
    assert!(!report.passed());
    assert!(
        report
            .cases
            .iter()
            .any(|case| case.id == "policy.visibility" && case.status == CaseStatus::Failed)
    );
}

#[test]
fn advertised_lifecycle_extension_requires_a_peer_before_writing() {
    let host = mock(true);
    let report = run(&host, false, "prepare");
    assert!(!report.passed());
    assert!(host.state.lock().unwrap().events.is_empty());
    assert!(!host.checkpoint.exists());
}
