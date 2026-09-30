use std::fs;
use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_agentciv-archive"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn no_command_or_unreadable_input_emits_no_records() {
    for args in [
        vec![],
        vec!["unknown"],
        vec!["validate", "missing-archive-file"],
    ] {
        let result = cli(&args);
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert!(!result.stderr.is_empty());
    }
}

#[test]
fn invalid_utf8_and_overlarge_inputs_are_bounded_without_content_echo() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.json");
    fs::write(&path, [0xff, 0xfe]).unwrap();
    let result = cli(&["validate", path.to_str().unwrap()]);
    assert!(!result.status.success());
    assert_eq!(result.stderr, b"invalid_utf8\n");
    assert!(result.stdout.is_empty());
    fs::write(&path, vec![b'X'; agentciv_archive::MAX_INPUT_BYTES + 1]).unwrap();
    let result = cli(&["inspect", path.to_str().unwrap()]);
    assert!(!result.status.success());
    assert_eq!(result.stderr, b"input_too_large\n");
    assert!(result.stdout.is_empty());
}

#[test]
fn independent_file_consumer_can_validate_inspect_and_export() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = include_str!("../../../conformance/fixtures/valid/archive-bundle.json");
    let bundle: serde_json::Value = serde_json::from_str(fixture).unwrap();
    let path = dir.path().join("bundle.json");
    fs::write(&path, fixture).unwrap();
    for command in ["validate", "inspect"] {
        let result = cli(&[command, path.to_str().unwrap()]);
        assert!(result.status.success(), "{:?}", result.stderr);
        let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(report["copy_integrity"], "matched");
        assert_eq!(report["source_authenticity"], "unverified");
        assert_eq!(report["authority_transferred"], false);
    }
    let snapshot = dir.path().join("snapshot.json");
    let permit = dir.path().join("permit.json");
    fs::write(
        &snapshot,
        serde_json::to_vec(&serde_json::json!({"world":bundle["world"],
        "records":bundle["entries"].as_array().unwrap().iter()
            .map(|entry| entry["record_utf8"].clone()).collect::<Vec<_>>()}))
        .unwrap(),
    )
    .unwrap();
    fs::write(
        &permit,
        serde_json::to_vec(&serde_json::json!({"world":bundle["world"],
        "audience":bundle["audience"], "event_ids":bundle["entries"].as_array().unwrap().iter()
            .map(|entry| entry["event_id"].clone()).collect::<Vec<_>>()}))
        .unwrap(),
    )
    .unwrap();
    let result = cli(&[
        "export",
        snapshot.to_str().unwrap(),
        permit.to_str().unwrap(),
    ]);
    assert!(result.status.success(), "{:?}", result.stderr);
    let exported: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(exported, bundle);
}

#[test]
fn failed_export_never_outputs_partial_private_payloads() {
    let dir = tempfile::tempdir().unwrap();
    let snapshot = dir.path().join("snapshot.json");
    let permit = dir.path().join("permit.json");
    fs::write(
        &snapshot,
        r#"{"world":"world","records":["private secret invalid event"]}"#,
    )
    .unwrap();
    fs::write(
        &permit,
        r#"{"world":"world","audience":["recipient"],"event_ids":["absent"]}"#,
    )
    .unwrap();
    let result = cli(&[
        "export",
        snapshot.to_str().unwrap(),
        permit.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert_eq!(result.stderr, b"invalid_json\n");
}
