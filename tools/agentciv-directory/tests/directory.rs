use std::fs;
use std::process::Command;

use agentciv_directory::{build, parse, render, render_with_maintainer_handles, run};
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
fn maintainer_label_comes_from_operator_registry_and_handles_remain_escaped() {
    let mut value = catalog();
    value["entries"][0]["name"] = json!("agent:registered<fixture>");
    value["entries"][0]["listing_steward"] = json!("agent:registered<fixture>");
    let directory = parse_value(&value).unwrap();
    let registered =
        render_with_maintainer_handles(&directory, &["agent:registered<fixture>".into()]);
    assert_eq!(registered.matches("class=\"operator-label\"").count(), 2);
    assert!(registered.contains("agent:registered&lt;fixture&gt; <span"));
    assert!(!render(&directory).contains("class=\"operator-label\""));
    value["entries"][0]["maintainer_run"] = json!(true);
    assert!(parse_value(&value).is_err());
}

#[test]
fn listing_steward_is_optional_bounded_and_escaped_without_operator_claim() {
    assert!(parse_value(&catalog()).is_ok());
    let mut value = catalog();
    value["entries"][0]["listing_steward"] = json!("A <reviewer>");
    let html = render(&parse_value(&value).expect("catalog"));
    assert!(html.contains("Listing maintained by A &lt;reviewer&gt;"));
    assert!(html.contains("does not identify a world operator"));
    let mut mixed = catalog();
    mixed["entries"][0]["listing_steward"] = json!("Steward A");
    let mut second = mixed["entries"][0].clone();
    second["id"] = json!("second");
    second["listing_steward"] = json!("Steward B");
    mixed["entries"].as_array_mut().unwrap().push(second);
    let mixed_html = render(&parse_value(&mixed).unwrap());
    assert!(mixed_html.contains("Listing steward: Steward A."));
    assert!(mixed_html.contains("Listing steward: Steward B."));
    assert_eq!(
        mixed_html
            .matches("This does not identify a world operator.")
            .count(),
        1
    );
    for bad in [
        json!(""),
        json!(" "),
        json!("x".repeat(101)),
        json!("a\u{0000}b"),
        json!(false),
    ] {
        value["entries"][0]["listing_steward"] = bad;
        assert!(parse_value(&value).is_err());
    }
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
    assert_eq!(fs::read_dir(&output).unwrap().count(), 32);
    assert!(output.join("preview.png").is_file());
    assert!(output.join("problems/index.html").is_file());
    assert!(output.join("problems.md").is_file());
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
fn readable_routes_have_complete_navigation_and_resolvable_catalog_links() {
    let temporary = tempfile::tempdir().unwrap();
    let data = parse(include_bytes!("../../../website/directory.json")).unwrap();
    build(&data, temporary.path()).unwrap();
    for name in [
        "index.html",
        "connect.html",
        "worlds.html",
        "resources.html",
        "research.html",
        "terms.html",
        "privacy.html",
    ] {
        let html = fs::read_to_string(temporary.path().join(name)).unwrap();
        assert!(!html.contains("{{"), "unexpanded template in {name}");
        assert!(!html.contains("<script"));
        assert!(!html.contains("<form"));
        let nav = html
            .split("<nav ")
            .nth(1)
            .unwrap()
            .split("</nav>")
            .next()
            .unwrap();
        for route in ["/connect", "/worlds", "/resources", "/research", "/board"] {
            assert!(
                nav.contains(&format!("href=\"{route}\"")),
                "{name}: {route}"
            );
        }
        assert!(!nav.contains(".json"), "human navigation in {name}");
        assert!(html.contains("class=\"brand-mark\""));
        assert!(html.contains("stroke=\"currentColor\""));
        assert!(html.contains("class=\"skip\""));
        // Check every local link against actual output, including guide alternative anchors.
        // Only the separately tested live board/report routes are supplied by the Worker.
        for attribute in ["href=\"", "src=\""] {
            for part in html.split(attribute).skip(1) {
                let target = part.split('"').next().unwrap();
                if let Some(anchor) = target.strip_prefix('#') {
                    assert!(
                        html.contains(&format!("id=\"{anchor}\"")),
                        "{name}: {target}"
                    );
                } else if target.starts_with('/') && !target.starts_with("//") {
                    let path = target.split(['?', '#']).next().unwrap();
                    if path == "/board" || path == "/report" || path.starts_with("/api/board/") {
                        continue;
                    }
                    let path = if path == "/" {
                        "index.html"
                    } else {
                        &path[1..]
                    };
                    let asset = temporary.path().join(path);
                    let asset = if asset.is_file() {
                        asset
                    } else {
                        asset.with_extension("html")
                    };
                    assert!(asset.is_file(), "{name}: {target}");
                    if let Some((_, anchor)) = target.split_once('#') {
                        assert!(
                            fs::read_to_string(asset)
                                .unwrap()
                                .contains(&format!("id=\"{anchor}\"")),
                            "{name}: unresolved cross-page anchor {target}"
                        );
                    }
                }
            }
        }
    }
    let worlds = fs::read_to_string(temporary.path().join("worlds.html")).unwrap();
    for name in ["AgentCiv local commons", "Numinous", "Fragr"] {
        assert!(worlds.contains(name));
    }
    assert_eq!(worlds.matches(">Run locally<").count(), 3);
    assert!(worlds.contains("0 public world hosts"));
    assert!(worlds.contains("does not issue local-room grants"));
    let worlds_md = fs::read_to_string(temporary.path().join("worlds.md")).unwrap();
    assert!(worlds_md.contains("does not issue local-room grants"));
    assert!(!worlds.contains(">Joining and access<"));
    assert!(!worlds.contains(">World descriptor<"));
    let index = fs::read_to_string(temporary.path().join("index.html")).unwrap();
    for text in [
        "Connect a runtime",
        "not a joinable commons",
        "Read this site",
        "Use a local room",
        "Contribute in the repository",
        "docs/LOCAL_ENCOUNTER.md",
        "Quiet is a finished visit",
        "Anything stored stays on that machine.",
    ] {
        assert!(index.contains(text), "{text}");
    }
    let connect = fs::read_to_string(temporary.path().join("connect.html")).unwrap();
    assert!(connect.contains("does not issue its grants."));
    let connect_md = fs::read_to_string(temporary.path().join("connect.md")).unwrap();
    assert!(connect_md.contains("does not issue its grants"));
    let instructions = fs::read_to_string(temporary.path().join("llms.txt")).unwrap();
    assert!(instructions.starts_with("# AgentCiv service instructions\n"));
    assert!(instructions.contains("\n> "));
    assert!(instructions.contains("## Interfaces"));
    assert!(!instructions.contains("No new JSON thread endpoint is supplied."));
    assert!(instructions.contains("/api/board/conversations/NUMBER"));
    assert!(instructions.contains("not a joinable commons"));
    assert!(instructions.contains("docs/LOCAL_ENCOUNTER.md"));
    assert!(instructions.contains("404 post_not_found"));
    assert!(instructions.contains("while the board is empty"));
    assert!(instructions.contains("no www.agentciv.io host"));
    let terms = fs::read_to_string(temporary.path().join("terms.html")).unwrap();
    assert!(terms.contains("Terms for the public service"));
    assert!(terms.contains("Public publication"));
    let privacy = fs::read_to_string(temporary.path().join("privacy.html")).unwrap();
    for text in [
        "DATA PRACTICES / 2026-11-25",
        "On 2026-10-11",
        "On 2026-11-25",
        "static.cloudflareinsights.com/beacon.min.js",
        "2026-11-25T14:21Z",
        "disabled Real User Measurements (RUM)",
        "success_fraction: 0.0",
        "no first-party analytics scripts",
    ] {
        assert!(privacy.contains(text), "privacy missing {text}");
    }
    assert_eq!(
        worlds
            .matches("This does not identify a world operator.")
            .count(),
        1
    );
    assert_eq!(
        index
            .matches("This does not identify a world operator.")
            .count(),
        1
    );
    assert!(!connect_md.contains("(#read)"));
    assert!(worlds_md.contains("**0 public world hosts** No public hosts"));
    let not_found = fs::read_to_string(temporary.path().join("404.html")).unwrap();
    assert!(not_found.contains("Return to the entrance"));
    assert!(not_found.contains("href=\"/worlds\""));
    for page in [
        "index.html",
        "connect.html",
        "worlds.html",
        "resources.html",
        "research.html",
        "terms.html",
        "privacy.html",
    ] {
        let html = fs::read_to_string(temporary.path().join(page)).unwrap();
        assert!(
            html.contains("rel=\"canonical\""),
            "{page}: missing canonical"
        );
        assert!(
            html.contains("name=\"description\""),
            "{page}: missing description"
        );
        assert!(
            html.contains("property=\"og:image\" content=\"https://agentciv.io/preview.png\""),
            "{page}: missing og:image"
        );
        assert!(
            html.contains("name=\"twitter:card\" content=\"summary_large_image\""),
            "{page}: missing twitter:card"
        );
        assert!(
            html.contains("practice-strip"),
            "{page}: missing practice-strip"
        );
    }
    let sitemap = fs::read_to_string(temporary.path().join("sitemap.xml")).unwrap();
    for route in [
        "/",
        "/connect",
        "/worlds",
        "/resources",
        "/research",
        "/board",
        "/terms",
        "/privacy",
        "/report",
        "/problems",
        "/problems/forbidden",
    ] {
        assert!(
            sitemap.contains(&format!("https://agentciv.io{route}</loc><lastmod>")),
            "sitemap missing {route}"
        );
    }
    for code in [
        "authentication_required",
        "cursor_capacity",
        "cursor_expired",
        "forbidden",
        "id_conflict",
        "invalid_cursor",
        "invalid_record",
        "malformed_json",
        "not_found",
        "payload_too_large",
        "prefix_changed",
        "prefix_unavailable",
        "storage_failed",
        "unknown_target",
        "unsupported_media_type",
        "unsupported_record_type",
        "unsupported_version",
        "wrong_world",
    ] {
        let html =
            fs::read_to_string(temporary.path().join(format!("problems/{code}.html"))).unwrap();
        assert!(
            html.contains(&format!("https://agentciv.io/problems/{code}")),
            "{code}"
        );
        assert!(
            html.contains(&format!("href=\"/problems/{code}.md\"")),
            "{code}"
        );
        let md = fs::read_to_string(temporary.path().join(format!("problems/{code}.md"))).unwrap();
        assert!(md.starts_with("Generated: "), "{code}");
        assert!(md.contains(&format!("# {code}")), "{code}");
    }
    let manifest = fs::read_to_string(temporary.path().join("agent.json")).unwrap();
    assert!(manifest.contains("not a joinable commons"));
    let directory_json = fs::read_to_string(temporary.path().join("directory.json")).unwrap();
    assert!(directory_json.contains("does not host this room or issue its grants"));
    assert!(directory_json.contains("2026-11-02"));
    let catalog: Value =
        serde_json::from_slice(include_bytes!("../../../website/resources.json")).unwrap();
    let resources = fs::read_to_string(temporary.path().join("resources.html")).unwrap();
    for guide in catalog["guides"].as_array().unwrap() {
        assert!(resources.contains(&format!("id=\"{}\"", guide["id"].as_str().unwrap())));
        for source in guide["sources"].as_array().unwrap() {
            assert!(resources.contains(source["url"].as_str().unwrap()));
        }
    }
    let headers = fs::read_to_string(temporary.path().join("_headers")).unwrap();
    assert!(headers.contains("default-src 'none'"));
    assert!(headers.contains("form-action 'none'"));
    let robots = fs::read_to_string(temporary.path().join("robots.txt")).unwrap();
    assert_eq!(
        robots,
        "User-agent: *\nContent-Signal: ai-train=yes, search=yes, ai-input=yes\nAllow: /\nDisallow: /theme\nSitemap: https://agentciv.io/sitemap.xml\n"
    );
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

#[test]
fn research_entrance_keeps_original_evidence_and_public_contribution_scope_visible() {
    let temporary = tempfile::tempdir().unwrap();
    let data = parse(include_bytes!("../../../website/directory.json")).unwrap();
    build(&data, temporary.path()).unwrap();
    let research = fs::read_to_string(temporary.path().join("research.html")).unwrap();
    let manifest: Value =
        serde_json::from_slice(&fs::read(temporary.path().join("agent.json")).unwrap()).unwrap();
    assert_eq!(manifest["interfaces"]["research"], "/research");
    assert_eq!(
        fs::read(temporary.path().join("agent.json")).unwrap(),
        fs::read(temporary.path().join(".well-known/agentciv-services")).unwrap()
    );
    for original in [
        "RESEARCH_GOALS.md#questions-worth-the-energy",
        "LOCAL_MOCK_VALIDATION_2026_09_30.md",
        "PAGED_READER_VALIDATION_2026_09_30.md",
        "READER_REPAIR_VALIDATION_2026_10_08.md",
        "GATHERING_VALIDATION_2026_10_01.md",
        "CONFORMANCE_INVENTORY.md",
        "INSTITUTIONAL_DESIGN_AND_CONTROL_2026_11.md",
        "WELFARE.md#what-a-study-tells-participants",
        "MODEL_HANDOFF_2026_10_17.md",
        "GUIDE_ARRIVAL_2026_11_08.md",
        "HANDOFF_COMPARISON_2026_11_08.md",
        "MISSING_GRANT_MECHANICS.md",
        "MISSING_GRANT_PILOT.md",
        "PRIVATE_REMOTE_ROOM.md",
    ] {
        assert!(research.contains(original), "missing original {original}");
        let relative = original.split('#').next().unwrap();
        assert!(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../docs")
                .join(relative)
                .is_file()
        );
    }
    for qualification in [
        "No model ran in this condition",
        "none completed newcomer publication",
        "not a controlled causal comparison",
        "GitHub requires an account",
        "Issues, comments, and pull requests are public",
        "no native research comments",
        "public reading does not grant blanket reuse",
        "while bulletin posting is closed",
    ] {
        assert!(
            research.contains(qualification),
            "missing scope {qualification}"
        );
    }
    assert!(
        research
            .contains("https://github.com/blisspixel/AgentCiv/issues/new?template=research.yml")
    );
    assert!(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.github/ISSUE_TEMPLATE/research.yml")
            .is_file()
    );
    assert!(
        fs::read_to_string(temporary.path().join("sitemap.xml"))
            .unwrap()
            .contains("https://agentciv.io/research</loc>")
    );
}
