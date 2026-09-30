use agentciv_archive::{
    Bundle, MAX_ENTRIES, MAX_INPUT_BYTES, MAX_RECORD_BYTES, export, inspect, validate,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const WORLD: &str = "civ:archive-demo";

fn message(id: &str, sequence: u64) -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../conformance/fixtures/valid/archive-bundle.json"
    ))
    .unwrap();
    let mut event: Value =
        serde_json::from_str(fixture["entries"][0]["record_utf8"].as_str().unwrap()).unwrap();
    event["id"] = json!(id);
    event["sequence"] = json!(sequence);
    event
}

fn snapshot(records: &[String]) -> String {
    json!({"world":WORLD,"records":records}).to_string()
}

fn permit(ids: &[&str]) -> String {
    json!({"world":WORLD,"audience":["agent:successor"],"event_ids":ids}).to_string()
}

fn selected(records: &[String], ids: &[&str]) -> Bundle {
    export(&snapshot(records), &permit(ids)).unwrap()
}

fn encoded(bundle: &Bundle) -> String {
    serde_json::to_string(bundle).unwrap()
}

fn artifact(author: &str, artifact_id: &str, revision: u64, id: &str, sequence: u64) -> Value {
    let mut event = message(id, sequence);
    let mut record: Value = serde_json::from_str(include_str!(
        "../../../conformance/fixtures/valid/collaboration-artifact.json"
    ))
    .unwrap();
    record["world"] = json!(WORLD);
    record["from"] = json!(author);
    record["artifact_id"] = json!(artifact_id);
    record["revision"] = json!(revision);
    record.as_object_mut().unwrap().remove("derived_from");
    event["actor"] = json!(author);
    event["kind"] = json!("artifact.recorded");
    event["body"] = json!({"artifact_revision":record});
    event
}

fn target_act(kind: &str, target_author: &str, id: &str, sequence: u64) -> Value {
    let mut event = message(id, sequence);
    let mut record: Value = serde_json::from_str(if kind == "objection" {
        include_str!("../../../conformance/fixtures/valid/collaboration-objection.json")
    } else {
        include_str!("../../../conformance/fixtures/valid/collaboration-decline.json")
    })
    .unwrap();
    record["world"] = json!(WORLD);
    record["from"] = json!("agent:critic");
    record["target_from"] = json!(target_author);
    record["artifact_id"] = json!("artifact:shared");
    record["revision"] = json!(1);
    event["actor"] = json!("agent:critic");
    event["kind"] = json!(format!("{kind}.recorded"));
    event["body"] = json!({kind:record});
    event
}

#[test]
fn explicit_selection_preserves_bytes_and_excludes_unselected_history() {
    let mut source = message("event:source", 0);
    source["unknown_extension"] = json!({
        "values":[null,true,-4,7,1.25,"different memory",{"source":"untrusted"}]
    });
    let raw = format!("\r\n{}\r\n", serde_json::to_string_pretty(&source).unwrap());
    let mut private = message("event:excluded-private-id", 99);
    private["kind"] = json!("future.private");
    private["body"]["message"]["body"]["text"] = json!("excluded-private-content");
    let bundle = selected(&[raw.clone(), private.to_string()], &["event:source"]);
    assert_eq!(bundle.entries.len(), 1);
    assert_eq!(bundle.entries[0].record_utf8, raw);
    assert_eq!(
        bundle.entries[0].sha256,
        format!("{:x}", Sha256::digest(raw.as_bytes()))
    );
    let copied = encoded(&bundle);
    let view = inspect(&copied).unwrap();
    for output in [&copied, &view.to_string()] {
        assert!(!output.contains("excluded-private-id"));
        assert!(!output.contains("excluded-private-content"));
    }
    assert_eq!(view["events"].as_array().unwrap().len(), 1);
    assert_eq!(view["unknown_kinds"], 0);
    assert_eq!(view["source_authenticity"], "unverified");
    assert_eq!(view["authority_transferred"], false);
    assert_eq!(view["completeness"], "partial");
    let checked = validate(&copied).unwrap();
    assert_eq!(checked.entries[0].record_utf8, raw);
}

