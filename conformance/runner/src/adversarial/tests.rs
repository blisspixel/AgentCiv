use super::*;

fn receipt(message: &Value, index: usize) -> Value {
    json!({"event_id": format!("event:{index}"), "sequence": index, "record_id": message["id"]})
}

fn event(message: &Value, receipt: &Value) -> Value {
    json!({"id": receipt["event_id"], "sequence": receipt["sequence"], "actor": message["from"], "kind": "message.recorded", "body": {"message": message}})
}

#[test]
fn exact_race_detects_receipt_disagreement_duplicates_and_wrong_content() {
    let messages = race_messages("civ:test", "agent:writer", RaceKind::Exact);
    let saved = receipt(&messages[0], 1);
    let accepted: Vec<_> = messages
        .iter()
        .map(|message| (message.clone(), saved.clone()))
        .collect();
    let history = vec![event(&messages[0], &saved)];
    verify_race(RaceKind::Exact, &messages, &accepted, 0, &history).unwrap();
    let mut changed = accepted.clone();
    changed[1].1 = receipt(&messages[1], 2);
    assert!(
        verify_race(RaceKind::Exact, &messages, &changed, 0, &history)
            .unwrap_err()
            .contains("different receipts")
    );
    let duplicated = vec![history[0].clone(), history[0].clone()];
    assert!(
        verify_race(RaceKind::Exact, &messages, &accepted, 0, &duplicated)
            .unwrap_err()
            .contains("number of recorded events")
    );
    let mut wrong = history.clone();
    wrong[0]["body"]["message"]["body"] = json!({"text":"different"});
    assert!(
        verify_race(RaceKind::Exact, &messages, &accepted, 0, &wrong)
            .unwrap_err()
            .contains("does not match")
    );
    wrong[0] = history[0].clone();
    wrong[0]["id"] = json!("event:wrong");
    assert!(
        verify_race(RaceKind::Exact, &messages, &accepted, 0, &wrong)
            .unwrap_err()
            .contains("appeared 0 times")
    );
}

#[test]
fn conflict_race_accepts_either_winner_and_rejects_two_winners() {
    let messages = race_messages("civ:test", "agent:writer", RaceKind::Conflict);
    for winner in &messages {
        let saved = receipt(winner, 1);
        let accepted = vec![(winner.clone(), saved.clone())];
        let history = vec![event(winner, &saved)];
        verify_race(RaceKind::Conflict, &messages, &accepted, 1, &history).unwrap();
        assert!(
            verify_race(RaceKind::Conflict, &messages, &accepted, 0, &history)
                .unwrap_err()
                .contains("success/conflict count")
        );
    }
    let accepted: Vec<_> = messages
        .iter()
        .enumerate()
        .map(|(index, message)| (message.clone(), receipt(message, index + 1)))
        .collect();
    let history: Vec<_> = accepted
        .iter()
        .map(|(message, saved)| event(message, saved))
        .collect();
    assert!(verify_race(RaceKind::Conflict, &messages, &accepted, 0, &history).is_err());
}

#[test]
fn distinct_race_checks_unique_events_sequences_and_receipt_correlation() {
    let messages = race_messages("civ:test", "agent:writer", RaceKind::Distinct);
    let accepted: Vec<_> = messages
        .iter()
        .enumerate()
        .map(|(index, message)| (message.clone(), receipt(message, index + 1)))
        .collect();
    let history: Vec<_> = accepted
        .iter()
        .map(|(message, saved)| event(message, saved))
        .collect();
    verify_race(RaceKind::Distinct, &messages, &accepted, 0, &history).unwrap();
    let mut duplicate_sequence = history.clone();
    let mut changed_receipts = accepted.clone();
    duplicate_sequence[1]["sequence"] = json!(1);
    changed_receipts[1].1["sequence"] = json!(1);
    assert!(
        verify_race(
            RaceKind::Distinct,
            &messages,
            &changed_receipts,
            0,
            &duplicate_sequence
        )
        .unwrap_err()
        .contains("reused")
    );
    let mut wrong_actor = history.clone();
    wrong_actor[0]["actor"] = json!("agent:other");
    assert!(verify_race(RaceKind::Distinct, &messages, &accepted, 0, &wrong_actor).is_err());
    let mut duplicate_id = history.clone();
    duplicate_id[1]["id"] = duplicate_id[0]["id"].clone();
    assert!(verify_race(RaceKind::Distinct, &messages, &accepted, 0, &duplicate_id).is_err());
}

fn context<'a>(visibility: &'a str, client: &'a Client, url: &'a Url) -> CollaborationRun<'a> {
    CollaborationRun {
        client,
        world_id: "civ:test",
        visibility,
        events: url,
        collaborate: url.clone(),
        writer: Party {
            principal: "agent:writer",
            token: "writer-token",
        },
        reader: Party {
            principal: "agent:reader",
            token: "reader-token",
        },
        peer: None,
        audience: vec!["agent:reader".to_owned()],
    }
}

#[test]
fn hidden_cases_skip_only_for_member_visibility_and_require_a_peer() {
    let client = Client::new();
    let url = Url::parse("http://127.0.0.1:9/events").unwrap();
    let mut cases = Vec::new();
    run_hidden_cases(&context("members", &client, &url), &mut cases);
    assert_eq!(cases.len(), 5);
    assert!(
        cases
            .iter()
            .all(|case| case.status == CaseStatus::Skipped && !case.required)
    );
    cases.clear();
    run_hidden_cases(&context("addressed", &client, &url), &mut cases);
    assert!(
        cases
            .iter()
            .all(|case| case.status == CaseStatus::Failed && case.required)
    );
}

