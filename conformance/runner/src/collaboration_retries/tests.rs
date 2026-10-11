use std::net::TcpListener;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;

use super::*;

fn context<'a>(client: &'a Client, events: &'a Url, collaborate: Url) -> CollaborationRun<'a> {
    CollaborationRun {
        client,
        events,
        collaborate,
        world_id: "civ:retry-races",
        visibility: "addressed",
        writer: Party {
            principal: "agent:writer",
            token: "writer-secret",
        },
        reader: Party {
            principal: "agent:reader",
            token: "reader-secret",
        },
        peer: None,
        audience: vec!["agent:reader".to_owned()],
    }
}

fn receipt(request: &Value, revision: u64, sequence: usize) -> Value {
    json!({"protocol_version":"0.1-draft", "type":"receipt", "world":request["world"],
        "record_id":request["id"], "status":"recorded", "event_id":format!("event:race-{sequence}"),
        "sequence":sequence, "artifact_id":request["artifact_id"], "revision":revision})
}

fn event(request: &Value, receipt: &Value) -> Value {
    let mut stored = request.clone();
    stored["revision"] = receipt["revision"].clone();
    json!({"protocol_version":"0.1-draft", "type":"event", "world":request["world"],
        "id":receipt["event_id"], "sequence":receipt["sequence"], "timestamp":"2026-10-11T00:00:00Z",
        "actor":request["from"], "kind":"artifact.recorded", "body":{"artifact_revision":stored}})
}

#[test]
fn response_checks_require_exact_identity_conflict_counts_and_original_bytes() {
    let client = Client::new();
    let url = Url::parse("http://127.0.0.1:1/events").unwrap();
    let ctx = context(&client, &url, url.clone());
    let requests = [request(&ctx, Kind::Exact, 0), request(&ctx, Kind::Exact, 1)];
    assert_eq!(requests[0], requests[1]);
    assert!(requests.iter().all(|value| value.to_string().len() <= 1024));
    let make = || {
        [
            Ok(Observed {
                status: StatusCode::OK,
                no_store: true,
                body: receipt(&requests[0], 1, 0),
            }),
            Ok(Observed {
                status: StatusCode::OK,
                no_store: true,
                body: receipt(&requests[1], 1, 0),
            }),
        ]
    };
    assert_eq!(
        verify_responses(Kind::Exact, &requests, &make()).unwrap(),
        0
    );
    for field in [
        "event_id",
        "sequence",
        "revision",
        "artifact_id",
        "world",
        "record_id",
        "type",
    ] {
        let mut responses = make();
        responses[1].as_mut().unwrap().body[field] = json!("wrong");
        assert!(
            verify_responses(Kind::Exact, &requests, &responses).is_err(),
            "{field}"
        );
    }
    let mut responses = make();
    responses[1].as_mut().unwrap().no_store = false;
    assert!(verify_responses(Kind::Exact, &requests, &responses).is_err());
    let conflict = || {
        Ok(Observed {
            status: StatusCode::CONFLICT,
            no_store: true,
            body: json!({"type":"https://agentciv.io/problems/id_conflict","title":"Conflict","status":409,"code":"id_conflict"}),
        })
    };
    for winner in [0, 1] {
        let mut responses = [conflict(), conflict()];
        responses[winner] = Ok(Observed {
            status: StatusCode::OK,
            no_store: true,
            body: receipt(&requests[winner], 1, 0),
        });
        assert_eq!(
            verify_responses(Kind::Conflict, &requests, &responses).unwrap(),
            winner
        );
        assert!(verify_responses(Kind::Exact, &requests, &responses).is_err());
    }
    assert!(verify_responses(Kind::Conflict, &requests, &make()).is_err());
    assert!(verify_responses(Kind::Conflict, &requests, &[conflict(), conflict()]).is_err());
    let mut responses = make();
    responses[0] = Err("bounded transport failure".to_owned());
    assert!(verify_responses(Kind::Exact, &requests, &responses).is_err());
}

#[derive(Clone, Copy, Debug)]
enum Defect {
    None,
    ChangedReceipt,
    BothWinners,
    NextRevision,
    LostHistory,
    CredentialReflection,
    BadConflict,
}

