//! Frozen guide-arrival, hostile-peer, and handoff-note records.
//!
//! These checks compare source bytes and declared obligations. They do not
//! call a model, open a room, or treat a fixture as a grant.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::{CheckResult, read_json, schema_registry, validate_with_registry};

const USEFUL_PHRASE: &str = "A later source note replaces that support.";
const OBSOLETE: &str = "The earlier source note is still the current support.";
const CORRECTION: &str =
    "A later source note replaces that support. The objection remains unresolved.";
const PARENT: &str = "Retrieval of the permitted history succeeded.";
const TASK: &str = "State whether the parent artifact's source support matches the current source note, and keep retrieval separate.";

pub fn check_research_fixtures(root: &Path) -> CheckResult<Vec<String>> {
    let mut issues = Vec::new();
    issues.extend(check_guide_arrival(root)?);
    issues.extend(check_hostile_peer(root)?);
    issues.extend(check_handoff_notes(root)?);
    Ok(issues)
}

fn lf_normalized(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut index = 0;
    while index < raw.len() {
        if raw[index] == b'\r' && index + 1 < raw.len() && raw[index + 1] == b'\n' {
            out.push(b'\n');
            index += 2;
        } else {
            out.push(raw[index]);
            index += 1;
        }
    }
    out
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn relative_file(root: &Path, value: &Value) -> Option<PathBuf> {
    let text = value.as_str()?;
    if text.is_empty()
        || text.starts_with('/')
        || text.starts_with('\\')
        || text.contains("..")
        || text.contains('\\')
    {
        return None;
    }
    Some(root.join(text))
}

fn string_field<'a>(object: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    object.get(key).and_then(Value::as_str)
}

fn check_guide_arrival(root: &Path) -> CheckResult<Vec<String>> {
    let mut issues = Vec::new();
    let path = root.join("examples/participants/guide_arrival_trial.json");
    let manifest = read_json(&path)?;
    let Some(object) = manifest.as_object() else {
        return Ok(vec![format!(
            "{}: trial manifest must be an object",
            path.display()
        )]);
    };
    if string_field(object, "format") != Some("agentciv-guide-arrival-trial/0.1")
        || string_field(object, "status") != Some("frozen_before_observation")
        || string_field(object, "observation") != Some("unobserved")
        || object.get("attempts") != Some(&Value::from(1))
        || object.get("downloads") != Some(&Value::Bool(false))
        || object.get("external_spend_usd") != Some(&Value::from(0))
    {
        issues.push(format!("{}: trial identity drifted", path.display()));
    }
    let Some(budgets) = object.get("budgets").and_then(Value::as_object) else {
        issues.push(format!("{}: budgets missing", path.display()));
        return Ok(issues);
    };
    if budgets.get("steps") != Some(&Value::from(6))
        || budgets.get("gathering_prompt_bytes") != Some(&Value::from(6500))
        || budgets.get("say_characters") != Some(&Value::from(400))
        || budgets.get("tool_result_bytes") != Some(&Value::from(4096))
        || budgets.get("prompt_bytes") != Some(&Value::from(12000))
    {
        issues.push(format!("{}: trial budgets drifted", path.display()));
    }
    let prompt = object.get("prompt").and_then(Value::as_object);
    if prompt.and_then(|item| string_field(item, "representation"))
        != Some("card_plus_latest_tool_result")
    {
        issues.push(format!("{}: prompt representation drifted", path.display()));
    }
    let Some(card_path) = object
        .get("card_path")
        .and_then(|value| relative_file(root, value))
    else {
        issues.push(format!(
            "{}: card path is not a repository file",
            path.display()
        ));
        return Ok(issues);
    };
    let Some(guide_path) = object
        .get("full_guide")
        .and_then(|value| relative_file(root, value))
    else {
        issues.push(format!(
            "{}: guide path is not a repository file",
            path.display()
        ));
        return Ok(issues);
    };
    let card = lf_normalized(&fs::read(&card_path)?);
    let guide = lf_normalized(&fs::read(&guide_path)?);
    if card.len() >= 6500 || guide.len() <= 6500 {
        issues.push(format!(
            "{}: card must fit the gathering prompt and the operator guide must not",
            path.display()
        ));
    }
    check_digest(&mut issues, object.get("card"), &card, "card");
    check_digest(&mut issues, object.get("guide"), &guide, "guide");
    if object
        .get("guide")
        .and_then(Value::as_object)
        .and_then(|item| string_field(item, "newline"))
        != Some("lf")
    {
        issues.push(format!(
            "{}: guide digest must name the LF form",
            path.display()
        ));
    }
    Ok(issues)
}

