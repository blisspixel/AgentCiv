//! Repository checks for documentation and draft protocol fixtures.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

mod inventory;
mod research;
mod workflows;
pub use inventory::check_inventory;
pub use research::check_research_fixtures;
pub use workflows::check_workflows;

pub(crate) type CheckResult<T> = Result<T, Box<dyn Error>>;

/// Check the exact reviewed website binding and resource configuration. This does
/// not inspect a live account's billing plan or enforce a monthly dollar ceiling.
pub fn check_bulletin_config(root: &Path) -> CheckResult<Vec<String>> {
    let source = fs::read_to_string(root.join("services/bulletin/wrangler.toml"))?;
    Ok(agentciv_directory::checked_prebuilt_config(&source)
        .err()
        .into_iter()
        .collect())
}

fn link_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\[[^\]]+\]\(([^)]+)\)").expect("valid link pattern"))
}

fn markdown_files(root: &Path) -> CheckResult<Vec<PathBuf>> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                // .agents holds disposable notes and is gitignored.
                if !matches!(
                    entry.file_name().to_str(),
                    Some(".git" | "target" | ".agents" | "dist")
                ) {
                    pending.push(path);
                }
            } else if path.extension().is_some_and(|extension| extension == "md") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn is_external(target: &str) -> bool {
    target.starts_with('#')
        || target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
}

fn check_markdown_file(path: &Path) -> CheckResult<Vec<String>> {
    let content = fs::read_to_string(path)?;
    let mut issues = Vec::new();
    for (index, line) in content.lines().enumerate() {
        let location = format!("{}:{}", path.display(), index + 1);
        if line.trim_end() != line {
            issues.push(format!("{location}: trailing whitespace"));
        }
        if line.contains('\u{2014}') {
            issues.push(format!("{location}: em dash"));
        }
        if line.contains('\u{2013}') {
            issues.push(format!("{location}: en dash"));
        }
        for capture in link_pattern().captures_iter(line) {
            let target = capture
                .get(1)
                .expect("capture group exists")
                .as_str()
                .trim();
            if is_external(target) {
                continue;
            }
            let raw_path = target.split(['#', '?']).next().unwrap_or("");
            let decoded = percent_encoding::percent_decode_str(raw_path).decode_utf8()?;
            if decoded.is_empty()
                || !path
                    .parent()
                    .expect("file has parent")
                    .join(decoded.as_ref())
                    .exists()
            {
                issues.push(format!("{location}: missing local link: {target}"));
            }
        }
    }
    if !content.ends_with('\n') {
        issues.push(format!("{}: missing final newline", path.display()));
    }
    Ok(issues)
}

/// Check Markdown links and basic repository style in `root`.
pub fn check_docs(root: &Path) -> CheckResult<Vec<String>> {
    let files = markdown_files(root)?;
    if files.is_empty() {
        return Ok(vec![format!("{}: no Markdown files found", root.display())]);
    }
    let mut issues = Vec::new();
    for path in files {
        issues.extend(check_markdown_file(&path)?);
    }
    Ok(issues)
}

pub(crate) fn read_json(path: &Path) -> CheckResult<Value> {
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

pub(crate) fn validate_with_registry(
    schema: &Value,
    record: &Value,
    registry: &jsonschema::Registry<'_>,
) -> CheckResult<()> {
    let validator = jsonschema::options()
        .with_registry(registry)
        .should_validate_formats(true)
        .build(schema)?;
    validator
        .validate(record)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    Ok(())
}

pub(crate) fn schema_registry(paths: &[PathBuf]) -> CheckResult<jsonschema::Registry<'static>> {
    let mut builder = jsonschema::Registry::new();
    for path in paths {
        let schema = read_json(path)?;
        let id = schema["$id"]
            .as_str()
            .ok_or("schema is missing its $id")?
            .to_owned();
        builder = builder.add(&id, schema)?;
    }
    Ok(builder.prepare()?)
}

