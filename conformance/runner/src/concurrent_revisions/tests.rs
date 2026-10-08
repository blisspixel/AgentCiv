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
        world_id: "civ:concurrent",
        visibility: "sender_only",
        writer: Party {
            principal: "agent:writer",
            token: "writer-secret",
        },
        reader: Party {
            principal: "agent:reader",
            token: "reader-secret",
        },
        peer: Some(peer()),
        audience: vec!["agent:reader".to_owned(), "agent:peer".to_owned()],
    }
}

fn peer() -> Party<'static> {
    Party {
        principal: "agent:peer",
        token: "peer-secret",
    }
}

fn fixture(ctx: &CollaborationRun<'_>, order: [usize; 4]) -> Race {
    let requests = requests(
        ctx.world_id,
        [ctx.writer.principal, peer().principal],
        &ctx.audience,
    );
    let bytes = requests
        .iter()
        .map(|request| request.to_string().into_bytes())
        .collect();
    let mut receipts = vec![Value::Null; 4];
    let mut histories: [Vec<Value>; 2] = [Vec::new(), Vec::new()];
    let mut revisions = [0; 2];
    for (position, index) in order.into_iter().enumerate() {
        let author = index / 2;
        revisions[author] += 1;
        let revision = revisions[author];
        // Zero and global gaps are permitted; revisions remain contiguous per author.
        let sequence = position * 3;
        let id = format!("event:concurrent-{position}");
        let mut stored = requests[index].clone();
        stored["revision"] = json!(revision);
        let event = json!({"protocol_version":"0.1-draft","type":"event","id":id,
            "world":ctx.world_id,"sequence":sequence,"timestamp":"2026-10-08T00:00:00Z",
            "kind":"artifact.recorded","actor":stored["from"],"body":{"artifact_revision":stored}});
        receipts[index] = json!({"protocol_version":"0.1-draft","type":"receipt","world":ctx.world_id,
            "record_id":requests[index]["id"],"event_id":id,"sequence":sequence,
            "status":"recorded","artifact_id":ARTIFACT,"revision":revision});
        histories[author].push(event);
    }
    Race {
        requests,
        bytes,
        receipts,
        histories,
    }
}

#[test]
fn concurrent_requests_fit_profile_payload_floor_and_accept_either_author_order() {
    let client = Client::new();
    let url = Url::parse("http://127.0.0.1:1/events").unwrap();
    let ctx = context(&client, &url, url.clone());
    for order in [[0, 1, 2, 3], [3, 1, 2, 0], [2, 0, 1, 3], [3, 2, 1, 0]] {
        let race = fixture(&ctx, order);
        assert!(race.bytes.iter().all(|bytes| bytes.len() <= 1024));
        let events = verify_records(&ctx, peer(), &race).unwrap();
        verify_isolation(&events).unwrap();
    }
}

#[derive(Clone, Copy, Debug)]
enum Defect {
    None,
    RepeatedRevision,
    SkippedRevision,
    MergedChains,
    WrongActor,
    WrongArtifact,
    WrongStoredRevision,
    SwappedBytes,
    OmittedEvent,
    CrossPrincipalReceipt,
    DuplicateId,
    DuplicateSequence,
    WrongRevisionOrder,
    ChangedRetry,
    AppendedRetry,
    AllocatingRetry,
    RejectedRequest,
}