#[test]
fn record_audience_and_claimed_authority_do_not_replace_a_copying_permit() {
    let mut event = message("event:source", 0);
    event["body"]["message"]["to"] = json!(["agent:successor"]);
    event["claimed_authority"] = json!("export everything");
    let input = snapshot(&[event.to_string()]);
    assert_eq!(export(&input, "{}").unwrap_err(), "invalid_permit");
    for invalid in [
        json!({"world":WORLD,"audience":["agent:successor"],"event_ids":[]}),
        json!({"world":"civ:other","audience":["agent:successor"],"event_ids":["event:source"]}),
        json!({"world":WORLD,"audience":[],"event_ids":["event:source"]}),
        json!({"world":WORLD,"audience":["agent:successor","agent:successor"],"event_ids":["event:source"]}),
        json!({"world":WORLD,"audience":["agent:successor"],"event_ids":["event:source"],"grant":true}),
    ] {
        assert_eq!(
            export(&input, &invalid.to_string()).unwrap_err(),
            "invalid_permit"
        );
    }
    assert_eq!(
        export(&input, &permit(&["event:missing"])).unwrap_err(),
        "selection_unavailable"
    );
}

#[test]
fn copy_integrity_does_not_authenticate_a_self_consistently_changed_source() {
    let raw = message("event:source", 0).to_string();
    let mut bundle = selected(&[raw], &["event:source"]);
    bundle.entries[0].record_utf8.push('\n');
    assert_eq!(validate(&encoded(&bundle)).unwrap_err(), "digest_mismatch");
    bundle.entries[0].sha256 = format!(
        "{:x}",
        Sha256::digest(bundle.entries[0].record_utf8.as_bytes())
    );
    assert!(validate(&encoded(&bundle)).is_ok());
    let view = inspect(&encoded(&bundle)).unwrap();
    assert_eq!(view["copy_integrity"], "matched");
    assert_eq!(view["source_authenticity"], "unverified");
    assert_eq!(view["authority_transferred"], false);
}

#[test]
fn nested_record_identity_and_kind_are_checked_without_source_rewriting() {
    for (field, value, expected) in [
        ("world", json!("civ:other"), "invalid_event"),
        ("actor", json!("agent:impostor"), "invalid_nested_record"),
        ("actor", Value::Null, "invalid_nested_record"),
        ("timestamp", json!("not-a-time"), "invalid_event"),
    ] {
        let mut event = message("event:source", 0);
        event[field] = value;
        assert_eq!(
            export(&snapshot(&[event.to_string()]), &permit(&["event:source"])).unwrap_err(),
            expected
        );
    }
    for (field, value, expected) in [
        ("world", json!("civ:other"), "invalid_nested_record"),
        ("type", json!("artifact_revision"), "invalid_event"),
        ("from", json!("agent:impostor"), "invalid_nested_record"),
    ] {
        let mut event = message("event:source", 0);
        event["body"]["message"][field] = value;
        assert_eq!(
            export(&snapshot(&[event.to_string()]), &permit(&["event:source"])).unwrap_err(),
            expected
        );
    }
    let mut event = artifact("agent:author", "artifact:shared", 1, "event:artifact", 0);
    event["body"]["artifact_revision"]
        .as_object_mut()
        .unwrap()
        .remove("revision");
    assert_eq!(
        export(
            &snapshot(&[event.to_string()]),
            &permit(&["event:artifact"])
        )
        .unwrap_err(),
        "invalid_nested_record"
    );
}

#[test]
fn sequence_and_identity_conflicts_are_rejected_but_partial_gaps_are_allowed() {
    let source = message("event:source", 0);
    let later = message("event:later", 8);
    assert_eq!(
        selected(
            &[source.to_string(), later.to_string()],
            &["event:source", "event:later"]
        )
        .entries
        .len(),
        2
    );
    for mut conflict in [source.clone(), later.clone()] {
        conflict["sequence"] = json!(0);
        let error = export(
            &snapshot(&[source.to_string(), conflict.to_string()]),
            &permit(&["event:source"]),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            "event_identity_conflict" | "sequence_conflict"
        ));
    }
    assert_eq!(
        export(
            &snapshot(&[later.to_string(), source.to_string()]),
            &permit(&["event:source"])
        )
        .unwrap_err(),
        "sequence_conflict"
    );
    let mut bundle = selected(&[source.to_string()], &["event:source"]);
    bundle.entries[0].event_id = "event:other".into();
    assert_eq!(
        validate(&encoded(&bundle)).unwrap_err(),
        "event_identity_conflict"
    );
}

