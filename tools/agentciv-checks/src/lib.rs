//! Repository checks for documentation and draft protocol fixtures.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;

type CheckResult<T> = Result<T, Box<dyn Error>>;

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
                if !matches!(entry.file_name().to_str(), Some(".git" | "target")) {
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

fn read_json(path: &Path) -> CheckResult<Value> {
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn validate_record(schema: &Value, record: &Value) -> CheckResult<()> {
    let validator = jsonschema::options()
        .should_validate_formats(true)
        .build(schema)?;
    validator
        .validate(record)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    Ok(())
}

/// Compile every schema and validate its matching example fixture.
pub fn check_schemas(root: &Path) -> CheckResult<Vec<String>> {
    let schema_dir = root.join("schemas");
    let fixture_dir = root.join("conformance/fixtures/valid");
    let mut schema_paths = fs::read_dir(schema_dir)?
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
                validate_record(&schema, &record)?;
                validate_record(&envelope, &record)
            });
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
    fn invalid_version_and_missing_required_fields_are_rejected() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let schemas = root.join("schemas");
        let fixtures = root.join("conformance/fixtures/valid");
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
            let mut wrong_version = record.clone();
            wrong_version["protocol_version"] = Value::from("unsupported");
            assert!(validate_record(&schema, &wrong_version).is_err(), "{name}");
            for field in schema["required"].as_array().unwrap() {
                let mut missing = record.clone();
                missing
                    .as_object_mut()
                    .unwrap()
                    .remove(field.as_str().unwrap());
                assert!(
                    validate_record(&schema, &missing).is_err(),
                    "{name}: {field}"
                );
            }
        }
    }

    #[test]
    fn invalid_date_time_format_is_rejected() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let schema = read_json(&root.join("schemas/event.schema.json")).unwrap();
        let mut record = read_json(&root.join("conformance/fixtures/valid/event.json")).unwrap();
        record["timestamp"] = Value::from("not-a-date");
        assert!(validate_record(&schema, &record).is_err());
    }
}
