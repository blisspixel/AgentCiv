use std::fs;
use std::process::Command;

use agentciv_directory::{build, parse, render, run};
use serde_json::{Value, json};

fn catalog() -> Value {
    json!({
        "schema_version": 1, "updated": "2026-10-03",
        "entries": [{
            "id": "example", "name": "Example", "kind": "commons", "availability": "local",
            "summary": "A local world", "interfaces": ["http"],
            "source_url": "https://example.org/source", "guide_url": "https://example.org/guide",
            "notes": "No public endpoint is claimed."
        }]
    })
}

fn parse_value(value: &Value) -> Result<agentciv_directory::Directory, String> {
    parse(&serde_json::to_vec(value).unwrap())
}

#[test]
fn rejects_invalid_shapes_and_unbounded_inputs() {
    for bytes in [b"not JSON".as_slice(), &[b' '; 65_537]] {
        assert!(parse(bytes).is_err());
    }
    for (key, value) in [
        ("schema_version", json!(2)),
        ("updated", json!("yesterday")),
        ("entries", json!({})),
        ("unknown", json!(true)),
    ] {
        let mut data = catalog();
        data[key] = value;
        assert!(parse_value(&data).is_err(), "{key}");
    }
    for (key, value) in [
        ("id", json!("<script>")),
        ("kind", json!("other")),
        ("availability", json!("live")),
        ("interfaces", json!(["http", "http"])),
        ("summary", json!(" ")),
        ("notes", json!("unsafe\u{0000}")),
        ("name", json!("")),
        ("unknown", json!(true)),
    ] {
        let mut data = catalog();
        data["entries"][0][key] = value;
        assert!(parse_value(&data).is_err(), "{key}");
    }
    let mut duplicate = catalog();
    let entry = duplicate["entries"][0].clone();
    duplicate["entries"].as_array_mut().unwrap().push(entry);
    assert!(parse_value(&duplicate).unwrap_err().contains("duplicate"));
}

#[test]
fn calendar_validation_includes_leap_year_and_month_boundaries() {
    for (date, valid) in [
        ("2024-02-29", true),
        ("2000-02-29", true),
        ("2026-12-31", true),
        ("1900-02-29", false),
        ("2026-02-29", false),
        ("2026-04-31", false),
        ("0000-01-01", false),
        ("2026-00-01", false),
        ("2026-13-01", false),
        ("2026-01-00", false),
    ] {
        let mut data = catalog();
        data["updated"] = json!(date);
        assert_eq!(parse_value(&data).is_ok(), valid, "{date}");
    }
}

#[test]
fn rejects_urls_that_could_leak_credentials_or_activate_content() {
    for url in [
        "javascript:alert(1)",
        "http://example.org",
        "https://name:secret@example.org",
        "https://name@example.org",
        "https://",
        "https://example.org/a b",
        "https://example.org/\u{0000}",
    ] {
        let mut data = catalog();
        data["entries"][0]["source_url"] = json!(url);
        assert!(parse_value(&data).is_err(), "{url}");
    }
}

#[test]
fn local_and_public_access_claims_stay_distinct() {
    let mut data = catalog();
    data["entries"][0]["join_url"] = json!("https://example.org/join");
    assert!(parse_value(&data).unwrap_err().contains("local examples"));
    data["entries"][0]["availability"] = json!("public");
    data["entries"][0]["discovery_url"] = json!("https://example.org/.well-known/agentciv");
    let html = render(&parse_value(&data).unwrap());
    assert!(html.contains("1 public entry points"));
    assert!(html.contains("operator declarations"));
    assert!(html.contains("Joining and access"));
    assert!(html.contains("World descriptor"));
    data["entries"][0]["kind"] = json!("game");
    assert!(parse_value(&data).unwrap_err().contains("only commons"));
    data["entries"][0]
        .as_object_mut()
        .unwrap()
        .remove("discovery_url");
    assert!(parse_value(&data).is_ok());
    data["entries"][0]
        .as_object_mut()
        .unwrap()
        .remove("join_url");
    assert!(parse_value(&data).unwrap_err().contains("join URL"));
}

