//! Traceability checks, not a second protocol or a conformance verdict.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path};

use serde::Deserialize;
use serde_json::Value;

use crate::CheckResult;

const INVENTORY: &str = "conformance/requirements.json";
const SOURCES: [&str; 2] = ["PROTOCOL.md", "docs/COLLABORATION_PROFILE.md"];
const MAX_INVENTORY_BYTES: u64 = 262_144;
const MAX_REFERENCE_BYTES: u64 = 1_048_576;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    format: String,
    profile_complete: bool,
    requirements: Vec<Requirement>,
    references: Vec<Reference>,
    observations: Vec<Observation>,
    gaps: Vec<Gap>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Requirement {
    id: String,
    source: String,
    quote: String,
    obligation: Obligation,
    applicability: String,
    coverage: Coverage,
    references: Vec<String>,
    observations: Vec<String>,
    gaps: Vec<String>,
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Obligation {
    Required,
    Conditional,
    Optional,
    Recommended,
    Informational,
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Coverage {
    PartialPublic,
    ImplementationOnly,
    Gap,
    Informational,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    id: String,
    kind: ReferenceKind,
    path: String,
    anchor: String,
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum ReferenceKind {
    PublicCase,
    ImplementationTest,
    SchemaFixture,
    Setup,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    id: String,
    path: String,
    pointer: String,
    case_id: String,
    status: String,
    required: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Gap {
    id: String,
    owner: String,
    proposed_test: String,
}

fn bounded_text(root: &Path, relative: &str, limit: u64) -> CheckResult<String> {
    if relative.is_empty()
        || relative.contains('\\')
        || !Path::new(relative)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err("inventory path must be relative and contain only normal components".into());
    }
    let path = root.join(relative).canonicalize()?;
    if !path.starts_with(root.canonicalize()?) {
        return Err("inventory reference leaves repository".into());
    }
    let mut text = String::new();
    File::open(path)?
        .take(limit + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > limit {
        return Err("inventory input exceeds byte limit".into());
    }
    Ok(text)
}

// Include whole prose paragraphs, tables, and individual list items in the
// contract sections. This catches uncapitalized requirements as well as MUST.
// Code examples, status prose, references, and test-setup commentary are excluded.
fn clauses(text: &str, collaboration: bool) -> Vec<String> {
    let mut active = false;
    let mut fenced = false;
    let mut pending = Vec::new();
    let mut result = Vec::new();
    let flush = |pending: &mut Vec<&str>, result: &mut Vec<String>| {
        if !pending.is_empty() {
            result.push(pending.join("\n"));
            pending.clear();
        }
    };
    for line in text.lines() {
        if line.starts_with("```") {
            flush(&mut pending, &mut result);
            fenced = !fenced;
        } else if !fenced && line.starts_with('#') {
            flush(&mut pending, &mut result);
            if line.starts_with("## ") {
                active = if collaboration {
                    !matches!(line, "## References")
                } else {
                    !matches!(line, "## Test setup and limits" | "## References")
                };
            }
        } else if !fenced && active {
            if line.trim().is_empty() {
                flush(&mut pending, &mut result);
            } else {
                if line.starts_with("- ")
                    || line
                        .chars()
                        .next()
                        .is_some_and(|first| first.is_ascii_digit())
                        && line.contains(". ")
                {
                    flush(&mut pending, &mut result);
                }
                pending.push(line);
            }
        }
    }
    flush(&mut pending, &mut result);
    result
}

fn unique_ids<'a>(ids: impl Iterator<Item = &'a str>, issues: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    for id in ids {
        if id.is_empty() || id.len() > 100 || !seen.insert(id) {
            issues.push(format!("inventory: empty, overlong, or duplicate id: {id}"));
        }
    }
}

fn is_test_anchor(reference: &Reference, text: &str) -> bool {
    if reference.path.ends_with(".py") {
        reference
            .path
            .rsplit('/')
            .next()
            .is_some_and(|name| name.starts_with("test_"))
            && reference.anchor.starts_with("def test_")
    } else if reference.path.ends_with(".rs")
        && (reference.path.ends_with("tests.rs") || reference.path.contains("/tests/"))
        && (reference.anchor.starts_with("fn ") || reference.anchor.starts_with("async fn "))
    {
        text.find(&reference.anchor).is_some_and(|offset| {
            let preceding = &text[..offset];
            preceding
                .lines()
                .rev()
                .take(2)
                .any(|line| line.contains("#[") && line.contains("test"))
        })
    } else {
        false
    }
}

fn validate(root: &Path, inventory: &Inventory) -> CheckResult<Vec<String>> {
    let mut issues = Vec::new();
    if inventory.format != "agentciv-conformance-inventory/0.1" || inventory.profile_complete {
        issues.push("inventory: unknown format or unsupported full-profile claim".to_owned());
    }
    unique_ids(
        inventory
            .requirements
            .iter()
            .map(|entry| entry.id.as_str())
            .chain(inventory.references.iter().map(|entry| entry.id.as_str()))
            .chain(inventory.observations.iter().map(|entry| entry.id.as_str()))
            .chain(inventory.gaps.iter().map(|entry| entry.id.as_str())),
        &mut issues,
    );
    let mut expected = BTreeSet::new();
    for source in SOURCES {
        let text = bounded_text(root, source, MAX_REFERENCE_BYTES)?;
        for quote in clauses(&text, source != "PROTOCOL.md") {
            expected.insert((source.to_owned(), quote));
        }
    }
    let references: BTreeMap<_, _> = inventory
        .references
        .iter()
        .map(|entry| (&entry.id, entry))
        .collect();
    let observations: BTreeSet<_> = inventory
        .observations
        .iter()
        .map(|entry| &entry.id)
        .collect();
    let gaps: BTreeSet<_> = inventory.gaps.iter().map(|entry| &entry.id).collect();
    for reference in &inventory.references {
        let result = bounded_text(root, &reference.path, MAX_REFERENCE_BYTES);
        match result {
            Ok(text)
                if !reference.anchor.is_empty()
                    && reference.anchor.len() <= 256
                    && text.contains(&reference.anchor) =>
            {
                if reference.kind == ReferenceKind::ImplementationTest
                    && !is_test_anchor(reference, &text)
                {
                    issues.push(format!(
                        "{}: implementation evidence must name an actual test",
                        reference.id
                    ));
                }
                if reference.kind == ReferenceKind::PublicCase
                    && (!reference.path.starts_with("conformance/runner/src/")
                        || !reference.path.ends_with(".rs")
                        || !reference.anchor.starts_with('"')
                        || !reference.anchor.ends_with('"'))
                {
                    issues.push(format!(
                        "{}: public evidence must name a runner case",
                        reference.id
                    ));
                }
            }
            _ => issues.push(format!(
                "{}: missing, unsafe, overlong, or stale reference",
                reference.id
            )),
        }
    }
    for observation in &inventory.observations {
        let text = bounded_text(root, &observation.path, MAX_REFERENCE_BYTES)?;
        let report: Value = serde_json::from_str(&text)?;
        let cases = report
            .pointer(&observation.pointer)
            .and_then(Value::as_array)
            .ok_or("observation pointer must name a retained cases array")?;
        let found: Vec<_> = cases
            .iter()
            .filter(|case| case["id"] == observation.case_id)
            .collect();
        let matches = match found.as_slice() {
            [] => observation.status == "omitted",
            [case] => {
                matches!(observation.status.as_str(), "passed" | "failed" | "skipped")
                    && case["id"] == observation.case_id
                    && case["status"] == observation.status
                    && case["required"] == observation.required
            }
            _ => false,
        };
        if !matches {
            issues.push(format!(
                "{}: retained case status, required flag, identity, or pointer differs",
                observation.id
            ));
        }
    }
    for gap in &inventory.gaps {
        if gap.owner.trim().is_empty() || gap.proposed_test.trim().is_empty() {
            issues.push(format!("{}: gap needs an owner and proposed test", gap.id));
        }
    }
    for entry in &inventory.requirements {
        if !expected.remove(&(entry.source.clone(), entry.quote.clone())) {
            issues.push(format!("{}: stale or duplicate contract clause", entry.id));
        }
        if entry.applicability.trim().is_empty() {
            issues.push(format!("{}: missing applicability", entry.id));
        }
        let entry_refs: Vec<_> = entry
            .references
            .iter()
            .filter_map(|id| references.get(id))
            .collect();
        if entry_refs.len() != entry.references.len()
            || entry
                .observations
                .iter()
                .any(|id| !observations.contains(id))
            || entry.gaps.iter().any(|id| !gaps.contains(id))
        {
            issues.push(format!("{}: unknown evidence or gap id", entry.id));
        }
        let public = entry_refs
            .iter()
            .any(|reference| reference.kind == ReferenceKind::PublicCase);
        let implementation = entry_refs
            .iter()
            .any(|reference| reference.kind == ReferenceKind::ImplementationTest);
        if (entry.coverage == Coverage::PartialPublic && !public)
            || (entry.coverage == Coverage::ImplementationOnly && !implementation)
            || (entry.coverage == Coverage::Gap && entry.gaps.is_empty())
            || (entry.coverage == Coverage::Informational
                && entry.obligation != Obligation::Informational)
            || (entry.references.is_empty()
                && entry.gaps.is_empty()
                && entry.coverage != Coverage::Informational)
        {
            issues.push(format!(
                "{}: uncovered obligation or inconsistent evidence category",
                entry.id
            ));
        }
    }
    for (source, quote) in expected {
        issues.push(format!(
            "inventory: unaccounted contract clause in {source}: {}",
            quote.lines().next().unwrap_or("")
        ));
    }
    Ok(issues)
}

/// Check bounded inventory references and account for all in-scope source clauses.
///
/// A clean result does not prove semantic coverage, current runtime success, or
/// conformance. Retained passed, failed, skipped, and omitted states stay distinct.
pub fn check_inventory(root: &Path) -> CheckResult<Vec<String>> {
    let text = bounded_text(root, INVENTORY, MAX_INVENTORY_BYTES)?;
    let inventory: Inventory = serde_json::from_str(&text)?;
    validate(root, &inventory)
}

#[cfg(test)]
mod tests;