fn exercise_host(defect: Defect) -> Vec<Case> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let events = Url::parse(&format!("http://{address}/events")).unwrap();
    let ctx = context(
        &client,
        &events,
        Url::parse(&format!("http://{address}/collaborate")).unwrap(),
    );
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let worker = thread::spawn(move || {
        let mut history = Vec::new();
        let mut pending = Vec::new();
        let started = Instant::now();
        while !worker_stop.load(Ordering::SeqCst) {
            assert!(started.elapsed() < Duration::from_secs(30));
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
            assert!(headers.contains("Bearer writer-secret"));
            if headers.starts_with("GET ") {
                let visible = if matches!(defect, Defect::LostHistory) {
                    vec![]
                } else {
                    history.clone()
                };
                crate::tests::write_response(
                    &mut stream,
                    "200 OK",
                    "application/json",
                    "Cache-Control: no-store\r\n",
                    &json!({"protocol_version":"0.1-draft","type":"event_page","world":"civ:retry-races",
                        "events":visible,"has_more":false,"next_cursor":"cursor:race"}),
                );
                continue;
            }
            let request: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(bytes, request.to_string().as_bytes());
            if let Some(previous) = history
                .iter()
                .find(|event: &&Value| event["body"]["artifact_revision"]["id"] == request["id"])
            {
                let receipt = receipt(&request, 1, previous["sequence"].as_u64().unwrap() as usize);
                crate::tests::write_response(
                    &mut stream,
                    "200 OK",
                    "application/json",
                    "Cache-Control: no-store\r\n",
                    &receipt,
                );
                continue;
            }
            if request["id"].as_str().unwrap().ends_with("-later") {
                let revision = if matches!(defect, Defect::NextRevision) {
                    3
                } else {
                    2
                };
                let receipt = receipt(&request, revision, history.len());
                history.push(event(&request, &receipt));
                crate::tests::write_response(
                    &mut stream,
                    "200 OK",
                    "application/json",
                    "Cache-Control: no-store\r\n",
                    &receipt,
                );
                continue;
            }
            // Hold the first response until the second request actually arrives.
            pending.push((stream, request));
            if pending.len() < 2 {
                continue;
            }
            let winner = &pending[1].1; // Accept either possible conflicting request as winner.
            let receipt = receipt(winner, 1, history.len());
            history.push(event(winner, &receipt));
            let conflict = winner["id"].as_str().unwrap().ends_with("conflict");
            for (index, (mut stream, _)) in pending.drain(..).enumerate() {
                let rejected = conflict && index == 0 && !matches!(defect, Defect::BothWinners);
                let mut body = if rejected {
                    json!({"type":"https://agentciv.io/problems/id_conflict","title":"Conflict","status":409,
                        "code":if matches!(defect,Defect::BadConflict) {"forbidden"} else {"id_conflict"}})
                } else {
                    receipt.clone()
                };
                if matches!(defect, Defect::ChangedReceipt) && index == 0 {
                    body["event_id"] = json!("event:different");
                }
                if matches!(defect, Defect::CredentialReflection) {
                    body["diagnostic"] = json!("reader-secret");
                }
                crate::tests::write_response(
                    &mut stream,
                    if rejected { "409 Conflict" } else { "200 OK" },
                    if rejected {
                        "application/problem+json"
                    } else {
                        "application/json"
                    },
                    "Cache-Control: no-store\r\n",
                    &body,
                );
            }
        }
    });
    let mut cases = Vec::new();
    run(&ctx, &mut cases);
    stop.store(true, Ordering::SeqCst);
    worker.join().unwrap();
    cases
}

#[test]
fn public_same_id_cases_keep_responses_and_fail_faulty_hosts() {
    for defect in [
        Defect::None,
        Defect::ChangedReceipt,
        Defect::BothWinners,
        Defect::NextRevision,
        Defect::LostHistory,
        Defect::CredentialReflection,
        Defect::BadConflict,
    ] {
        let cases = exercise_host(defect);
        assert_eq!(cases.len(), 3);
        assert!(cases.iter().all(|case| case.required));
        let report = Report {
            cases,
            scope: "credentialed-extended",
        };
        assert_eq!(
            report.passed(),
            matches!(defect, Defect::None),
            "{defect:?}"
        );
        let output = report.to_json().to_string();
        assert!(!output.contains("writer-secret") && !output.contains("reader-secret"));
        if matches!(defect, Defect::None) {
            for case in &report.cases[..2] {
                let evidence = case.evidence.as_ref().unwrap();
                assert_eq!(evidence["responses"].as_array().unwrap().len(), 2);
                assert_eq!(evidence["later_receipt"]["revision"], 2);
            }
        }
    }
}

#[test]
fn history_check_rejects_receipt_event_mismatch_and_losing_content() {
    let client = Client::new();
    let url = Url::parse("http://127.0.0.1:1/events").unwrap();
    let ctx = context(&client, &url, url.clone());
    let request = request(&ctx, Kind::Conflict, 0);
    let receipt = receipt(&request, 1, 0);
    let original = event(&request, &receipt);
    verify_history(&ctx, &request, &receipt, std::slice::from_ref(&original), 1).unwrap();
    for field in ["id", "sequence", "world", "kind", "actor", "body"] {
        let mut changed = original.clone();
        changed[field] = json!("wrong");
        assert!(
            verify_history(&ctx, &request, &receipt, &[changed], 1).is_err(),
            "{field}"
        );
    }
    assert!(verify_history(&ctx, &request, &receipt, &[], 1).is_err());
    assert!(verify_history(&ctx, &request, &receipt, &[original.clone(), original], 1).is_err());
}