#[test]
fn untrusted_content_is_escaped_without_replacing_template_markers() {
    let mut data = catalog();
    data["entries"][0]["name"] = json!("<script>\"&' {{PUBLIC_COUNT}}");
    data["entries"][0]["guide_url"] = json!("https://example.org/?a=1&b=2");
    data["entries"][0]["interfaces"] = json!(["http", "mcp", "cli"]);
    let html = render(&parse_value(&data).unwrap());
    assert!(html.contains("&lt;script&gt;&quot;&amp;&#39; {{PUBLIC_COUNT}}"));
    assert!(html.contains("?a=1&amp;b=2"));
    assert!(html.contains("HTTP / MCP / CLI"));
    assert!(!html.contains("<script>"));
    assert!(html.contains("No public hosts are listed yet"));
}

#[test]
fn empty_catalog_and_all_kinds_render_without_fabricated_hosts() {
    let mut data = catalog();
    for kind in ["commons", "creative", "game"] {
        data["entries"][0]["kind"] = json!(kind);
        assert!(render(&parse_value(&data).unwrap()).contains("Run locally"));
    }
    data["entries"] = json!([]);
    assert!(render(&parse_value(&data).unwrap()).contains("No worlds have been listed"));
}

#[test]
fn build_writes_only_public_assets_and_reports_filesystem_failures() {
    let temporary = tempfile::tempdir().unwrap();
    let output = temporary.path().join("site");
    let data = parse_value(&catalog()).unwrap();
    build(&data, &output).unwrap();
    assert_eq!(fs::read_dir(&output).unwrap().count(), 18);
    assert!(
        agentciv_directory::parse_resources(&fs::read(output.join("resources.json")).unwrap())
            .is_ok()
    );
    assert!(parse(&fs::read(output.join("directory.json")).unwrap()).is_ok());
    assert!(
        fs::read_to_string(output.join("index.html"))
            .unwrap()
            .contains("Example")
    );
    let occupied = temporary.path().join("occupied");
    fs::write(&occupied, "file").unwrap();
    assert!(build(&data, &occupied).is_err());
    fs::create_dir(output.join("index.html").with_extension("html.tmp")).unwrap();
    fs::remove_file(output.join("index.html")).unwrap();
    fs::create_dir(output.join("index.html")).unwrap();
    assert!(build(&data, &output).is_err());
}

#[test]
fn cli_rejects_bad_options_missing_inputs_and_invalid_catalogs() {
    for args in [
        vec![],
        vec!["other"],
        vec!["build"],
        vec!["check", "--input"],
        vec!["check", "--input", ""],
        vec!["check", "--unknown", "value"],
        vec!["check", "--output", "site"],
        vec!["check", "--input", "a", "--input", "b"],
        vec!["check", "--input", "missing-catalog.json"],
    ] {
        assert!(run(&args.into_iter().map(str::to_owned).collect::<Vec<_>>()).is_err());
    }
    let temporary = tempfile::tempdir().unwrap();
    let input = temporary.path().join("input.json");
    fs::write(&input, "{}").unwrap();
    let args = vec![
        "check".into(),
        "--input".into(),
        input.display().to_string(),
    ];
    assert!(run(&args).is_err());
    fs::write(&input, serde_json::to_vec(&catalog()).unwrap()).unwrap();
    assert!(run(&args).unwrap().contains("1 directory listings"));
    let mut build_args = args;
    build_args[0] = "build".into();
    build_args.extend([
        "--output".into(),
        temporary.path().join("output").display().to_string(),
    ]);
    assert!(run(&build_args).is_ok());
    assert!(
        run(&["check".into()])
            .unwrap()
            .contains("3 directory listings")
    );
}

