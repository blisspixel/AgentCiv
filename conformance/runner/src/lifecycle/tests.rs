use super::*;

fn parties() -> [(&'static str, &'static str); 3] {
    [
        ("agent:writer", "writer-secret"),
        ("agent:reader", "reader-secret"),
        ("agent:peer", "peer-secret"),
    ]
}

fn checkpoint() -> Value {
    let mut world: Value =
        serde_json::from_str(include_str!("../../../fixtures/valid/world.json")).unwrap();
    world["history"]["visibility"] = json!("members");
    world["capabilities"]
        .as_array_mut()
        .unwrap()
        .push(json!("collaboration.submit"));
    let world_id = world["id"].as_str().unwrap();
    let names: Vec<_> = parties().iter().map(|party| party.0).collect();
    let audience: Vec<_> = names.iter().map(|name| (*name).to_owned()).collect();
    let race = concurrent_requests(world_id, names[0], names[2], &audience);
    // Independent principal chains may arrive in either client request order.
    let mut requests = initial_requests(world_id, &names);
    requests.extend([
        race[2].clone(),
        race[0].clone(),
        race[3].clone(),
        race[1].clone(),
    ]);
    let mut records = Vec::new();
    let mut events = Vec::new();
    let mut revisions = std::collections::HashMap::<(String, String), u64>::new();
    for (index, body) in requests.into_iter().enumerate() {
        let principal = body["from"].as_str().unwrap();
        let kind = body["type"].as_str().unwrap();
        let sequence = if index == 5 {
            3
        } else if index >= 6 {
            index
        } else {
            index + 1
        };
        let mut receipt = json!({"protocol_version":"0.1-draft","type":"receipt","world":world_id,"record_id":body["id"],"event_id":format!("event:{sequence}"),"sequence":sequence,"status":"recorded"});
        let mut stored = body.clone();
        let (field, event_kind) = match kind {
            "message" => ("message", "message.recorded"),
            "artifact_revision" => {
                let revision = revisions
                    .entry((
                        principal.to_owned(),
                        body["artifact_id"].as_str().unwrap().to_owned(),
                    ))
                    .or_default();
                *revision += 1;
                stored["revision"] = json!(*revision);
                receipt["artifact_id"] = body["artifact_id"].clone();
                receipt["revision"] = json!(*revision);
                ("artifact_revision", "artifact.recorded")
            }
            "objection" => ("objection", "objection.recorded"),
            "decline" => ("decline", "decline.recorded"),
            _ => ("withdrawal", "artifact.withdrawn"),
        };
        let event = json!({"protocol_version":"0.1-draft","type":"event","id":format!("event:{sequence}"),"world":world_id,"sequence":sequence,"timestamp":"2026-09-29T00:00:00Z","kind":event_kind,"actor":principal,"body":if kind=="withdrawal" {json!({})} else {json!({field:stored})}});
        if index == 5 {
            events[2] = event.clone();
        } else {
            events.push(event.clone());
        }
        records.push(json!({"operation":if kind=="message" {"submit"} else {"collaborate"},"principal":principal,"bytes":body.to_string(),"receipt":receipt,"event_at_recording":event}));
    }
    json!({"checkpoint_version":CHECKPOINT_VERSION,"world":world,"principals":parties().map(|party|party.0),"reader_cursor":"cursor:before","records":records,"history":{"agent:writer":events,"agent:reader":events,"agent:peer":events}})
}

#[test]
fn checkpoint_validation_rejects_malformed_or_mismatched_evidence() {
    let saved = checkpoint();
    validate_checkpoint(&saved).unwrap();
    for (field, value) in [
        ("checkpoint_version", json!("other")),
        ("principals", json!(null)),
        (
            "principals",
            json!(["agent:writer", "agent:writer", "agent:peer"]),
        ),
        ("reader_cursor", json!("")),
        ("records", json!([])),
        ("history", json!({})),
    ] {
        let mut changed = saved.clone();
        changed[field] = value;
        assert!(validate_checkpoint(&changed).is_err(), "{field}");
    }
    for (field, value) in [
        ("operation", json!("http://elsewhere.invalid")),
        ("principal", json!("agent:outsider")),
        ("bytes", json!(null)),
        ("bytes", json!("not-json")),
        ("bytes", json!("{}")),
        ("receipt", json!({})),
        ("event_at_recording", json!({})),
    ] {
        let mut changed = saved.clone();
        changed["records"][0][field] = value;
        assert!(validate_checkpoint(&changed).is_err(), "{field}");
    }
    let mut changed = saved;
    changed["history"]["agent:writer"][0]["world"] = json!("civ:other");
    assert!(validate_checkpoint(&changed).is_err());
}

