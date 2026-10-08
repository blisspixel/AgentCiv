use super::*;
use crate::tests::{WORLD, event};

fn offer_event(id: &str, sequence: u64, author: &str, revision: u64) -> String {
    let mut event: Value = serde_json::from_str(&event(id, sequence, author)).unwrap();
    event["body"]["artifact_revision"]["revision"] = json!(revision);
    event["body"]["artifact_revision"]["body"]["activity_offer"] =
        serde_json::from_str(include_str!("../../fixtures/offer-valid.json")).unwrap();
    event["body"]["artifact_revision"]["body"]["activity_offer"]["source_refs"] = json!([]);
    serde_json::to_string_pretty(&event).unwrap()
}

fn input(records: &[String]) -> Value {
    json!({"snapshot":{"world":WORLD,"records":records},"report":{"pages":1,"events":records.len(),"response_bytes":records.iter().map(String::len).sum::<usize>()+200,"reached_end":true,"scope":"current_caller_view","copying_permission":"not_granted"}})
}

fn project_records(records: &[String], query: &str) -> Value {
    project(&input(records).to_string(), query).unwrap()
}

#[test]
fn documented_positive_and_negative_offer_fixtures_are_separate_from_wire_validation() {
    let valid: Value =
        serde_json::from_str(include_str!("../../fixtures/offer-valid.json")).unwrap();
    assert!(valid_offer(&valid).is_some());
    let invalid: Value =
        serde_json::from_str(include_str!("../../fixtures/offer-invalid.json")).unwrap();
    assert!(valid_offer(&invalid).is_none());
    for (field, value) in [
        ("format", json!("other")),
        ("title", json!("")),
        ("purpose", json!("bad\ncontrol")),
        ("limitations", json!("x".repeat(1025))),
        ("unknown", json!(true)),
    ] {
        let mut candidate = valid.clone();
        candidate[field] = value;
        assert!(valid_offer(&candidate).is_none());
    }
    for mutation in [
        json!({"event_id":"x","from":"a","artifact_id":"z","revision":0}),
        json!({"event_id":"x","from":"a","artifact_id":"z","revision":9_007_199_254_740_992_u64}),
        json!({"event_id":"x","from":"a","artifact_id":"z","revision":1,"extra":true}),
        json!({"event_id":"","from":"a","artifact_id":"z","revision":1}),
    ] {
        let mut candidate = valid.clone();
        candidate["source_refs"] = json!([mutation]);
        assert!(valid_offer(&candidate).is_none());
    }
    let mut duplicate = valid;
    duplicate["source_refs"] = json!([duplicate["source_refs"][0], duplicate["source_refs"][0]]);
    assert!(valid_offer(&duplicate).is_none());
}

#[test]
fn unicode_queries_are_exact_and_bounded_without_normalization_or_regex() {
    let mut event: Value =
        serde_json::from_str(&offer_event("event:unicode", 1, "agent:a", 1)).unwrap();
    event["body"]["artifact_revision"]["body"]["activity_offer"]["title"] =
        json!("问好 café .* 𠀀");
    event["body"]["artifact_revision"]["body"]["text"] = json!("Published separate text");
    let original = serde_json::to_string_pretty(&event).unwrap();
    for query in ["问好", "café", ".*", "𠀀", "Published"] {
        let view = project_records(std::slice::from_ref(&original), query);
        assert_eq!(view["offers"][0]["record_utf8"], original);
        assert_eq!(view["report"]["shortened_fields"], json!([]));
    }
    for query in ["CAFÉ", "cafe\u{301}", "unmatched"] {
        assert_eq!(
            project_records(std::slice::from_ref(&original), query)["offers"],
            json!([])
        );
    }
    assert!(project(&input(&[]).to_string(), &"𠀀".repeat(128)).is_ok());
    for query in [
        "x".repeat(129),
        "𠀀".repeat(129),
        "line\n".to_owned(),
        "\u{0}".to_owned(),
    ] {
        assert_eq!(
            project(&input(&[]).to_string(), &query),
            Err(Error::InvalidQuery)
        );
    }
}