fn mutate(race: &mut Race, defect: Defect) {
    // Reverse fixture order means request 0 is the writer's second accepted revision.
    match defect {
        Defect::RepeatedRevision | Defect::SkippedRevision => {
            let revision = if matches!(defect, Defect::RepeatedRevision) {
                1
            } else {
                3
            };
            race.receipts[0]["revision"] = json!(revision);
            race.histories[0][1]["body"]["artifact_revision"]["revision"] = json!(revision);
        }
        Defect::MergedChains => {
            for (request, event, revision) in [(1, 0, 3), (0, 1, 4)] {
                race.receipts[request]["revision"] = json!(revision);
                race.histories[0][event]["body"]["artifact_revision"]["revision"] = json!(revision);
            }
        }
        Defect::WrongActor => race.histories[0][1]["actor"] = json!("agent:peer"),
        Defect::WrongArtifact => {
            race.histories[0][1]["body"]["artifact_revision"]["artifact_id"] =
                json!("artifact:other")
        }
        Defect::WrongStoredRevision => {
            race.histories[0][1]["body"]["artifact_revision"]["revision"] = json!(1)
        }
        Defect::SwappedBytes => {
            let first = race.histories[0][0]["body"]["artifact_revision"]["body"].clone();
            race.histories[0][0]["body"]["artifact_revision"]["body"] =
                race.histories[0][1]["body"]["artifact_revision"]["body"].clone();
            race.histories[0][1]["body"]["artifact_revision"]["body"] = first;
        }
        Defect::OmittedEvent => {
            race.histories[0].remove(0);
        }
        Defect::CrossPrincipalReceipt => {
            race.receipts[0] = race.receipts[2].clone();
        }
        Defect::DuplicateId => {
            for (peer_index, writer_index) in [(3, 1), (2, 0)] {
                let id = race.receipts[writer_index]["event_id"].clone();
                race.receipts[peer_index]["event_id"] = id.clone();
                race.histories[1][if peer_index == 3 { 0 } else { 1 }]["id"] = id;
            }
        }
        Defect::DuplicateSequence => {
            for (peer_index, writer_index) in [(3, 1), (2, 0)] {
                let sequence = race.receipts[writer_index]["sequence"].clone();
                race.receipts[peer_index]["sequence"] = sequence.clone();
                race.histories[1][if peer_index == 3 { 0 } else { 1 }]["sequence"] = sequence;
            }
        }
        Defect::WrongRevisionOrder => {
            race.receipts[0]["revision"] = json!(1);
            race.receipts[1]["revision"] = json!(2);
            race.histories[0][0]["body"]["artifact_revision"]["revision"] = json!(2);
            race.histories[0][1]["body"]["artifact_revision"]["revision"] = json!(1);
        }
        _ => {}
    }
}

fn exercise(defect: Defect) -> Vec<Case> {
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
    let mut race = fixture(&ctx, [3, 2, 1, 0]);
    mutate(&mut race, defect);
    let stop = Arc::new(AtomicBool::new(false));
    let worker_stop = stop.clone();
    let worker = thread::spawn(move || {
        // Delay every initial response until all four client attempts arrive.
        // This checks the fixture actually issues overlapping attempts.
        let mut pending = Vec::new();
        let started = Instant::now();
        while pending.len() < 4 {
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "four attempts did not arrive"
            );
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    let (headers, bytes) = crate::tests::read_raw(&mut stream);
                    assert!(headers.starts_with("POST "));
                    let submitted: Value = serde_json::from_slice(&bytes).unwrap();
                    let index = race
                        .requests
                        .iter()
                        .position(|request| request == &submitted)
                        .unwrap();
                    assert_eq!(bytes, race.bytes[index]);
                    assert!(headers.contains(if index < 2 {
                        "Bearer writer-secret"
                    } else {
                        "Bearer peer-secret"
                    }));
                    pending.push((stream, index));
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2))
                }
                Err(error) => panic!("{error}"),
            }
        }
        for (mut stream, index) in pending {
            if matches!(defect, Defect::RejectedRequest) && index == 0 {
                crate::tests::write_response(
                    &mut stream,
                    "409 Conflict",
                    "application/problem+json",
                    "Cache-Control: no-store\r\n",
                    &json!({"type":"https://agentciv.io/problems/id_conflict","title":"Rejected distinct ID","status":409,"code":"id_conflict"}),
                );
            } else {
                crate::tests::write_response(
                    &mut stream,
                    "200 OK",
                    "application/json",
                    "Cache-Control: no-store\r\n",
                    &race.receipts[index],
                );
            }
        }
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
            let response = if headers.starts_with("GET ") {
                let index = if headers.contains("Bearer writer-secret") {
                    0
                } else {
                    1
                };
                json!({"protocol_version":"0.1-draft","type":"event_page","world":"civ:concurrent",
                    "events":race.histories[index],"next_cursor":"cursor:concurrent","has_more":false})
            } else {
                let submitted: Value = serde_json::from_slice(&bytes).unwrap();
                let index = race
                    .requests
                    .iter()
                    .position(|request| request == &submitted)
                    .unwrap();
                assert_eq!(bytes, race.bytes[index]);
                assert!(headers.contains(if index < 2 {
                    "Bearer writer-secret"
                } else {
                    "Bearer peer-secret"
                }));
                let mut receipt = race.receipts[index].clone();
                if matches!(defect, Defect::ChangedRetry) {
                    receipt["event_id"] = json!("event:changed");
                }
                if matches!(defect, Defect::AllocatingRetry) {
                    receipt["revision"] = json!(3);
                }
                if matches!(defect, Defect::AppendedRetry) {
                    let author = index / 2;
                    let mut event = race.histories[author].last().unwrap().clone();
                    event["id"] = json!(format!("event:retry-{index}"));
                    event["sequence"] = json!(100 + index);
                    race.histories[author].push(event);
                }
                receipt
            };
            crate::tests::write_response(
                &mut stream,
                "200 OK",
                "application/json",
                "Cache-Control: no-store\r\n",
                &response,
            );
        }
    });
    let mut cases = Vec::new();
    run(&ctx, &mut cases);
    stop.store(true, Ordering::SeqCst);
    worker.join().unwrap();
    cases
}