#[test]
fn checkpoint_revision_layout_and_old_version_are_rejected_before_requests() {
    let saved = checkpoint();
    for index in [7, 8] {
        for revision in [1, 4] {
            let mut changed = saved.clone();
            changed["records"][index]["receipt"]["revision"] = json!(revision);
            assert!(validate_checkpoint(&changed).is_err());
        }
        let mut changed = saved.clone();
        let mut body: Value =
            serde_json::from_str(changed["records"][index]["bytes"].as_str().unwrap()).unwrap();
        body["artifact_id"] = json!("artifact:other");
        changed["records"][index]["bytes"] = json!(body.to_string());
        assert!(validate_checkpoint(&changed).is_err());
        let mut changed = saved.clone();
        changed["records"][index]["bytes"] = json!("{}");
        assert!(validate_checkpoint(&changed).is_err());
    }
    let mut changed = saved.clone();
    changed["records"].as_array_mut().unwrap().swap(7, 8);
    assert!(validate_checkpoint(&changed).is_err());
    for alteration in ["spacing", "duplicate_member"] {
        let mut changed = saved.clone();
        let original = changed["records"][7]["bytes"].as_str().unwrap();
        changed["records"][7]["bytes"] = json!(if alteration == "spacing" {
            format!(" {original}")
        } else {
            original.replacen('{', "{\"type\":\"message\",", 1)
        });
        assert!(validate_checkpoint(&changed).is_err(), "{alteration}");
    }
    for version in [
        "agentciv-lifecycle/0.1-draft",
        "agentciv-lifecycle/0.2-draft",
    ] {
        let mut changed = saved.clone();
        changed["checkpoint_version"] = json!(version);
        assert_eq!(
            validate_checkpoint(&changed).unwrap_err(),
            "unsupported checkpoint version"
        );
    }
}

fn synchronize_record_event(saved: &mut Value, index: usize) {
    let event = saved["records"][index]["event_at_recording"].clone();
    for history in saved["history"].as_object_mut().unwrap().values_mut() {
        let found = history
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|old| old["id"] == event["id"])
            .unwrap();
        *found = event.clone();
    }
}

#[test]
fn concurrent_checkpoint_accepts_alternate_actual_order_without_contiguous_global_sequences() {
    let mut saved = checkpoint();
    let original = saved["records"].as_array().unwrap().clone();
    for (position, old) in [10, 12, 9, 11].into_iter().enumerate() {
        let index = 9 + position;
        saved["records"][index] = original[old].clone();
        // Gaps are allowed in global event numbering; chain revisions are not.
        let sequence = 20 + position * 3;
        saved["records"][index]["receipt"]["sequence"] = json!(sequence);
        saved["records"][index]["event_at_recording"]["sequence"] = json!(sequence);
        synchronize_record_event(&mut saved, index);
    }
    for history in saved["history"].as_object_mut().unwrap().values_mut() {
        history
            .as_array_mut()
            .unwrap()
            .sort_by_key(|event| event["sequence"].as_u64().unwrap());
    }
    validate_checkpoint(&saved).unwrap();
}

#[test]
fn lifecycle_requests_fit_the_profile_payload_floor() {
    let names = [
        "agent:conformance-writer",
        "agent:conformance-reader",
        "agent:conformance-peer",
    ];
    let audience: Vec<_> = names.iter().map(|name| (*name).to_owned()).collect();
    let mut requests = initial_requests("civ:conformance", &names);
    requests.extend(concurrent_requests(
        "civ:conformance",
        names[0],
        names[2],
        &audience,
    ));
    assert_eq!(requests.len(), 13);
    for request in requests {
        assert!(request.to_string().len() <= 1024, "{}", request["id"]);
    }
}

