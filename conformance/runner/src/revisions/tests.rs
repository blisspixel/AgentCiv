use std::net::TcpListener;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;

use super::*;

#[derive(Clone, Copy, Debug)]
enum Defect {
    None,
    ResetRevision,
    DuplicateId,
    DuplicateSequence,
    WrongActor,
    WrongArtifact,
    ChangedBytes,
    OmittedMiddle,
    ChangedRetry,
    AppendedRetry,
    RejectionAllocates,
    AcceptedClientRevision,
    RejectionWrites,
}

fn exercise(defect: Defect) -> Vec<Case> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let worker = thread::spawn(move || {
        let mut events: Vec<Value> = Vec::new();
        let mut receipts = std::collections::HashMap::<String, Value>::new();
        let mut allocated = 0;
        while !worker_stop.load(Ordering::SeqCst) {
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
            let (status, media, response) = if headers.starts_with("GET ") {
                let mut visible = events.clone();
                if matches!(defect, Defect::OmittedMiddle) && allocated >= 3 {
                    visible.retain(|event| event["body"]["artifact_revision"]["revision"] != 2);
                }
                (
                    "200 OK",
                    "application/json",
                    json!({"protocol_version":"0.1-draft","type":"event_page","world":"civ:sequence","events":visible,"next_cursor":"cursor:sequence","has_more":false}),
                )
            } else {
                let mut request: Value = serde_json::from_slice(&bytes).unwrap();
                let id = request["id"].as_str().unwrap().to_owned();
                let client_revision = request.get("revision").is_some();
                let conflict =
                    request["body"]["text"] == "Conflicting bytes must not allocate a revision.";
                if client_revision || conflict {
                    if matches!(defect, Defect::RejectionAllocates) {
                        allocated += 1;
                    }
                    if matches!(defect, Defect::RejectionWrites) {
                        let mut event = events.last().unwrap().clone();
                        event["id"] = json!("event:rejected");
                        event["sequence"] = json!(99);
                        events.push(event);
                    }
                    let (status, number, code) = if client_revision {
                        ("422 Unprocessable Entity", 422, "invalid_record")
                    } else {
                        ("409 Conflict", 409, "id_conflict")
                    };
                    if matches!(defect, Defect::AcceptedClientRevision) && client_revision {
                        (
                            "200 OK",
                            "application/json",
                            receipts.values().next().unwrap().clone(),
                        )
                    } else {
                        (
                            status,
                            "application/problem+json",
                            json!({"type":format!("https://agentciv.io/problems/{code}"),"title":"Rejected fixture","status":number,"code":code}),
                        )
                    }
                } else if let Some(saved) = receipts.get(&id) {
                    let mut receipt = saved.clone();
                    if matches!(defect, Defect::ChangedRetry) {
                        receipt["event_id"] = json!("event:changed");
                    }
                    if matches!(defect, Defect::AppendedRetry) {
                        let mut event = events.last().unwrap().clone();
                        event["id"] = json!("event:retry");
                        event["sequence"] = json!(99);
                        events.push(event);
                    }
                    ("200 OK", "application/json", receipt)
                } else {
                    allocated += 1;
                    let revision = if matches!(defect, Defect::ResetRevision) && allocated > 1 {
                        allocated - 1
                    } else {
                        allocated
                    };
                    request["revision"] = json!(revision);
                    let event_id = if matches!(defect, Defect::DuplicateId) && allocated == 2 {
                        "event:1".to_owned()
                    } else {
                        format!("event:{allocated}")
                    };
                    let sequence = if matches!(defect, Defect::DuplicateSequence) && allocated == 2
                    {
                        1
                    } else {
                        allocated
                    };
                    let receipt = json!({"protocol_version":"0.1-draft","type":"receipt","world":"civ:sequence","record_id":id,"event_id":event_id,"sequence":sequence,"status":"recorded","artifact_id":ARTIFACT,"revision":revision});
                    if matches!(defect, Defect::WrongArtifact) {
                        request["artifact_id"] = json!("artifact:wrong");
                    }
                    if matches!(defect, Defect::ChangedBytes) {
                        request["body"]["text"] = json!("changed");
                    }
                    events.push(json!({"protocol_version":"0.1-draft","type":"event","id":event_id,"world":"civ:sequence","sequence":sequence,"timestamp":"2026-10-08T00:00:00Z","kind":"artifact.recorded","actor":if matches!(defect, Defect::WrongActor) { "agent:other" } else { "agent:writer" },"body":{"artifact_revision":request}}));
                    receipts.insert(id, receipt.clone());
                    ("200 OK", "application/json", receipt)
                }
            };
            crate::tests::write_response(
                &mut stream,
                status,
                media,
                "Cache-Control: no-store\r\n",
                &response,
            );
        }
    });
    let client = Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let events = Url::parse(&format!("http://{address}/events")).unwrap();
    let ctx = CollaborationRun {
        client: &client,
        world_id: "civ:sequence",
        visibility: "members",
        events: &events,
        collaborate: Url::parse(&format!("http://{address}/collaborate")).unwrap(),
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
    };
    for revision in [1, 2, 3] {
        assert!(request(&ctx, revision).to_string().len() <= 1024);
    }
    let mut cases = Vec::new();
    run(&ctx, &mut cases);
    stop.store(true, Ordering::SeqCst);
    worker.join().unwrap();
    cases
}

#[test]
fn public_revision_sequence_detects_distinct_receipt_event_and_rejection_defects() {
    assert!(
        exercise(Defect::None)
            .iter()
            .all(|case| case.status == CaseStatus::Passed)
    );
    for defect in [
        Defect::ResetRevision,
        Defect::DuplicateId,
        Defect::DuplicateSequence,
        Defect::WrongActor,
        Defect::WrongArtifact,
        Defect::ChangedBytes,
        Defect::OmittedMiddle,
        Defect::ChangedRetry,
        Defect::AppendedRetry,
        Defect::RejectionAllocates,
        Defect::AcceptedClientRevision,
        Defect::RejectionWrites,
    ] {
        let cases = exercise(defect);
        assert_eq!(cases.len(), 3, "{defect:?}");
        assert!(cases.iter().all(|case| case.required));
        assert!(
            cases.iter().any(|case| case.status == CaseStatus::Failed),
            "{defect:?}: {cases:?}"
        );
    }
}
