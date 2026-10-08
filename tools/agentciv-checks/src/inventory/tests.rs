use super::*;
use serde_json::json;

fn fixture() -> (tempfile::TempDir, Value) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    for path in ["docs", "conformance", "checks"] {
        std::fs::create_dir(root.join(path)).unwrap();
    }
    std::fs::create_dir_all(root.join("conformance/runner/src")).unwrap();
    std::fs::write(
        root.join(SOURCES[0]),
        "# Profile\n\n## Scope\n\nA host MUST keep records.\n",
    )
    .unwrap();
    std::fs::write(
        root.join(SOURCES[1]),
        "# Extension\n\n## Where it sits\n\nA host MUST keep acts.\n",
    )
    .unwrap();
    std::fs::write(
        root.join("checks/tests.rs"),
        "#[test]\nfn durable_records() {}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("conformance/runner/src/lib.rs"),
        "const CASE: &str = \"events.recorded\";\n",
    )
    .unwrap();
    std::fs::write(
        root.join("checks/report.json"),
        r#"{"cases":[{"id":"events.recorded","status":"passed","required":true}]}"#,
    )
    .unwrap();
    let requirement = |id: &str, source: &str, quote: &str| {
        json!({
            "id":id,"source":source,"quote":quote,"obligation":"required",
            "applicability":"Every tested host","coverage":"partial_public",
            "references":["public","unit"],"observations":["observed"],"gaps":["gap"]
        })
    };
    let value = json!({
        "format":"agentciv-conformance-inventory/0.1","profile_complete":false,
        "requirements":[requirement("P",SOURCES[0],"A host MUST keep records."),requirement("C",SOURCES[1],"A host MUST keep acts.")],
        "references":[
            {"id":"public","kind":"public_case","path":"conformance/runner/src/lib.rs","anchor":"\"events.recorded\""},
            {"id":"unit","kind":"implementation_test","path":"checks/tests.rs","anchor":"fn durable_records("}
        ],
        "observations":[{"id":"observed","path":"checks/report.json","pointer":"/cases","case_id":"events.recorded","status":"passed","required":true}],
        "gaps":[{"id":"gap","owner":"Storage maintainers","proposed_test":"Interrupt before commit and reopen."}]
    });
    (directory, value)
}

fn run(root: &Path, value: &Value) -> Vec<String> {
    std::fs::write(root.join(INVENTORY), serde_json::to_vec(value).unwrap()).unwrap();
    check_inventory(root).unwrap()
}

#[test]
fn current_inventory_accounts_for_the_contract_and_retained_scopes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let issues = check_inventory(&root).unwrap();
    assert!(issues.is_empty(), "{issues:#?}");
}

#[test]
fn source_scanner_excludes_examples_and_out_of_scope_commentary() {
    let text = "# Title\n\nIntroduction MUST be excluded.\n\n## Scope\n\nA host MUST record.\n\n```json\n{\"MUST\":\"example\"}\n```\n\n1. First ordered check.\n2. Second ordered check.\n\n## Test setup and limits\n\nCommentary.\n\n## References\n\n- External.\n";
    assert_eq!(
        clauses(text, false),
        [
            "A host MUST record.",
            "1. First ordered check.",
            "2. Second ordered check."
        ]
    );
    assert_eq!(
        clauses(
            "## Where it sits\n\nFirst.\n\n### Revision\n\nSecond.\n",
            true
        ),
        ["First.", "Second."]
    );
}

#[test]
fn changed_missing_new_and_duplicate_source_clauses_fail() {
    let (directory, value) = fixture();
    assert!(run(directory.path(), &value).is_empty());
    let mut changed = value.clone();
    changed["requirements"][0]["quote"] = json!("An obsolete requirement.");
    assert!(
        run(directory.path(), &changed)
            .iter()
            .any(|issue| issue.contains("stale"))
    );
    let mut missing = value.clone();
    missing["requirements"].as_array_mut().unwrap().pop();
    assert!(
        run(directory.path(), &missing)
            .iter()
            .any(|issue| issue.contains("unaccounted"))
    );
    let mut duplicated = value.clone();
    duplicated["requirements"]
        .as_array_mut()
        .unwrap()
        .push(value["requirements"][0].clone());
    let issues = run(directory.path(), &duplicated);
    assert!(issues.iter().any(|issue| issue.contains("duplicate id")));
    assert!(
        issues
            .iter()
            .any(|issue| issue.contains("duplicate contract"))
    );
    std::fs::write(
        directory.path().join(SOURCES[0]),
        "## Scope\n\nA host MUST keep records.\n\nA new obligation.\n",
    )
    .unwrap();
    assert!(
        run(directory.path(), &value)
            .iter()
            .any(|issue| issue.contains("A new obligation"))
    );
}