#[test]
fn concurrent_checkpoint_rejects_coherent_wrong_numbers_bodies_and_rosters() {
    let saved = checkpoint();
    for (index, revision) in [(12, 4), (12, 6), (10, 1), (9, 4), (11, 1)] {
        let mut changed = saved.clone();
        changed["records"][index]["receipt"]["revision"] = json!(revision);
        changed["records"][index]["event_at_recording"]["body"]["artifact_revision"]["revision"] =
            json!(revision);
        synchronize_record_event(&mut changed, index);
        assert!(validate_checkpoint(&changed).is_err(), "{index}:{revision}");
    }
    let mut reversed = saved.clone();
    for (index, revision) in [(10, 5), (12, 4)] {
        reversed["records"][index]["receipt"]["revision"] = json!(revision);
        reversed["records"][index]["event_at_recording"]["body"]["artifact_revision"]["revision"] =
            json!(revision);
        synchronize_record_event(&mut reversed, index);
    }
    assert!(validate_checkpoint(&reversed).is_err());
    for field in ["id", "from", "artifact_id", "body"] {
        let mut changed = saved.clone();
        let mut body: Value =
            serde_json::from_str(changed["records"][9]["bytes"].as_str().unwrap()).unwrap();
        body[field] = match field {
            "body" => json!({"text":"substituted evidence"}),
            "from" => json!("agent:writer"),
            _ => json!("substituted:identity"),
        };
        changed["records"][9]["bytes"] = json!(body.to_string());
        let revision = changed["records"][9]["receipt"]["revision"].clone();
        body["revision"] = revision;
        changed["records"][9]["event_at_recording"]["body"]["artifact_revision"] = body;
        synchronize_record_event(&mut changed, 9);
        assert!(validate_checkpoint(&changed).is_err(), "{field}");
    }
    let mut reordered = saved.clone();
    reordered["records"].as_array_mut().unwrap().swap(9, 10);
    assert!(validate_checkpoint(&reordered).is_err());
    let mut dropped = saved.clone();
    dropped["records"].as_array_mut().unwrap().remove(10);
    assert!(validate_checkpoint(&dropped).is_err());
    for mutation in ["receipt", "stored", "identity", "sequence"] {
        let mut changed = saved.clone();
        match mutation {
            "receipt" => changed["records"][9]["receipt"]["event_id"] = json!("event:other"),
            "stored" => {
                changed["records"][9]["event_at_recording"]["body"]["artifact_revision"]["revision"] =
                    json!(2)
            }
            "identity" => {
                changed["records"][10]["event_at_recording"]["id"] =
                    changed["records"][9]["event_at_recording"]["id"].clone()
            }
            _ => {
                changed["records"][10]["event_at_recording"]["sequence"] =
                    changed["records"][9]["event_at_recording"]["sequence"].clone()
            }
        }
        assert!(validate_checkpoint(&changed).is_err(), "{mutation}");
    }
}

#[test]
fn world_comparison_allows_new_endpoints_but_rejects_changed_contract() {
    let saved = checkpoint();
    let mut world = saved["world"].clone();
    world["endpoints"]["events"] = json!("http://127.0.0.1:1234/events");
    check_world(&saved, &world, &parties(), "verify").unwrap();
    world["history"]["visibility"] = json!("sender_only");
    assert!(check_world(&saved, &world, &parties(), "verify").is_err());
    check_world(&saved, &world, &parties(), "policy").unwrap();
    world["id"] = json!("civ:other");
    assert!(check_world(&saved, &world, &parties(), "policy").is_err());
    assert!(check_world(&saved, &saved["world"], &parties(), "policy").is_err());
    assert!(compare_history(&json!([1]), &json!([2])).is_err());
    compare_history(&json!([1]), &json!([1])).unwrap();
}

#[test]
fn lifecycle_input_failures_leave_required_cases_unpassed_and_redact_credentials() {
    let missing = std::env::temp_dir().join(format!(
        "agentciv-missing-checkpoint-{}",
        std::process::id()
    ));
    let parties = parties();
    let run = |phase, peer| {
        run_lifecycle(
            "http://127.0.0.1:9/discovery",
            parties[0],
            parties[1],
            peer,
            phase,
            &missing,
        )
    };
    assert!(!run("invalid", Some(parties[2])).passed());
    let report = run("prepare", None);
    assert!(!report.passed());
    assert_eq!(report.cases.len(), 4);
    assert_eq!(report.cases[0].status, CaseStatus::Failed);
    assert!(
        report.cases[1..]
            .iter()
            .all(|case| case.required && case.status == CaseStatus::Skipped)
    );
    let duplicate = run("prepare", Some(parties[0]));
    assert!(!duplicate.passed());
    let report = run("verify", Some(parties[2]));
    assert!(!report.passed());
    assert!(!report.to_json().to_string().contains("writer-secret"));
    let existing = std::env::current_exe().unwrap();
    assert!(
        !run_lifecycle(
            "http://127.0.0.1:9/discovery",
            parties[0],
            parties[1],
            Some(parties[2]),
            "prepare",
            &existing
        )
        .passed()
    );
    assert!(load_checkpoint(&existing).is_err());
}

