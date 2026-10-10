use super::*;
use crate::tests::{WORLD, event};

type Cite<'a> = Option<(&'a str, &'a str, u64)>;

fn revision(id: &str, sequence: u64, author: &str, name: &str, number: u64, cites: Cite) -> String {
    let mut value: Value = serde_json::from_str(&event(id, sequence, author)).unwrap();
    let record = &mut value["body"]["artifact_revision"];
    record["artifact_id"] = json!(name);
    record["revision"] = json!(number);
    if let Some((from, artifact_id, revision)) = cites {
        record["derived_from"] = json!({"from":from,"artifact_id":artifact_id,"revision":revision});
    }
    serde_json::to_string_pretty(&value).unwrap()
}

fn objection(id: &str, sequence: u64, target: (&str, &str, u64)) -> String {
    let fixture: Vec<Value> = serde_json::from_str(include_str!(
        "../../../../examples/inheritance/history.json"
    ))
    .unwrap();
    let mut value = fixture[2].clone();
    value["id"] = json!(id);
    value["world"] = json!(WORLD);
    value["sequence"] = json!(sequence);
    let body = &mut value["body"]["objection"];
    body["id"] = json!(format!("submission:{id}"));
    body["world"] = json!(WORLD);
    body["target_from"] = json!(target.0);
    body["artifact_id"] = json!(target.1);
    body["revision"] = json!(target.2);
    value.to_string()
}

fn input(records: &[String]) -> String {
    json!({"snapshot":{"world":WORLD,"records":records},"report":{"pages":records.len().div_ceil(100).max(1),
        "events":records.len(),"response_bytes":records.iter().map(String::len).sum::<usize>()+200,
        "reached_end":true,"scope":"current_caller_view","copying_permission":"not_granted"}})
    .to_string()
}

fn view(records: &[String]) -> Value {
    project(&input(records)).unwrap()
}