#[test]
fn missing_unsafe_stale_and_non_test_references_fail() {
    let (directory, value) = fixture();
    for path in [
        "missing.rs",
        "../outside.rs",
        "checks\\tests.rs",
        "C:/outside.rs",
    ] {
        let mut bad = value.clone();
        bad["references"][0]["path"] = json!(path);
        assert!(
            run(directory.path(), &bad)
                .iter()
                .any(|issue| issue.contains("reference"))
        );
    }
    let mut stale = value.clone();
    stale["references"][0]["anchor"] = json!("events.obsolete");
    assert!(
        run(directory.path(), &stale)
            .iter()
            .any(|issue| issue.contains("stale reference"))
    );
    std::fs::write(
        directory.path().join("checks/tests.rs"),
        "fn durable_records() {}\n",
    )
    .unwrap();
    assert!(
        run(directory.path(), &value)
            .iter()
            .any(|issue| issue.contains("actual test"))
    );
}

#[test]
fn python_evidence_requires_a_named_test_in_a_test_file() {
    let (directory, mut value) = fixture();
    std::fs::write(
        directory.path().join("checks/test_records.py"),
        "    def test_records(self):\n        pass\n",
    )
    .unwrap();
    value["references"][1]["path"] = json!("checks/test_records.py");
    value["references"][1]["anchor"] = json!("def test_records(");
    assert!(run(directory.path(), &value).is_empty());
    std::fs::write(
        directory.path().join("checks/document.md"),
        "def test_records(\n",
    )
    .unwrap();
    value["references"][1]["path"] = json!("checks/document.md");
    assert!(
        run(directory.path(), &value)
            .iter()
            .any(|issue| issue.contains("actual test"))
    );
}

#[test]
fn evidence_categories_and_owned_gaps_are_checked() {
    let (directory, value) = fixture();
    for (field, replacement) in [
        ("references", json!([])),
        ("observations", json!(["absent"])),
        ("gaps", json!(["absent"])),
    ] {
        let mut bad = value.clone();
        bad["requirements"][0][field] = replacement;
        assert!(!run(directory.path(), &bad).is_empty());
    }
    for category in [
        "partial_public",
        "implementation_only",
        "gap",
        "informational",
    ] {
        let mut bad = value.clone();
        bad["requirements"][0]["coverage"] = json!(category);
        bad["requirements"][0]["references"] = json!([]);
        bad["requirements"][0]["gaps"] = json!([]);
        assert!(!run(directory.path(), &bad).is_empty());
    }
    let mut bad = value.clone();
    bad["gaps"][0]["owner"] = json!("");
    bad["requirements"][0]["applicability"] = json!(" ");
    bad["profile_complete"] = json!(true);
    bad["format"] = json!("other");
    assert_eq!(run(directory.path(), &bad).len(), 3);
}

#[test]
fn retained_failed_skipped_and_omitted_cases_cannot_be_relabelled_passed() {
    let (directory, value) = fixture();
    for (status, required) in [("failed", true), ("skipped", true), ("skipped", false)] {
        let report =
            json!({"cases":[{"id":"events.recorded","status":status,"required":required}]});
        std::fs::write(
            directory.path().join("checks/report.json"),
            report.to_string(),
        )
        .unwrap();
        assert!(!run(directory.path(), &value).is_empty());
        let mut honest = value.clone();
        honest["observations"][0]["status"] = json!(status);
        honest["observations"][0]["required"] = json!(required);
        assert!(run(directory.path(), &honest).is_empty());
    }
    std::fs::write(
        directory.path().join("checks/report.json"),
        "{\"cases\":[]}",
    )
    .unwrap();
    assert!(!run(directory.path(), &value).is_empty());
    let mut omitted = value;
    omitted["observations"][0]["status"] = json!("omitted");
    assert!(run(directory.path(), &omitted).is_empty());
}

#[test]
fn duplicate_or_misidentified_retained_cases_and_invalid_pointers_fail() {
    let (directory, mut value) = fixture();
    let case = json!({"id":"events.recorded","status":"passed","required":true});
    std::fs::write(
        directory.path().join("checks/report.json"),
        json!({"cases":[case.clone(),case]}).to_string(),
    )
    .unwrap();
    assert!(!run(directory.path(), &value).is_empty());
    value["observations"][0]["case_id"] = json!("another.case");
    assert!(!run(directory.path(), &value).is_empty());
    value["observations"][0]["pointer"] = json!("/missing");
    std::fs::write(directory.path().join(INVENTORY), value.to_string()).unwrap();
    assert!(
        check_inventory(directory.path())
            .unwrap_err()
            .to_string()
            .contains("cases array")
    );
}

#[test]
fn malformed_unknown_and_oversized_inputs_fail_closed() {
    let (directory, mut value) = fixture();
    value["unrecognized"] = json!(true);
    std::fs::write(directory.path().join(INVENTORY), value.to_string()).unwrap();
    assert!(check_inventory(directory.path()).is_err());
    std::fs::write(directory.path().join(INVENTORY), "{").unwrap();
    assert!(check_inventory(directory.path()).is_err());
    std::fs::write(
        directory.path().join(INVENTORY),
        " ".repeat(MAX_INVENTORY_BYTES as usize + 1),
    )
    .unwrap();
    assert!(
        check_inventory(directory.path())
            .unwrap_err()
            .to_string()
            .contains("byte limit")
    );
    assert!(bounded_text(directory.path(), "", 10).is_err());
    assert!(bounded_text(directory.path(), "checks/tests.rs", 1).is_err());
}