#[test]
fn seed_validation_rejects_missing_events_wrong_receipts_and_changed_tombstones() {
    let saved = checkpoint();
    verify_seed(
        &saved["history"],
        saved["records"].as_array().unwrap(),
        &parties(),
    )
    .unwrap();
    for mutation in 0..7 {
        let mut changed = saved.clone();
        match mutation {
            0 => changed["history"] = json!({}),
            1 => {
                changed["history"]["agent:reader"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            }
            2 => changed["records"][0]["receipt"]["sequence"] = json!(20),
            3 => {
                changed["records"][5]["event_at_recording"]["timestamp"] =
                    json!("2026-09-30T00:00:00Z")
            }
            4 => changed["history"]["agent:writer"][2]["body"] = json!({"text":"leak"}),
            5 => changed["records"][1]["event_at_recording"]["sequence"] = json!(1),
            _ => changed["history"]["agent:peer"][0]["body"] = json!({"text":"changed"}),
        }
        assert!(
            verify_seed(
                &changed["history"],
                changed["records"].as_array().unwrap(),
                &parties()
            )
            .is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn checkpoint_validation_checks_endpoint_schema_identity_and_history_before_requests() {
    let saved = checkpoint();
    for mutation in 0..8 {
        let mut changed = saved.clone();
        match mutation {
            0 => changed["records"][2]["operation"] = json!("submit"),
            1 => changed["records"][0]["event_at_recording"]["id"] = json!("event:wrong"),
            2 => changed["records"][0]["event_at_recording"]["actor"] = json!("agent:peer"),
            3 => {
                changed["records"][0]["event_at_recording"]["body"]["message"]["body"] =
                    json!({"text":"forged"})
            }
            4 => changed["records"][2]["receipt"]["revision"] = json!(2),
            5 => changed["records"][1] = changed["records"][0].clone(),
            6 => {
                let mut body: Value =
                    serde_json::from_str(changed["records"][0]["bytes"].as_str().unwrap()).unwrap();
                body["to"] = json!([]);
                changed["records"][0]["bytes"] = json!(body.to_string());
            }
            _ => changed["history"]["agent:reader"][0]["sequence"] = json!(99),
        }
        assert!(validate_checkpoint(&changed).is_err(), "{mutation}");
    }
    let report = run_lifecycle(
        "https://remote.example.invalid/discovery",
        parties()[0],
        parties()[1],
        Some(parties()[2]),
        "prepare",
        &std::env::temp_dir().join("agentciv-never-created-checkpoint.json"),
    );
    assert!(!report.passed());
    assert!(
        report
            .cases
            .iter()
            .any(|case| case.detail.contains("loopback"))
    );
}

#[test]
fn echoed_credentials_are_rejected_before_checkpoint_creation() {
    ensure_no_credentials(b"{\"safe\":true}", &parties()).unwrap();
    assert!(ensure_no_credentials(b"{\"host_echo\":\"writer-secret\"}", &parties()).is_err());
    let token = "secret\\with\"quotes";
    let encoded = serde_json::to_vec(&json!({"host_echo":token})).unwrap();
    assert!(ensure_no_credentials(&encoded, &[("agent:writer", token)]).is_err());
}

#[test]
fn lifecycle_seed_accepts_sequence_zero_and_rejects_non_increasing_sequences() {
    let mut saved = checkpoint();
    for record in saved["records"].as_array_mut().unwrap() {
        for field in ["receipt", "event_at_recording"] {
            let value = record[field]["sequence"].as_u64().unwrap();
            record[field]["sequence"] = json!(value - 1);
        }
    }
    for events in saved["history"].as_object_mut().unwrap().values_mut() {
        for event in events.as_array_mut().unwrap() {
            let value = event["sequence"].as_u64().unwrap();
            event["sequence"] = json!(value - 1);
        }
    }
    validate_checkpoint(&saved).unwrap();
    for next in [0, 2] {
        let mut changed = saved.clone();
        let index = if next == 0 { 1 } else { 0 };
        changed["records"][index]["event_at_recording"]["sequence"] = json!(next);
        changed["records"][index]["receipt"]["sequence"] = json!(next);
        for events in changed["history"].as_object_mut().unwrap().values_mut() {
            events[index]["sequence"] = json!(next);
        }
        assert!(
            verify_seed(
                &changed["history"],
                changed["records"].as_array().unwrap(),
                &parties()
            )
            .unwrap_err()
            .contains("repeat identity or sequence")
        );
    }
}