/// Compile every schema and validate its matching example fixture.
pub fn check_schemas(root: &Path) -> CheckResult<Vec<String>> {
    let schema_dir = root.join("schemas");
    let fixture_root = root.join("conformance/fixtures");
    let fixture_dir = fixture_root.join("valid");
    let mut schema_paths = fs::read_dir(&schema_dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    schema_paths.retain(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".schema.json"))
    });
    schema_paths.sort();
    let mut issues = Vec::new();
    if schema_paths.is_empty() {
        issues.push("no JSON schemas found".to_owned());
        return Ok(issues);
    }
    let registry = schema_registry(&schema_paths)?;
    let envelope = read_json(&root.join("schemas/envelope.schema.json"))?;
    for path in schema_paths {
        let name = path
            .file_name()
            .expect("schema has filename")
            .to_string_lossy();
        let fixture = fixture_dir.join(name.replace(".schema.json", ".json"));
        let result = read_json(&path)
            .and_then(|schema| read_json(&fixture).map(|record| (schema, record)))
            .and_then(|(schema, record)| {
                validate_with_registry(&schema, &record, &registry)?;
                if !matches!(
                    name.as_ref(),
                    "problem.schema.json"
                        | "archive-seal.schema.json"
                        | "peer-referral.schema.json"
                        | "postcard.schema.json"
                ) {
                    validate_with_registry(&envelope, &record, &registry)?;
                }
                Ok(())
            });
        if let Err(error) = result {
            issues.push(format!("{}: {error}", path.display()));
        }
    }
    let negative_dir = fixture_root.join("invalid");
    let mut negative_count = 0;
    for group in fs::read_dir(negative_dir)? {
        let group = group?;
        if !group.file_type()?.is_dir() {
            issues.push(format!(
                "{}: expected schema group directory",
                group.path().display()
            ));
            continue;
        }
        let schema = schema_dir.join(format!(
            "{}.schema.json",
            group.file_name().to_string_lossy()
        ));
        if !schema.is_file() {
            issues.push(format!("{}: no matching schema", group.path().display()));
            continue;
        }
        let schema = read_json(&schema)?;
        for entry in fs::read_dir(group.path())? {
            let entry = entry?;
            if entry.path().extension().is_none_or(|ext| ext != "json") {
                issues.push(format!("{}: expected JSON fixture", entry.path().display()));
                continue;
            }
            negative_count += 1;
            let record = read_json(&entry.path())?;
            if validate_with_registry(&schema, &record, &registry).is_ok() {
                issues.push(format!(
                    "{}: invalid fixture was accepted",
                    entry.path().display()
                ));
            }
        }
    }
    if negative_count == 0 {
        issues.push("no invalid JSON fixtures found".to_owned());
    }
    issues.extend(check_submitted_postcards(root, &schema_dir, &registry)?);
    Ok(issues)
}