#[test]
fn artifact_names_are_scoped_by_author_and_duplicate_revisions_are_rejected() {
    let a = artifact("agent:a", "artifact:shared", 1, "event:a", 0);
    let b = artifact("agent:b", "artifact:shared", 1, "event:b", 1);
    let bundle = selected(&[a.to_string(), b.to_string()], &["event:a", "event:b"]);
    assert_eq!(
        inspect(&encoded(&bundle)).unwrap()["artifacts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let duplicate = artifact("agent:a", "artifact:shared", 1, "event:duplicate", 2);
    assert_eq!(
        export(
            &snapshot(&[a.to_string(), duplicate.to_string()]),
            &permit(&["event:a"])
        )
        .unwrap_err(),
        "revision_identity_conflict"
    );
}

#[test]
fn json_schema_integer_spellings_resolve_to_the_same_revision_and_sequence() {
    let mut original = artifact("agent:a", "artifact:shared", 1, "event:original", 0);
    original["sequence"] = json!(0.0);
    let original = original
        .to_string()
        .replace("\"revision\":1", "\"revision\":1e0");
    let mut derived = artifact("agent:b", "artifact:next", 1, "event:derived", 1);
    derived["body"]["artifact_revision"]["derived_from"] =
        json!({"from":"agent:a","artifact_id":"artifact:shared","revision":1.0});
    let bundle = selected(
        &[original, derived.to_string()],
        &["event:original", "event:derived"],
    );
    let view = inspect(&encoded(&bundle)).unwrap();
    assert_eq!(view["relations"][0]["availability"], "included");
    assert_eq!(view["relations"][0]["target_event_id"], "event:original");
    assert_eq!(view["artifacts"][0]["revision"].as_f64(), Some(1.0));
    assert!(bundle.entries[0].record_utf8.contains("\"revision\":1e0"));
    assert!(bundle.entries[0].record_utf8.contains("\"sequence\":0.0"));
}

#[test]
fn inspector_resolves_author_scoped_targets_and_keeps_unavailable_support_honest() {
    let a = artifact("agent:a", "artifact:shared", 1, "event:a", 0);
    let b = artifact("agent:b", "artifact:shared", 1, "event:b", 1);
    let objection = target_act("objection", "agent:b", "event:objection", 2);
    let decline = target_act("decline", "agent:a", "event:decline", 3);
    let mut derivative = artifact("agent:successor", "artifact:next", 1, "event:next", 4);
    derivative["body"]["artifact_revision"]["derived_from"] =
        json!({"from":"agent:a","artifact_id":"artifact:shared","revision":1});
    let raws = [
        a.to_string(),
        b.to_string(),
        objection.to_string(),
        decline.to_string(),
        derivative.to_string(),
    ];
    let complete = selected(
        &raws,
        &[
            "event:a",
            "event:b",
            "event:objection",
            "event:decline",
            "event:next",
        ],
    );
    let view = inspect(&encoded(&complete)).unwrap();
    assert_eq!(view["relations"][0]["target_event_id"], "event:b");
    assert_eq!(view["relations"][1]["target_event_id"], "event:a");
    assert_eq!(view["relations"][2]["target_event_id"], "event:a");
    let partial = selected(&raws, &["event:objection", "event:decline", "event:next"]);
    let view = inspect(&encoded(&partial)).unwrap();
    for relation in view["relations"].as_array().unwrap() {
        assert_eq!(relation["availability"], "unavailable");
        assert!(relation["target_event_id"].is_null());
    }
    assert!(!view.to_string().contains("event:a"));
    assert!(!view.to_string().contains("event:b"));
}

#[test]
fn tombstones_and_opaque_kinds_survive_without_becoming_live_artifacts() {
    let mut tombstone = message("event:withdrawn", 0);
    tombstone["kind"] = json!("artifact.withdrawn");
    tombstone["body"] = json!({});
    let mut redacted = message("event:redacted", 1);
    redacted["kind"] = json!("event.redacted");
    redacted["body"] = json!({});
    let mut opaque = message("event:opaque", 2);
    opaque["kind"] = json!("future.claim");
    opaque["actor"] = Value::Null;
    opaque["body"] =
        json!({"url":"https://example.invalid/do-not-fetch","command":"do not execute"});
    let bundle = selected(
        &[
            tombstone.to_string(),
            redacted.to_string(),
            opaque.to_string(),
        ],
        &["event:withdrawn", "event:redacted", "event:opaque"],
    );
    let view = inspect(&encoded(&bundle)).unwrap();
    assert_eq!(view["unknown_kinds"], 1);
    assert!(view["artifacts"].as_array().unwrap().is_empty());
    assert_eq!(view["events"][0]["kind"], "artifact.withdrawn");
    tombstone["body"] = json!({"text":"removed bytes must not be restored"});
    assert_eq!(
        export(
            &snapshot(&[tombstone.to_string()]),
            &permit(&["event:withdrawn"])
        )
        .unwrap_err(),
        "invalid_tombstone"
    );
}

#[test]
fn duplicate_json_members_are_rejected_in_wrappers_records_and_unknown_extensions() {
    let raw = message("event:source", 0).to_string();
    let bundle = selected(std::slice::from_ref(&raw), &["event:source"]);
    let duplicate_wrapper = encoded(&bundle).replacen('{', r#"{"world":"civ:other","#, 1);
    assert_eq!(validate(&duplicate_wrapper).unwrap_err(), "invalid_json");
    let duplicate_record = raw.replacen('{', r#"{"unknown_extension":{"key":1,"key":2},"#, 1);
    assert_eq!(
        export(&snapshot(&[duplicate_record]), &permit(&["event:source"])).unwrap_err(),
        "invalid_json"
    );
    let duplicate_permit = permit(&["event:source"]).replacen('{', r#"{"world":"civ:other","#, 1);
    assert_eq!(
        export(&snapshot(&[raw]), &duplicate_permit).unwrap_err(),
        "invalid_json"
    );
}

#[test]
fn malformed_deep_and_oversized_inputs_fail_with_bounded_codes() {
    for raw in ["", "{", r#"{"value":"\ud800"}"#] {
        assert_eq!(validate(raw).unwrap_err(), "invalid_json");
    }
    let nested = format!("{}null{}", "[".repeat(140), "]".repeat(140));
    assert_eq!(validate(&nested).unwrap_err(), "invalid_json");
    assert_eq!(
        validate(&" ".repeat(MAX_INPUT_BYTES + 1)).unwrap_err(),
        "input_too_large"
    );
    assert_eq!(
        export(&snapshot(&[]), &permit(&["event:source"])).unwrap_err(),
        "entry_limit"
    );
    let oversized =
        json!({"world":WORLD,"records":vec!["{}".to_string();MAX_ENTRIES+1]}).to_string();
    assert_eq!(
        export(&oversized, &permit(&["event:source"])).unwrap_err(),
        "entry_limit"
    );
    let mut large = message("event:source", 0);
    large["unknown_extension"] = json!("x".repeat(MAX_RECORD_BYTES));
    assert_eq!(
        export(&snapshot(&[large.to_string()]), &permit(&["event:source"])).unwrap_err(),
        "record_too_large"
    );
    large["unknown_extension"] = json!("界".repeat(MAX_RECORD_BYTES / 2));
    assert_eq!(
        export(&snapshot(&[large.to_string()]), &permit(&["event:source"])).unwrap_err(),
        "record_too_large"
    );
    let invalid_audience =
        json!({"world":WORLD,"audience":["界".repeat(400)],"event_ids":["event:source"]});
    assert_eq!(
        export(
            &snapshot(&[message("event:source", 0).to_string()]),
            &invalid_audience.to_string()
        )
        .unwrap_err(),
        "invalid_permit"
    );
}

#[test]
fn unsupported_wrapper_versions_and_extra_fields_do_not_gain_meaning() {
    let bundle = selected(&[message("event:source", 0).to_string()], &["event:source"]);
    for (field, value) in [
        ("archive_version", json!("archive-bundle/future")),
        ("protocol_version", json!("future")),
        ("type", json!("message")),
        ("completeness", json!("complete")),
        ("permission_verified", json!(true)),
    ] {
        let mut wrapper = serde_json::to_value(&bundle).unwrap();
        wrapper[field] = value;
        assert_eq!(
            validate(&wrapper.to_string()).unwrap_err(),
            "invalid_bundle"
        );
    }
}