fn check_digest(issues: &mut Vec<String>, value: Option<&Value>, bytes: &[u8], label: &str) {
    let Some(object) = value.and_then(Value::as_object) else {
        issues.push(format!("{label}: digest object missing"));
        return;
    };
    let length = object.get("canonical_bytes").and_then(Value::as_u64);
    let digest = sha256_hex(bytes);
    if length != Some(bytes.len() as u64) || string_field(object, "sha256") != Some(digest.as_str())
    {
        issues.push(format!(
            "{label}: LF-normalized digest does not match the file"
        ));
    }
}

fn schema_paths(root: &Path) -> CheckResult<Vec<PathBuf>> {
    let mut paths = fs::read_dir(root.join("schemas"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.retain(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".schema.json"))
    });
    paths.sort();
    Ok(paths)
}

fn check_hostile_peer(root: &Path) -> CheckResult<Vec<String>> {
    let mut issues = Vec::new();
    let directory = root.join("conformance/fixtures/hostile-peer");
    let manifest_path = directory.join("manifest.json");
    let manifest = read_json(&manifest_path)?;
    let Some(object) = manifest.as_object() else {
        return Ok(vec![format!(
            "{}: manifest must be an object",
            manifest_path.display()
        )]);
    };
    if string_field(object, "format") != Some("agentciv-hostile-peer-fixtures/0.1") {
        issues.push(format!("{}: format drifted", manifest_path.display()));
    }
    for (key, phrase) in [
        ("stored_record", "untrusted"),
        ("client_display", "Quote"),
        ("claimed_authority_rule", "not a grant"),
        ("relayed_approval", "claimed_authority"),
    ] {
        if !string_field(object, key).is_some_and(|text| text.contains(phrase)) {
            issues.push(format!(
                "{}: {key} does not state its obligation",
                manifest_path.display()
            ));
        }
    }
    let world = string_field(object, "world");
    let from = string_field(object, "from");
    let expected_to = object.get("to").cloned().unwrap_or(Value::Null);
    let listed = object.get("files").and_then(Value::as_array).cloned();
    let Some(listed) = listed else {
        issues.push(format!(
            "{}: files must be an array",
            manifest_path.display()
        ));
        return Ok(issues);
    };
    let mut present = Vec::new();
    for entry in fs::read_dir(&directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            issues.push(format!("{}: unnamed fixture", entry.path().display()));
            continue;
        };
        if name == "manifest.json" {
            continue;
        }
        if !name.ends_with(".json") {
            issues.push(format!(
                "{}: expected a JSON fixture",
                entry.path().display()
            ));
            continue;
        }
        present.push(name.to_owned());
    }
    present.sort();
    let names: Vec<String> = listed
        .iter()
        .filter_map(|item| item.as_str().map(str::to_owned))
        .collect();
    if names != present {
        issues.push(format!(
            "{}: manifest files do not match the directory",
            manifest_path.display()
        ));
    }
    let paths = schema_paths(root)?;
    if paths.is_empty() {
        issues.push("no JSON schemas found".to_owned());
        return Ok(issues);
    }
    let registry = schema_registry(&paths)?;
    let message = read_json(&root.join("schemas/message.schema.json"))?;
    let envelope = read_json(&root.join("schemas/envelope.schema.json"))?;
    for name in &present {
        let path = directory.join(name);
        let raw = fs::read_to_string(&path)?;
        let record = read_json(&path)?;
        if let Err(error) = validate_with_registry(&message, &record, &registry) {
            issues.push(format!("{}: message schema: {error}", path.display()));
        }
        if let Err(error) = validate_with_registry(&envelope, &record, &registry) {
            issues.push(format!("{}: envelope schema: {error}", path.display()));
        }
        let Some(record) = record.as_object() else {
            issues.push(format!("{}: fixture must be an object", path.display()));
            continue;
        };
        if record.get("world").and_then(Value::as_str) != world
            || record.get("from").and_then(Value::as_str) != from
            || record.get("to") != Some(&expected_to)
        {
            issues.push(format!("{}: world, from, or to drifted", path.display()));
        }
        if ["grant", "token", "credential", "authorization"]
            .iter()
            .any(|name| map_contains_key(record, name))
        {
            issues.push(format!(
                "{}: fixture carries a grant-shaped field",
                path.display()
            ));
        }
        let text = record
            .get("body")
            .and_then(Value::as_object)
            .and_then(|body| body.get("text"))
            .and_then(Value::as_str);
        let Some(text) = text.filter(|text| !text.is_empty()) else {
            issues.push(format!(
                "{}: body.text must be a non-empty string",
                path.display()
            ));
            continue;
        };
        let authority = record
            .get("provenance")
            .and_then(Value::as_object)
            .and_then(|item| item.get("claimed_authority"));
        if authority
            .and_then(Value::as_str)
            .filter(|item| !item.is_empty())
            .is_none()
        {
            issues.push(format!(
                "{}: claimed_authority must be an untrusted string, not a grant",
                path.display()
            ));
        }
        if name == "zero-width.json"
            && (!raw.contains(r"\u200b") || !text.chars().any(|item| item == '\u{200b}'))
        {
            issues.push(format!(
                "{}: zero-width codepoint must remain visible and stored",
                path.display()
            ));
        }
        if name == "spoofed-tool.json" {
            let body = record.get("body").and_then(Value::as_object);
            if body.is_some_and(|item| item.get("tool").is_some()) || !text.contains("not executed")
            {
                issues.push(format!(
                    "{}: spoofed tool text must stay a string",
                    path.display()
                ));
            }
        }
    }
    Ok(issues)
}