#[test]
fn transitive_dependents_follow_declared_citations_and_keep_the_authors_chain_separate() {
    let parent = revision("event:p1", 1, "agent:p", "artifact:parent", 1, None);
    let corrected = revision(
        "event:p2",
        2,
        "agent:p",
        "artifact:parent",
        2,
        Some(("agent:p", "artifact:parent", 1)),
    );
    let dispute = objection("event:o1", 3, ("agent:p", "artifact:parent", 1));
    let first = revision(
        "event:d1",
        4,
        "agent:d",
        "artifact:d",
        1,
        Some(("agent:p", "artifact:parent", 1)),
    );
    let second = revision(
        "event:e1",
        5,
        "agent:e",
        "artifact:e",
        1,
        Some(("agent:d", "artifact:d", 1)),
    );
    let current = revision(
        "event:c1",
        6,
        "agent:c",
        "artifact:c",
        1,
        Some(("agent:p", "artifact:parent", 2)),
    );
    let unrelated = revision("event:u1", 7, "agent:u", "artifact:u", 1, None);
    let records = vec![
        parent.clone(),
        corrected.clone(),
        dispute.clone(),
        first.clone(),
        second.clone(),
        current,
        unrelated,
    ];
    let result = view(&records);
    assert_eq!(result["format"], FORMAT);
    assert_eq!(result["unresolved_citations"], json!([]));
    let concerns = result["concerns"].as_array().unwrap();
    assert_eq!(concerns.len(), 1);
    let concern = &concerns[0];
    assert_eq!(
        concern["target"],
        json!({"from":"agent:p","artifact_id":"artifact:parent","revision":1})
    );
    assert_eq!(concern["target_status"], "available");
    assert_eq!(
        concern["reasons"],
        json!(["objected", "later_revision_by_author"])
    );
    assert_eq!(concern["target_original"]["record_utf8"], parent);
    assert_eq!(concern["objections"][0]["record_utf8"], dispute);
    assert_eq!(
        concern["later_revisions"],
        json!([{"event_id":"event:p2","record_utf8":corrected}])
    );
    let dependents = concern["dependents"].as_array().unwrap();
    assert_eq!(dependents.len(), 2);
    assert_eq!(
        (
            dependents[0]["event_id"].as_str(),
            dependents[0]["depth"].as_u64()
        ),
        (Some("event:d1"), Some(1))
    );
    assert_eq!(dependents[0]["record_utf8"], first);
    assert_eq!(
        (
            dependents[1]["event_id"].as_str(),
            dependents[1]["depth"].as_u64()
        ),
        (Some("event:e1"), Some(2))
    );
    assert_eq!(
        dependents[1]["cites"],
        json!({"from":"agent:d","artifact_id":"artifact:d","revision":1})
    );
    assert_eq!(dependents[1]["record_utf8"], second);
    assert!(!result.to_string().contains("event:c1"));
    assert!(!result.to_string().contains("event:u1"));
    assert_eq!(
        result["report"]["dependency"],
        "declared_derived_from_citations_only_never_inferred"
    );
    assert_eq!(result["report"]["truncated"], false);

    let without_objection: Vec<String> = records
        .iter()
        .filter(|record| **record != dispute)
        .cloned()
        .collect();
    let superseded = view(&without_objection);
    assert_eq!(
        superseded["concerns"][0]["reasons"],
        json!(["later_revision_by_author"])
    );
    assert_eq!(superseded["concerns"][0]["objections"], json!([]));
    assert_eq!(
        superseded["concerns"][0]["dependents"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let quiet: Vec<String> = without_objection
        .into_iter()
        .filter(|record| *record != first && *record != second)
        .collect();
    assert_eq!(view(&quiet)["concerns"], json!([]));
}

#[test]
fn unavailable_targets_and_citations_report_unknown_lineage_without_guessing() {
    let dispute = objection("event:o1", 1, ("agent:gone", "artifact:x", 1));
    let dependent = revision(
        "event:d1",
        2,
        "agent:d",
        "artifact:d",
        1,
        Some(("agent:gone", "artifact:x", 1)),
    );
    let orphan = revision(
        "event:f1",
        3,
        "agent:f",
        "artifact:f",
        1,
        Some(("agent:none", "artifact:y", 4)),
    );
    let result = view(&[dispute, dependent.clone(), orphan.clone()]);
    let concern = &result["concerns"][0];
    assert_eq!(concern["target_status"], "unavailable");
    assert!(concern.get("target_original").is_none());
    assert_eq!(concern["dependents"][0]["record_utf8"], dependent);
    let unresolved = result["unresolved_citations"].as_array().unwrap();
    assert_eq!(unresolved.len(), 2);
    assert!(unresolved.iter().all(|entry| entry["lineage"] == "unknown"));
    assert_eq!(unresolved[1]["record_utf8"], orphan);
    assert_eq!(
        unresolved[1]["cites"],
        json!({"from":"agent:none","artifact_id":"artifact:y","revision":4})
    );
    assert_eq!(
        result["report"]["unavailable_targets"],
        "absent_hidden_or_withdrawn_cannot_be_distinguished"
    );
    let mut tombstone: Value = serde_json::from_str(&event("event:t1", 4, "agent:gone")).unwrap();
    tombstone["kind"] = json!("artifact.withdrawn");
    tombstone["body"] = json!({});
    let with_tombstone = view(&[
        revision(
            "event:d2",
            1,
            "agent:d",
            "artifact:d",
            1,
            Some(("agent:gone", "artifact:same-name", 1)),
        ),
        tombstone.to_string(),
    ]);
    assert_eq!(with_tombstone["concerns"], json!([]));
    assert_eq!(
        with_tombstone["unresolved_citations"][0]["lineage"],
        "unknown"
    );
}

#[test]
fn copied_citation_cycles_terminate_and_list_each_dependent_once() {
    let first = revision(
        "event:a1",
        1,
        "agent:a",
        "artifact:a",
        1,
        Some(("agent:b", "artifact:b", 1)),
    );
    let second = revision(
        "event:b1",
        2,
        "agent:b",
        "artifact:b",
        1,
        Some(("agent:a", "artifact:a", 1)),
    );
    let dispute = objection("event:o1", 3, ("agent:a", "artifact:a", 1));
    let result = view(&[first, second, dispute]);
    let dependents = result["concerns"][0]["dependents"].as_array().unwrap();
    assert_eq!(dependents.len(), 1);
    assert_eq!(dependents[0]["event_id"], "event:b1");
}

#[test]
fn partial_or_malformed_input_uses_fixed_correction_diagnostics() {
    let records = [revision(
        "event:p1",
        1,
        "agent:p",
        "artifact:parent",
        1,
        None,
    )];
    let mut partial: Value = serde_json::from_str(&input(&records)).unwrap();
    partial["report"]["reached_end"] = json!(false);
    assert_eq!(
        project(&partial.to_string()),
        Err(Error::InvalidCorrectionView)
    );
    assert_eq!(
        Error::InvalidCorrectionView.code(),
        "invalid_correction_view_input"
    );
    let mut widened: Value = serde_json::from_str(&input(&records)).unwrap();
    widened["report"]["copying_permission"] = json!("granted");
    assert_eq!(
        project(&widened.to_string()),
        Err(Error::InvalidCorrectionView)
    );
    assert_eq!(
        project("{\"snapshot\":{},\"snapshot\":{}}"),
        Err(Error::InvalidJson)
    );
    assert_eq!(
        project(&"x".repeat(MAX_INPUT_BYTES + 1)),
        Err(Error::InputLimit)
    );
    let mut conflicting: Value = serde_json::from_str(&records[0]).unwrap();
    conflicting["id"] = json!("event:other");
    conflicting["sequence"] = json!(2);
    assert!(project(&input(&[records[0].clone(), conflicting.to_string()])).is_err());
}

#[test]
fn concerns_are_ordered_bounded_and_marked_truncated() {
    let mut records = Vec::new();
    for number in 0..51_u64 {
        let author = format!("agent:{number}");
        records.push(revision(
            &format!("event:r{number}"),
            number * 2 + 1,
            &author,
            "artifact:x",
            1,
            None,
        ));
        records.push(objection(
            &format!("event:o{number}"),
            number * 2 + 2,
            (&author, "artifact:x", 1),
        ));
    }
    let result = view(&records);
    assert_eq!(result["report"]["displayed"], 50);
    assert_eq!(result["report"]["truncated"], true);
    assert_eq!(result["concerns"][0]["target"]["from"], "agent:0");
    assert_eq!(result["concerns"][49]["target"]["from"], "agent:49");
}