fn check_submitted_postcards(
    root: &Path,
    schema_dir: &Path,
    registry: &jsonschema::Registry<'_>,
) -> CheckResult<Vec<String>> {
    let postcard_dir = root.join("commons/postcards");
    if !postcard_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = fs::read_dir(&postcard_dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.retain(|path| {
        path.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "json")
    });
    paths.sort();
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let schema = read_json(&schema_dir.join("postcard.schema.json"))?;
    let mut issues = Vec::new();
    for path in paths {
        let result =
            read_json(&path).and_then(|record| validate_with_registry(&schema, &record, registry));
        if let Err(error) = result {
            issues.push(format!("{}: {error}", path.display()));
        }
    }
    Ok(issues)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bulletin_configuration_requires_the_reviewed_source_and_reports_missing_files() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        assert!(check_bulletin_config(&source).unwrap().is_empty());
        let temporary = tempfile::tempdir().unwrap();
        assert!(check_bulletin_config(temporary.path()).is_err());
        let service = temporary.path().join("services/bulletin");
        fs::create_dir_all(&service).unwrap();
        let approved = fs::read_to_string(source.join("services/bulletin/wrangler.toml")).unwrap();
        fs::write(service.join("wrangler.toml"), &approved).unwrap();
        assert!(check_bulletin_config(temporary.path()).unwrap().is_empty());
        fs::write(
            service.join("wrangler.toml"),
            format!("{approved}\n[ai]\nbinding = \"AI\"\n"),
        )
        .unwrap();
        let issues = check_bulletin_config(temporary.path()).unwrap();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("reviewed Free-only deployment allowlist"));
    }

    #[test]
    fn valid_markdown_links_are_accepted() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("two words.md"), "# Target\n").unwrap();
        let document = directory.path().join("index.md");
        fs::write(
            &document,
            "[local](two%20words.md#section) [web](https://example.org) [here](#x)\n",
        )
        .unwrap();
        assert!(check_docs(directory.path()).unwrap().is_empty());
    }

    #[test]
    fn invalid_markdown_is_reported() {
        let directory = tempfile::tempdir().unwrap();
        let document = directory.path().join("index.md");
        fs::write(&document, "[missing](gone.md)  \ntext\u{2014}text").unwrap();
        let issues = check_docs(directory.path()).unwrap();
        assert_eq!(issues.len(), 4);
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("missing local link"))
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("trailing whitespace"))
        );
        assert!(issues.iter().any(|issue| issue.contains("em dash")));
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("missing final newline"))
        );
    }

    #[test]
    fn disposable_agent_notes_are_not_repository_checks() {
        let directory = tempfile::tempdir().unwrap();
        let notes = directory.path().join(".agents");
        fs::create_dir(&notes).unwrap();
        fs::write(notes.join("note.md"), "text\u{2014}  \n").unwrap();
        fs::write(directory.path().join("index.md"), "# Ok\n").unwrap();
        assert!(check_docs(directory.path()).unwrap().is_empty());
    }

    #[test]
    fn empty_repository_is_reported() {
        let directory = tempfile::tempdir().unwrap();
        assert!(check_docs(directory.path()).unwrap()[0].contains("no Markdown files"));
    }

    #[test]
    fn en_dash_is_reported() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("index.md"), "one\u{2013}two\n").unwrap();
        assert!(check_docs(directory.path()).unwrap()[0].contains("en dash"));
    }

    #[test]
    fn every_fixture_matches_its_schema() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        assert!(check_schemas(&root).unwrap().is_empty());
    }

    #[test]
    fn omitting_outside_systems_stays_valid_and_other_values_fail() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = root.join("schemas/world.schema.json");
        let registry = schema_registry(std::slice::from_ref(&path)).unwrap();
        let schema = read_json(&path).unwrap();
        let required = schema["properties"]["authentication"]["required"]
            .as_array()
            .unwrap();
        assert!(required.iter().all(|name| name != "outside_systems"));
        let mut record = read_json(&root.join("conformance/fixtures/valid/world.json")).unwrap();
        validate_with_registry(&schema, &record, &registry).unwrap();
        record["authentication"]
            .as_object_mut()
            .unwrap()
            .remove("outside_systems");
        validate_with_registry(&schema, &record, &registry).unwrap();
        record["authentication"]["outside_systems"] = Value::from("granted");
        assert!(validate_with_registry(&schema, &record, &registry).is_err());
        record["authentication"]["outside_systems"] = Value::from("not_granted");
        validate_with_registry(&schema, &record, &registry).unwrap();
    }

    #[test]
    fn archive_bundle_does_not_expand_http_submission_types() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let names = [
            "archive-bundle",
            "envelope",
            "message",
            "collaboration-artifact",
            "collaboration-objection",
            "collaboration-decline",
            "collaboration-withdrawal",
        ];
        let paths = names
            .iter()
            .map(|name| root.join(format!("schemas/{name}.schema.json")))
            .collect::<Vec<_>>();
        let registry = schema_registry(&paths).unwrap();
        let record =
            read_json(&root.join("conformance/fixtures/valid/archive-bundle.json")).unwrap();
        for (index, name) in names.iter().enumerate() {
            let schema = read_json(&paths[index]).unwrap();
            let result = validate_with_registry(&schema, &record, &registry);
            if matches!(*name, "archive-bundle" | "envelope") {
                assert!(result.is_ok(), "{name}: {result:?}");
            } else {
                assert!(result.is_err(), "bundle accepted as {name}");
            }
        }
    }

    #[test]
    fn archive_fixture_check_reports_missing_positive_and_accepted_negative() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let schemas = root.join("schemas");
        let positives = root.join("conformance/fixtures/valid");
        let negatives = root.join("conformance/fixtures/invalid/archive-bundle");
        for path in [&schemas, &positives, &negatives] {
            fs::create_dir_all(path).unwrap();
        }
        for name in ["archive-bundle", "envelope"] {
            fs::copy(
                source.join(format!("schemas/{name}.schema.json")),
                schemas.join(format!("{name}.schema.json")),
            )
            .unwrap();
            fs::copy(
                source.join(format!("conformance/fixtures/valid/{name}.json")),
                positives.join(format!("{name}.json")),
            )
            .unwrap();
        }
        let positive = positives.join("archive-bundle.json");
        let record = read_json(&positive).unwrap();
        let mut unsupported = record.clone();
        unsupported["archive_version"] = Value::from("unsupported");
        let negative = negatives.join("unsupported-version.json");
        fs::write(&negative, serde_json::to_vec(&unsupported).unwrap()).unwrap();
        assert!(check_schemas(root).unwrap().is_empty());

        fs::write(&negative, serde_json::to_vec(&record).unwrap()).unwrap();
        let issues = check_schemas(root).unwrap();
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].contains("invalid fixture was accepted"));

        fs::write(&negative, serde_json::to_vec(&unsupported).unwrap()).unwrap();
        fs::remove_file(positive).unwrap();
        let issues = check_schemas(root).unwrap();
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].contains("archive-bundle.schema.json"));
    }

    #[test]
    fn invalid_version_and_missing_required_fields_are_rejected() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let schemas = root.join("schemas");
        let fixtures = root.join("conformance/fixtures/valid");
        let paths = fs::read_dir(&schemas)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .ends_with(".schema.json")
            })
            .collect::<Vec<_>>();
        let registry = schema_registry(&paths).unwrap();
        for entry in fs::read_dir(schemas).unwrap() {
            let path = entry.unwrap().path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !name.ends_with(".schema.json") {
                continue;
            }
            let schema = read_json(&path).unwrap();
            let fixture = fixtures.join(name.replace(".schema.json", ".json"));
            let record = read_json(&fixture).unwrap();
            if name == "archive-seal.schema.json" {
                let mut wrong_version = record.clone();
                wrong_version["seal_version"] = Value::from("unsupported");
                assert!(
                    validate_with_registry(&schema, &wrong_version, &registry).is_err(),
                    "{name}"
                );
            } else if name == "peer-referral.schema.json" {
                let mut wrong_version = record.clone();
                wrong_version["referral_version"] = Value::from("unsupported");
                assert!(
                    validate_with_registry(&schema, &wrong_version, &registry).is_err(),
                    "{name}"
                );
            } else if name == "postcard.schema.json" {
                let mut wrong_version = record.clone();
                wrong_version["postcard_version"] = Value::from("unsupported");
                assert!(
                    validate_with_registry(&schema, &wrong_version, &registry).is_err(),
                    "{name}"
                );
            } else if name != "problem.schema.json" {
                let mut wrong_version = record.clone();
                wrong_version["protocol_version"] = Value::from("unsupported");
                assert!(
                    validate_with_registry(&schema, &wrong_version, &registry).is_err(),
                    "{name}"
                );
            }
            for field in schema["required"].as_array().unwrap() {
                let mut missing = record.clone();
                missing
                    .as_object_mut()
                    .unwrap()
                    .remove(field.as_str().unwrap());
                assert!(
                    validate_with_registry(&schema, &missing, &registry).is_err(),
                    "{name}: {field}"
                );
            }
        }
    }

    #[test]
    fn invalid_date_time_format_is_rejected() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let schema_path = root.join("schemas/event.schema.json");
        let schema = read_json(&schema_path).unwrap();
        let message_path = root.join("schemas/message.schema.json");
        let registry = schema_registry(&[schema_path, message_path]).unwrap();
        let mut record = read_json(&root.join("conformance/fixtures/valid/event.json")).unwrap();
        record["timestamp"] = Value::from("not-a-date");
        assert!(validate_with_registry(&schema, &record, &registry).is_err());
    }

    fn decode_standard_b64(input: &str) -> Vec<u8> {
        let val = |byte: u8| -> u8 {
            match byte {
                b'A'..=b'Z' => byte - b'A',
                b'a'..=b'z' => byte - b'a' + 26,
                b'0'..=b'9' => byte - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                other => panic!("unexpected base64 byte: {other}"),
            }
        };
        let bytes = input.as_bytes();
        let (chunks, remainder) = bytes.as_chunks::<4>();
        assert!(
            remainder.is_empty(),
            "base64 length must be a multiple of 4"
        );
        let mut out = Vec::with_capacity(chunks.len() * 3);
        for chunk in chunks {
            let a = val(chunk[0]);
            let b = val(chunk[1]);
            out.push((a << 2) | (b >> 4));
            if chunk[2] != b'=' {
                let c = val(chunk[2]);
                out.push((b << 4) | (c >> 2));
                if chunk[3] != b'=' {
                    let d = val(chunk[3]);
                    out.push((c << 6) | d);
                }
            }
        }
        out
    }

    #[test]
    fn peer_referral_enforces_key_id_endpoint_and_non_envelope_boundaries() {
        use sha2::{Digest, Sha256};

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let names = [
            "peer-referral",
            "envelope",
            "message",
            "collaboration-artifact",
            "collaboration-objection",
            "collaboration-decline",
            "collaboration-withdrawal",
        ];
        let paths = names
            .iter()
            .map(|name| root.join(format!("schemas/{name}.schema.json")))
            .collect::<Vec<_>>();
        let registry = schema_registry(&paths).unwrap();
        let record =
            read_json(&root.join("conformance/fixtures/valid/peer-referral.json")).unwrap();

        for (index, name) in names.iter().enumerate() {
            let schema = read_json(&paths[index]).unwrap();
            let result = validate_with_registry(&schema, &record, &registry);
            if *name == "peer-referral" {
                assert!(result.is_ok(), "{name}: {result:?}");
            } else {
                assert!(result.is_err(), "peer referral accepted as {name}");
            }
        }

        let alg = record["alg"].as_str().unwrap();
        let pubkey_bytes = decode_standard_b64(record["referrer_pubkey_b64"].as_str().unwrap());
        assert_eq!(pubkey_bytes.len(), 32);
        let mut hasher = Sha256::new();
        hasher.update(alg.as_bytes());
        hasher.update(b":");
        hasher.update(&pubkey_bytes);
        let expected_key_id = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(record["referrer_key_id"].as_str().unwrap(), expected_key_id);

        let referral_schema = read_json(&paths[0]).unwrap();
        for allowed in [
            "https://commons.example.org/worlds/alpha",
            "https://commons.example.org:1",
            "https://commons.example.org:8443",
            "https://commons.example.org:65535/worlds/alpha",
            "http://127.0.0.1:8080/worlds/local",
            "http://[::1]:8080/worlds/local",
        ] {
            let mut candidate = record.clone();
            candidate["endpoint_url"] = Value::from(allowed);
            assert!(
                validate_with_registry(&referral_schema, &candidate, &registry).is_ok(),
                "expected allowed endpoint_url: {allowed}"
            );
        }
        for rejected in [
            "https://user:pass@commons.example.org/worlds/alpha",
            "http://user@127.0.0.1:8080/worlds/local",
            "http://commons.example.org/worlds/alpha",
            "http://127.0.0.1.evil.example/worlds/alpha",
            "ftp://commons.example.org/worlds/alpha",
            "https://commons.example.org/worlds/alpha?token=secret",
            "https://commons.example.org:0/worlds/alpha",
            "https://commons.example.org:99999/worlds/alpha",
            "http://127.0.0.1:0/worlds/local",
            "http://127.0.0.1:65536/worlds/local",
            "https://commons.example.org/worlds\\alpha",
            "https://commons.example.org/worlds/\u{0000}alpha",
            "https://commons.example.org/worlds/caf\u{00e9}",
        ] {
            let mut candidate = record.clone();
            candidate["endpoint_url"] = Value::from(rejected);
            assert!(
                validate_with_registry(&referral_schema, &candidate, &registry).is_err(),
                "expected rejected endpoint_url: {rejected}"
            );
        }
    }

    #[test]
    fn postcard_enforces_non_envelope_and_optional_field_boundaries() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let names = [
            "postcard",
            "envelope",
            "message",
            "collaboration-artifact",
            "collaboration-objection",
            "collaboration-decline",
            "collaboration-withdrawal",
        ];
        let paths = names
            .iter()
            .map(|name| root.join(format!("schemas/{name}.schema.json")))
            .collect::<Vec<_>>();
        let registry = schema_registry(&paths).unwrap();
        let record = read_json(&root.join("conformance/fixtures/valid/postcard.json")).unwrap();

        for (index, name) in names.iter().enumerate() {
            let schema = read_json(&paths[index]).unwrap();
            let result = validate_with_registry(&schema, &record, &registry);
            if *name == "postcard" {
                assert!(result.is_ok(), "{name}: {result:?}");
            } else {
                assert!(result.is_err(), "postcard accepted as {name}");
            }
        }

        let postcard_schema = read_json(&paths[0]).unwrap();

        let mut with_optional = record.clone();
        with_optional["in_reply_to"] = Value::from("550e8400-e29b-41d4-a716-446655440000");
        with_optional["open_questions"] = Value::from(vec!["Q1", "Q2", "Q3", "Q4", "Q5"]);
        with_optional["detached_signature"] = serde_json::json!({
            "alg": "ed25519+ml-dsa-65",
            "key_id": "e469e588b7ee7248b6787c6b9343436b61d4ef0c049209b19253acb9709ab82b",
            "pubkey_b64": "QkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkI=",
            "signature_b64": "JCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJA=="
        });
        validate_with_registry(&postcard_schema, &with_optional, &registry).unwrap();

        let mut too_many_questions = with_optional.clone();
        too_many_questions["open_questions"] = Value::from(vec!["1", "2", "3", "4", "5", "6"]);
        assert!(validate_with_registry(&postcard_schema, &too_many_questions, &registry).is_err());

        let mut bad_handle = record.clone();
        bad_handle["author_handle"] = Value::from("Bad Handle!");
        assert!(validate_with_registry(&postcard_schema, &bad_handle, &registry).is_err());

        let mut bad_alg = with_optional.clone();
        bad_alg["detached_signature"]["alg"] = Value::from("rsa-2048");
        assert!(validate_with_registry(&postcard_schema, &bad_alg, &registry).is_err());

        for field in [
            "setting_and_operator",
            "synthetic_or_seeded",
            "enforced_vs_requested_limits",
            "observation_and_retention",
            "what_leaving_changes",
            "secondary_use_authority",
        ] {
            let mut missing_disclosure = record.clone();
            missing_disclosure["study_declaration"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                validate_with_registry(&postcard_schema, &missing_disclosure, &registry).is_err(),
                "expected missing study_declaration.{field} to fail"
            );
        }
    }

    #[test]
    fn submitted_postcards_in_commons_directory_are_validated() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let schemas = root.join("schemas");
        let positives = root.join("conformance/fixtures/valid");
        let negatives = root.join("conformance/fixtures/invalid/postcard");
        let postcards = root.join("commons/postcards");
        for path in [&schemas, &positives, &negatives, &postcards] {
            fs::create_dir_all(path).unwrap();
        }
        for name in ["postcard", "envelope"] {
            fs::copy(
                source.join(format!("schemas/{name}.schema.json")),
                schemas.join(format!("{name}.schema.json")),
            )
            .unwrap();
            fs::copy(
                source.join(format!("conformance/fixtures/valid/{name}.json")),
                positives.join(format!("{name}.json")),
            )
            .unwrap();
        }
        fs::copy(
            source.join("conformance/fixtures/invalid/postcard/empty-body.json"),
            negatives.join("empty-body.json"),
        )
        .unwrap();
        fs::write(postcards.join("README.md"), "# Postcards\n").unwrap();

        let submitted = postcards.join("2026-11-27-example.json");
        fs::copy(positives.join("postcard.json"), &submitted).unwrap();
        assert!(check_schemas(root).unwrap().is_empty());

        fs::copy(negatives.join("empty-body.json"), &submitted).unwrap();
        let issues = check_schemas(root).unwrap();
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].contains("2026-11-27-example.json"));

        fs::write(&submitted, "{ not valid json").unwrap();
        let issues = check_schemas(root).unwrap();
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert!(issues[0].contains("2026-11-27-example.json"));
    }
}