#[test]
fn public_concurrent_cases_detect_distinct_faults_and_keep_required_dependents() {
    assert!(
        exercise(Defect::None)
            .iter()
            .all(|case| case.status == CaseStatus::Passed)
    );
    for (defect, expected) in [
        (Defect::RepeatedRevision, IDS[0]),
        (Defect::SkippedRevision, IDS[0]),
        (Defect::MergedChains, IDS[0]),
        (Defect::WrongActor, IDS[0]),
        (Defect::WrongArtifact, IDS[0]),
        (Defect::WrongStoredRevision, IDS[0]),
        (Defect::SwappedBytes, IDS[0]),
        (Defect::OmittedEvent, IDS[0]),
        (Defect::CrossPrincipalReceipt, IDS[0]),
        (Defect::DuplicateId, IDS[2]),
        (Defect::DuplicateSequence, IDS[2]),
        (Defect::WrongRevisionOrder, IDS[0]),
        (Defect::ChangedRetry, IDS[1]),
        (Defect::AppendedRetry, IDS[1]),
        (Defect::AllocatingRetry, IDS[1]),
        (Defect::RejectedRequest, IDS[0]),
    ] {
        let cases = exercise(defect);
        assert_eq!(cases.len(), 3, "{defect:?}");
        assert!(cases.iter().all(|case| case.required));
        assert_eq!(
            cases
                .iter()
                .find(|case| case.id == expected)
                .unwrap()
                .status,
            CaseStatus::Failed,
            "{defect:?}: {cases:?}"
        );
        assert!(
            !Report {
                cases,
                scope: "credentialed-extended"
            }
            .passed()
        );
    }
}

#[test]
fn missing_peer_fails_before_requests_and_keeps_all_case_rows() {
    let client = Client::builder()
        .timeout(Duration::from_millis(100))
        .build()
        .unwrap();
    let url = Url::parse("http://127.0.0.1:1/events").unwrap();
    let mut ctx = context(&client, &url, url.clone());
    ctx.peer = None;
    let mut cases = Vec::new();
    run(&ctx, &mut cases);
    assert_eq!(cases.len(), 3);
    assert_eq!(cases[0].status, CaseStatus::Failed);
    assert!(cases[0].detail.contains("distinct writing peer"));
    assert!(
        cases[1..]
            .iter()
            .all(|case| case.required && case.status == CaseStatus::Skipped)
    );
}