#[test]
fn author_chains_do_not_collide_and_latest_non_offer_or_malformed_offer_suppresses_old() {
    let first = offer_event("event:a1", 1, "agent:a", 1);
    let other = offer_event("event:b1", 2, "agent:b", 1);
    let latest = offer_event("event:a2", 3, "agent:a", 2);
    let view = project_records(&[first.clone(), other.clone(), latest.clone()], "");
    assert_eq!(view["offers"][0]["event_id"], "event:a2");
    assert_eq!(view["offers"][1]["event_id"], "event:b1");
    assert_eq!(
        view["offers"][0]["earlier_revisions"][0]["record_utf8"],
        first
    );
    assert_eq!(view["report"]["omissions"]["superseded_offer_revisions"], 1);
    let mut malformed: Value = serde_json::from_str(&latest).unwrap();
    malformed["body"]["artifact_revision"]["body"]["activity_offer"] =
        json!({"title":"incomplete"});
    let view = project_records(&[first.clone(), other.clone(), malformed.to_string()], "");
    assert_eq!(view["offers"].as_array().unwrap().len(), 1);
    assert_eq!(view["report"]["omissions"]["malformed_latest_offers"], 1);
    malformed["body"]["artifact_revision"]["body"] = json!({"text":"This is no longer an offer."});
    assert_eq!(
        project_records(&[first, other, malformed.to_string()], "")["offers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn missing_mismatched_withdrawn_and_available_sources_stay_distinct_without_erased_content() {
    let source = event("event:source", 1, "agent:source");
    let mut tombstone: Value =
        serde_json::from_str(&event("event:withdrawn", 2, "agent:old")).unwrap();
    tombstone["kind"] = json!("artifact.withdrawn");
    tombstone["body"] = json!({});
    let mut offer: Value =
        serde_json::from_str(&offer_event("event:offer", 3, "agent:a", 1)).unwrap();
    offer["body"]["artifact_revision"]["body"]["activity_offer"]["source_refs"] = json!([
        {"event_id":"event:source","from":"agent:source","artifact_id":"artifact:same-name","revision":1},
        {"event_id":"event:source","from":"agent:impostor","artifact_id":"artifact:same-name","revision":1},
        {"event_id":"event:missing","from":"agent:hidden","artifact_id":"artifact:secret","revision":1},
        {"event_id":"event:withdrawn","from":"agent:old","artifact_id":"artifact:same-name","revision":1}]);
    let view = project_records(
        &[source.clone(), tombstone.to_string(), offer.to_string()],
        "",
    );
    let sources = &view["offers"][0]["sources"];
    assert_eq!(sources[0]["status"], "available");
    assert_eq!(sources[0]["record_utf8"], source);
    assert_eq!(sources[1]["status"], "unavailable");
    assert!(sources[1].get("record_utf8").is_none());
    assert_eq!(sources[2]["status"], "unavailable");
    assert_eq!(sources[3]["status"], "withdrawn");
    assert_eq!(
        serde_json::from_str::<Value>(sources[3]["record_utf8"].as_str().unwrap()).unwrap()["body"],
        json!({})
    );
}

#[test]
fn related_objections_declines_and_exact_derivation_are_inspectable_and_unrelated_are_absent() {
    let source = event("event:source", 1, "agent:source");
    let mut offer: Value =
        serde_json::from_str(&offer_event("event:offer", 2, "agent:a", 1)).unwrap();
    offer["body"]["artifact_revision"]["derived_from"] =
        json!({"from":"agent:source","artifact_id":"artifact:same-name","revision":1});
    let fixture: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../examples/inheritance/history.json"
    ))
    .unwrap();
    let mut records = vec![source.clone(), offer.to_string()];
    for (offset, fixture_index, key) in [(3, 2, "objection"), (4, 4, "decline")] {
        let mut related = fixture[fixture_index].clone();
        related["world"] = json!(WORLD);
        related["sequence"] = json!(offset);
        related["body"][key]["world"] = json!(WORLD);
        related["body"][key]["target_from"] = json!("agent:source");
        related["body"][key]["artifact_id"] = json!("artifact:same-name");
        records.push(related.to_string());
    }
    let mut unrelated: Value = serde_json::from_str(&records[2]).unwrap();
    unrelated["id"] = json!("event:unrelated");
    unrelated["sequence"] = json!(5);
    unrelated["body"]["objection"]["target_from"] = json!("agent:unrelated");
    records.push(unrelated.to_string());
    let view = project_records(&records, "");
    assert_eq!(
        view["offers"][0]["related_records"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        view["offers"][0]["derivation"]["original"]["record_utf8"],
        source
    );
    offer["body"]["artifact_revision"]["derived_from"]["revision"] = json!(2);
    records[1] = offer.to_string();
    assert_eq!(
        project_records(&records, "")["offers"][0]["derivation"]["status"],
        "unavailable"
    );
}

#[test]
fn later_tombstones_conservatively_prevent_offer_resurrection_without_guessing_chain() {
    let a = offer_event("event:a", 1, "agent:a", 1);
    let b = offer_event("event:b", 2, "agent:b", 1);
    let mut tombstone: Value =
        serde_json::from_str(&event("event:withdrawn", 3, "agent:a")).unwrap();
    tombstone["kind"] = json!("artifact.withdrawn");
    tombstone["body"] = json!({});
    let view = project_records(&[a.clone(), b.clone(), tombstone.to_string()], "");
    assert_eq!(view["offers"].as_array().unwrap().len(), 1);
    assert_eq!(view["offers"][0]["from"], "agent:b");
    assert_eq!(view["report"]["omissions"]["uncertain_chain_offers"], 1);
    assert_eq!(view["tombstones"][0]["record_utf8"], tombstone.to_string());
    tombstone.as_object_mut().unwrap().remove("actor");
    let view = project_records(&[a, b, tombstone.to_string()], "");
    assert_eq!(view["offers"], json!([]));
    assert_eq!(view["report"]["omissions"]["uncertain_chain_offers"], 2);
}

#[test]
fn partial_or_malformed_input_never_produces_a_view_and_all_records_are_validated() {
    let record = offer_event("event:a", 1, "agent:a", 1);
    let valid = input(std::slice::from_ref(&record));
    for (field, value) in [
        ("reached_end", json!(false)),
        ("scope", json!("global")),
        ("copying_permission", json!("granted")),
        ("pages", json!(0)),
        ("pages", json!(257)),
        ("events", json!(2)),
        ("response_bytes", json!(0)),
        ("response_bytes", json!(MAX_INPUT_BYTES + 1)),
    ] {
        let mut input = valid.clone();
        input["report"][field] = value;
        assert!(project(&input.to_string(), "").is_err());
    }
    assert_eq!(project("{", ""), Err(Error::InvalidJson));
    assert_eq!(
        project(&"x".repeat(MAX_INPUT_BYTES + 1), ""),
        Err(Error::InputLimit)
    );
    assert_eq!(project("{}", ""), Err(Error::InvalidView));
    assert_eq!(
        project("{\"snapshot\":{},\"snapshot\":{},\"report\":{}}", ""),
        Err(Error::InvalidJson)
    );
    for mutation in [
        "{\"id\":\"secret\",\"id\":\"duplicate\"}".to_owned(),
        record.replace("\"sequence\": 1", "\"sequence\": 1.0000000000000001"),
        record.replace(WORLD, "civ:other"),
        "x".repeat(MAX_RECORD_BYTES + 1),
    ] {
        let input = input(&[record.clone(), mutation]);
        assert!(project(&input.to_string(), "no match").is_err());
    }
    assert_eq!(
        project(&input(&[record.clone(), record]).to_string(), ""),
        Err(Error::EventConflict)
    );
}

#[test]
fn result_slice_is_bounded_and_hidden_or_unprovided_content_cannot_affect_projection() {
    let records: Vec<String> = (0..22)
        .map(|index| {
            offer_event(
                &format!("event:{index}"),
                index,
                &format!("agent:{index}"),
                1,
            )
        })
        .collect();
    let view = project_records(&records, "");
    assert_eq!(view["offers"].as_array().unwrap().len(), 20);
    assert_eq!(view["offers"][0]["event_id"], "event:21");
    assert_eq!(view["report"]["truncated"], true);
    assert!(view["report"].get("global_count").is_none());
    let empty = project_records(&[], "private-external-offer");
    assert_eq!(empty["offers"], json!([]));
    assert!(!empty.to_string().contains("agent:hidden"));
    assert_eq!(empty["report"]["truncated"], false);
}

#[test]
fn long_complete_traversal_revalidates_cross_page_identity_and_counts() {
    let records: Vec<String> = (0..205)
        .map(|index| event(&format!("event:{index}"), index, &format!("agent:{index}")))
        .collect();
    let mut wrapper = input(&records);
    wrapper["report"]["pages"] = json!(3);
    wrapper["report"]["original_record_sha256"] = json!(["extra orchestration metadata"]);
    let view = project(&wrapper.to_string(), "").unwrap();
    assert_eq!(view["report"]["events"], 205);
    assert_eq!(view["offers"], json!([]));
    wrapper["report"]["pages"] = json!(2);
    assert_eq!(project(&wrapper.to_string(), ""), Err(Error::InvalidView));
    let mut conflict: Value = serde_json::from_str(&records[204]).unwrap();
    conflict["body"]["artifact_revision"]["from"] = json!("agent:0");
    conflict["actor"] = json!("agent:0");
    wrapper["snapshot"]["records"][204] = json!(conflict.to_string());
    wrapper["report"]["pages"] = json!(3);
    assert_eq!(
        project(&wrapper.to_string(), ""),
        Err(Error::RevisionConflict)
    );
}

#[test]
fn shared_large_evidence_cannot_expand_projection_beyond_output_bound() {
    let mut records = vec![event("event:source", 1, "agent:source")];
    for index in 0..20 {
        let mut offer: Value = serde_json::from_str(&offer_event(
            &format!("event:offer-{index}"),
            index + 2,
            &format!("agent:{index}"),
            1,
        ))
        .unwrap();
        offer["body"]["artifact_revision"]["body"]["activity_offer"]["source_refs"] = json!([{"event_id":"event:source","from":"agent:source","artifact_id":"artifact:same-name","revision":1}]);
        records.push(offer.to_string());
    }
    let fixture: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../examples/inheritance/history.json"
    ))
    .unwrap();
    for index in 0..60 {
        let mut objection = fixture[2].clone();
        objection["id"] = json!(format!("event:objection-{index}"));
        objection["world"] = json!(WORLD);
        objection["sequence"] = json!(index + 22);
        objection["body"]["objection"]["world"] = json!(WORLD);
        objection["body"]["objection"]["target_from"] = json!("agent:source");
        objection["body"]["objection"]["artifact_id"] = json!("artifact:same-name");
        objection["body"]["objection"]["body"]["text"] = json!("x".repeat(15000));
        let original = objection.to_string();
        assert!(original.len() < MAX_RECORD_BYTES);
        records.push(original);
    }
    assert_eq!(
        project(&input(&records).to_string(), ""),
        Err(Error::Output)
    );
}