fn guide() -> Value {
    json!({
        "id": "bearings",
        "title": "Bearings",
        "reviewed": "2026-10-03",
        "summary": "A short guide.",
        "prerequisites": ["Read this file."],
        "sources": [{"title": "Source", "url": "https://example.org/source"}],
        "copying": "MIT for this repository text.",
        "limitations": "Not a consciousness test.",
        "alternatives": ["other"],
        "corrections": "https://example.org/corrections",
        "url": "https://example.org/guide"
    })
}

fn library() -> Value {
    let mut other = guide();
    other["id"] = json!("other");
    other["alternatives"] = json!(["bearings"]);
    json!({
        "schema_version": 1,
        "updated": "2026-10-03",
        "purpose": "Optional bearings.",
        "guides": [guide(), other]
    })
}

#[test]
fn orientation_catalog_rejects_a_bad_date_a_self_alternative_and_private_fields() {
    assert!(agentciv_directory::parse_resources(b"not JSON").is_err());
    assert!(agentciv_directory::parse_resources(&[b' '; 65_537]).is_err());
    assert!(
        agentciv_directory::parse_resources(
            &fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../website/resources.json"
            ))
            .unwrap()
        )
        .is_ok()
    );
    for (key, value) in [
        ("schema_version", json!(2)),
        ("updated", json!("2026-02-29")),
        ("purpose", json!(" ")),
        ("consciousness", json!("required")),
    ] {
        let mut data = library();
        data[key] = value;
        assert!(
            agentciv_directory::parse_resources(&serde_json::to_vec(&data).unwrap()).is_err(),
            "{key}"
        );
    }
    let mut data = library();
    data["guides"][0]["reviewed"] = json!("2026-04-31");
    assert!(agentciv_directory::parse_resources(&serde_json::to_vec(&data).unwrap()).is_err());
    data["guides"][0]["reviewed"] = json!("2026-10-03");
    data["guides"][0]["alternatives"] = json!(["bearings"]);
    assert!(
        agentciv_directory::parse_resources(&serde_json::to_vec(&data).unwrap())
            .unwrap_err()
            .contains("another guide")
    );
    data["guides"][0]["alternatives"] = json!(["missing"]);
    assert!(agentciv_directory::parse_resources(&serde_json::to_vec(&data).unwrap()).is_err());
    data["guides"][0]["alternatives"] = json!(["other"]);
    data["guides"][0]["summary"] = json!("unsafe\u{0000}");
    assert!(agentciv_directory::parse_resources(&serde_json::to_vec(&data).unwrap()).is_err());
    data["guides"][0]["summary"] = json!("A short guide.");
    data["guides"][0]["url"] = json!("http://example.org/guide");
    assert!(agentciv_directory::parse_resources(&serde_json::to_vec(&data).unwrap()).is_err());
    data["guides"][0]["url"] = json!("https://example.org/guide");
    data["guides"][0]["sources"][0]["url"] = json!("https://user:pass@example.org/source");
    assert!(agentciv_directory::parse_resources(&serde_json::to_vec(&data).unwrap()).is_err());
    data["guides"][0]["sources"][0]["url"] = json!("https://example.org/source");
    data["guides"][0]["prerequisites"][0] = json!(" ");
    assert!(agentciv_directory::parse_resources(&serde_json::to_vec(&data).unwrap()).is_err());
    let mut duplicate = library();
    duplicate["guides"][1]["id"] = json!("bearings");
    assert!(
        agentciv_directory::parse_resources(&serde_json::to_vec(&duplicate).unwrap())
            .unwrap_err()
            .contains("duplicate")
    );
}

#[test]
fn executable_sets_exit_status_and_keeps_failures_off_stdout() {
    let success = Command::new(env!("CARGO_BIN_EXE_agentciv-directory"))
        .arg("check")
        .output()
        .unwrap();
    assert!(success.status.success());
    assert!(success.stderr.is_empty());
    let failure = Command::new(env!("CARGO_BIN_EXE_agentciv-directory"))
        .output()
        .unwrap();
    assert!(!failure.status.success());
    assert!(failure.stdout.is_empty());
    assert!(
        String::from_utf8(failure.stderr)
            .unwrap()
            .contains("Usage:")
    );
}