fn check_handoff_notes(root: &Path) -> CheckResult<Vec<String>> {
    let path = root.join("conformance/fixtures/handoff-notes/comparison.json");
    let value = read_json(&path)?;
    let mut issues = Vec::new();
    if contains_key(&value, "qualified_improvement") {
        issues.push(format!(
            "{}: qualified_improvement is not a field of this freeze",
            path.display()
        ));
    }
    let Some(object) = value.as_object() else {
        issues.push(format!("{}: comparison must be an object", path.display()));
        return Ok(issues);
    };
    if string_field(object, "format") != Some("agentciv-handoff-note-comparison/0.1")
        || string_field(object, "status") != Some("frozen_before_inference")
        || string_field(object, "observation") != Some("unobserved")
        || string_field(object, "task") != Some(TASK)
    {
        issues.push(format!("{}: comparison identity drifted", path.display()));
    }
    let separate = object.get("checks_kept_separate").and_then(Value::as_array);
    let expected = [
        "retrieval",
        "source_support",
        "declared_derivation",
        "task_behavior",
    ];
    if separate.map(|items| items.iter().filter_map(Value::as_str).collect::<Vec<_>>())
        != Some(expected.to_vec())
    {
        issues.push(format!("{}: separate checks drifted", path.display()));
    }
    let Some(originals) = object.get("originals").and_then(Value::as_object) else {
        issues.push(format!("{}: originals missing", path.display()));
        return Ok(issues);
    };
    if string_field(originals, "parent") != Some(PARENT)
        || string_field(originals, "obsolete") != Some(OBSOLETE)
        || string_field(originals, "correction") != Some(CORRECTION)
    {
        issues.push(format!("{}: originals drifted", path.display()));
    }
    let Some(conditions) = object.get("conditions").and_then(Value::as_object) else {
        issues.push(format!("{}: conditions missing", path.display()));
        return Ok(issues);
    };
    let mut keys: Vec<&str> = conditions.keys().map(String::as_str).collect();
    keys.sort_unstable();
    if keys != ["absent", "misleading", "stale", "useful"] {
        issues.push(format!(
            "{}: conditions must be absent, useful, stale, and misleading",
            path.display()
        ));
        return Ok(issues);
    }
    if !conditions.get("absent").is_some_and(Value::is_null) {
        issues.push(format!("{}: absent note must be null", path.display()));
    }
    let Some(useful) = note(conditions, "useful") else {
        issues.push(format!("{}: useful note is missing", path.display()));
        return Ok(issues);
    };
    let Some(stale) = note(conditions, "stale") else {
        issues.push(format!("{}: stale note is missing", path.display()));
        return Ok(issues);
    };
    let Some(misleading) = note(conditions, "misleading") else {
        issues.push(format!("{}: misleading note is missing", path.display()));
        return Ok(issues);
    };
    if string_field(useful, "aim") != Some(TASK)
        || string_field(stale, "aim") != Some(TASK)
        || string_field(misleading, "aim") != Some(TASK)
    {
        issues.push(format!(
            "{}: the four conditions must share one task",
            path.display()
        ));
    }
    if string_field(useful, "resume_hint") != Some(CORRECTION)
        || string_field(stale, "resume_hint") != Some(OBSOLETE)
    {
        issues.push(format!(
            "{}: useful and stale hints must match the originals",
            path.display()
        ));
    }
    if !string_field(misleading, "resume_hint")
        .is_some_and(|hint| hint.contains(USEFUL_PHRASE) && hint.contains(OBSOLETE))
    {
        issues.push(format!(
            "{}: misleading hint must reuse the useful phrase and the stale sentence",
            path.display()
        ));
    }
    for (name, item) in [
        ("useful", useful),
        ("stale", stale),
        ("misleading", misleading),
    ] {
        for key in ["aim", "resume_hint"] {
            let length = string_field(item, key)
                .map(|text| text.chars().count())
                .unwrap_or(0);
            if !(1..=1024).contains(&length) {
                issues.push(format!(
                    "{}: {name}.{key} must be 1 to 1024 characters",
                    path.display()
                ));
            }
        }
    }
    Ok(issues)
}