#[test]
fn hidden_requests_are_valid_records_and_do_not_address_the_source_to_the_peer() {
    let client = Client::new();
    let url = Url::parse("http://127.0.0.1:9/events").unwrap();
    let ctx = context("addressed", &client, &url);
    let peer = Party {
        principal: "agent:peer",
        token: "peer-token",
    };
    for (kind, schema) in [
        (
            "artifact_revision",
            include_str!("../../../../schemas/collaboration-artifact.schema.json"),
        ),
        (
            "objection",
            include_str!("../../../../schemas/collaboration-objection.schema.json"),
        ),
        (
            "decline",
            include_str!("../../../../schemas/collaboration-decline.schema.json"),
        ),
        (
            "withdrawal",
            include_str!("../../../../schemas/collaboration-withdrawal.schema.json"),
        ),
    ] {
        let record = hidden_record(&ctx, peer, kind);
        validate(schema, &record).unwrap();
        assert!(record.to_string().len() <= 1024);
        assert_eq!(record["from"], "agent:peer");
    }
    let receipt = json!({"event_id":"event:hidden"});
    ensure_hidden(&[], &receipt).unwrap();
    assert!(ensure_hidden(&[json!({"id":"event:hidden", "body":{}})], &receipt).is_err());
    assert!(ensure_hidden(&[json!({"id":"event:other", "body":{"artifact_revision":{"artifact_id":"artifact:hidden-source"}}})], &receipt).is_err());
}

#[test]
fn hidden_source_requires_exact_first_revision_and_correlated_receipt() {
    let source = artifact_submission(
        "civ:test",
        "agent:writer",
        &["agent:writer".to_owned()],
        "submission:hidden-source",
        "artifact:hidden-source",
        "Private source content.",
    );
    let receipt = json!({"world":"civ:test","record_id":"submission:hidden-source","artifact_id":"artifact:hidden-source","revision":1,"event_id":"event:source","sequence":0});
    let mut stored = source.clone();
    stored["revision"] = json!(1);
    let event = json!({"id":"event:source","sequence":0,"world":"civ:test","actor":"agent:writer","kind":"artifact.recorded","body":{"artifact_revision":stored}});
    validate_hidden_source(&source, &receipt, &event, "agent:writer").unwrap();
    for mutation in 0..10 {
        let mut changed_event = event.clone();
        let mut changed_receipt = receipt.clone();
        match mutation {
            0 => changed_event["body"]["artifact_revision"]["revision"] = json!(2),
            1 => changed_event["body"]["artifact_revision"]["body"] = json!({"text":"different"}),
            2 => changed_event["sequence"] = json!(1),
            3 => changed_event["actor"] = json!("agent:peer"),
            4 => changed_event["kind"] = json!("artifact.withdrawn"),
            5 => changed_event["world"] = json!("civ:other"),
            6 => changed_event["id"] = json!("event:other"),
            7 => changed_receipt["revision"] = json!(2),
            8 => changed_receipt["record_id"] = json!("submission:other"),
            _ => changed_event["body"]["artifact_revision"]["to"] = json!(["agent:peer"]),
        }
        assert!(
            validate_hidden_source(&source, &changed_receipt, &changed_event, "agent:writer")
                .is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn wrong_hidden_source_revision_stops_before_privacy_requests() {
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let mut saved = Value::Null;
        for index in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let (headers, bytes) = crate::tests::read_raw(&mut stream);
            assert!(headers.contains("Bearer writer-token"));
            let body = if index == 0 {
                assert!(headers.starts_with("POST /collaborate "));
                saved = serde_json::from_slice(&bytes).unwrap();
                json!({"protocol_version":"0.1-draft","type":"receipt","world":"civ:test","record_id":"submission:hidden-source","artifact_id":"artifact:hidden-source","revision":1,"event_id":"event:source","sequence":0,"status":"recorded"})
            } else {
                assert!(headers.starts_with("GET /events "));
                saved["revision"] = json!(2);
                json!({"protocol_version":"0.1-draft","type":"event_page","world":"civ:test","events":[{"protocol_version":"0.1-draft","type":"event","id":"event:source","sequence":0,"world":"civ:test","timestamp":"2026-09-30T00:00:00Z","actor":"agent:writer","kind":"artifact.recorded","body":{"artifact_revision":saved}}],"next_cursor":"cursor:end","has_more":false})
            };
            crate::tests::write_response(
                &mut stream,
                "200 OK",
                "application/json",
                "Cache-Control: no-store\r\n",
                &body,
            );
        }
    });
    let client = Client::new();
    let events = Url::parse(&format!("http://{address}/events")).unwrap();
    let mut ctx = context("addressed", &client, &events);
    ctx.collaborate = Url::parse(&format!("http://{address}/collaborate")).unwrap();
    ctx.peer = Some(Party {
        principal: "agent:peer",
        token: "peer-token",
    });
    let mut cases = Vec::new();
    run_hidden_cases(&ctx, &mut cases);
    worker.join().unwrap();
    assert_eq!(cases[0].status, CaseStatus::Failed);
    assert!(
        cases[0]
            .detail
            .contains("hidden source event does not match")
    );
    assert!(
        cases[1..]
            .iter()
            .all(|case| case.required && case.status == CaseStatus::Skipped)
    );
}