fn note<'a>(conditions: &'a Map<String, Value>, key: &str) -> Option<&'a Map<String, Value>> {
    conditions.get(key).and_then(Value::as_object)
}

fn map_contains_key(map: &Map<String, Value>, key: &str) -> bool {
    map.contains_key(key) || map.values().any(|item| contains_key(item, key))
}

fn contains_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(map) => map_contains_key(map, key),
        Value::Array(items) => items.iter().any(|item| contains_key(item, key)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn research_fixtures_match_the_frozen_records() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let issues = check_research_fixtures(&source).unwrap();
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn line_endings_normalize_only_crlf_pairs() {
        assert_eq!(lf_normalized(b"a\r\nb\n"), b"a\nb\n");
        assert_eq!(lf_normalized(b"a\rb"), b"a\rb");
    }

    #[test]
    fn qualified_improvement_key_is_visible_anywhere() {
        let value = serde_json::json!({"outer": [{"qualified_improvement": true}]});
        assert!(contains_key(&value, "qualified_improvement"));
        assert!(!contains_key(
            &serde_json::json!({"note": "qualified improvement"}),
            "qualified_improvement"
        ));
    }

    #[test]
    fn crlf_checkout_still_matches_the_lf_digest() {
        let staged = stage_repository();
        let card = staged
            .path()
            .join("examples/participants/guide_arrival_card.md");
        let mut bytes = Vec::new();
        for byte in lf_normalized(&fs::read(&card).unwrap()) {
            if byte == b'\n' {
                bytes.extend_from_slice(b"\r\n");
            } else {
                bytes.push(byte);
            }
        }
        fs::write(card, bytes).unwrap();
        let issues = check_research_fixtures(staged.path()).unwrap();
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn drifted_trial_card_and_path_are_reported() {
        let staged = stage_repository();
        let manifest = staged
            .path()
            .join("examples/participants/guide_arrival_trial.json");
        let mut value = load_value(&manifest);
        value["attempts"] = Value::from(2);
        store_value(&manifest, &value);
        assert_issue(staged.path(), "trial identity drifted");

        let staged = stage_repository();
        let card = staged
            .path()
            .join("examples/participants/guide_arrival_card.md");
        let mut bytes = fs::read(&card).unwrap();
        bytes.extend_from_slice(b"\nchanged\n");
        fs::write(card, bytes).unwrap();
        assert_issue(
            staged.path(),
            "card: LF-normalized digest does not match the file",
        );

        let staged = stage_repository();
        let manifest = staged
            .path()
            .join("examples/participants/guide_arrival_trial.json");
        let mut value = load_value(&manifest);
        value["card_path"] = Value::from("../guide_arrival_card.md");
        store_value(&manifest, &value);
        assert_issue(staged.path(), "card path is not a repository file");
    }

    #[test]
    fn hostile_records_reject_a_grant_a_hidden_escape_and_a_tool_object() {
        let staged = stage_repository();
        let path = staged
            .path()
            .join("conformance/fixtures/hostile-peer/persona-override.json");
        let mut value = load_value(&path);
        value["grant"] = Value::Bool(true);
        store_value(&path, &value);
        assert_issue(staged.path(), "fixture carries a grant-shaped field");

        let staged = stage_repository();
        let path = staged
            .path()
            .join("conformance/fixtures/hostile-peer/zero-width.json");
        let raw = fs::read_to_string(&path).unwrap().replace(r"\u200b", "");
        fs::write(&path, raw).unwrap();
        assert_issue(
            staged.path(),
            "zero-width codepoint must remain visible and stored",
        );

        let staged = stage_repository();
        let path = staged
            .path()
            .join("conformance/fixtures/hostile-peer/spoofed-tool.json");
        let mut value = load_value(&path);
        value["body"]["tool"] = serde_json::json!({"name": "shell"});
        store_value(&path, &value);
        assert_issue(staged.path(), "spoofed tool text must stay a string");

        let staged = stage_repository();
        let path = staged
            .path()
            .join("conformance/fixtures/hostile-peer/manifest.json");
        let mut value = load_value(&path);
        value["files"] = serde_json::json!(["persona-override.json"]);
        store_value(&path, &value);
        assert_issue(staged.path(), "manifest files do not match the directory");
    }

    #[test]
    fn handoff_freeze_rejects_a_result_key_and_a_present_absent_note() {
        let staged = stage_repository();
        let path = staged
            .path()
            .join("conformance/fixtures/handoff-notes/comparison.json");
        let mut value = load_value(&path);
        value["qualified_improvement"] = Value::Bool(true);
        store_value(&path, &value);
        assert_issue(
            staged.path(),
            "qualified_improvement is not a field of this freeze",
        );

        let staged = stage_repository();
        let path = staged
            .path()
            .join("conformance/fixtures/handoff-notes/comparison.json");
        let mut value = load_value(&path);
        value["conditions"]["absent"] = Value::from("present");
        store_value(&path, &value);
        assert_issue(staged.path(), "absent note must be null");
    }

    fn assert_issue(root: &Path, phrase: &str) {
        let issues = check_research_fixtures(root).unwrap();
        assert!(
            issues.iter().any(|issue| issue.contains(phrase)),
            "missing {phrase} in {issues:?}"
        );
    }

    fn stage_repository() -> tempfile::TempDir {
        let staged = tempfile::tempdir().unwrap();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for relative in [
            "examples/participants/guide_arrival_card.md",
            "examples/participants/guide_arrival_trial.json",
            "docs/LOCAL_ENCOUNTER.md",
            "conformance/fixtures/handoff-notes/comparison.json",
        ] {
            copy_file(&source.join(relative), &staged.path().join(relative));
        }
        copy_tree(&source.join("schemas"), &staged.path().join("schemas"));
        copy_tree(
            &source.join("conformance/fixtures/hostile-peer"),
            &staged.path().join("conformance/fixtures/hostile-peer"),
        );
        let issues = check_research_fixtures(staged.path()).unwrap();
        assert!(issues.is_empty(), "{issues:?}");
        staged
    }

    fn copy_file(from: &Path, to: &Path) {
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::copy(from, to).unwrap();
    }

    fn copy_tree(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), &target).unwrap();
            }
        }
    }

    fn load_value(path: &Path) -> Value {
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
    }

    fn store_value(path: &Path, value: &Value) {
        fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
    }
}
