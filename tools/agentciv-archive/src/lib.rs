//! Optional offline utility. The written archive contract and schemas are authoritative.
//! Nothing here authenticates a source, grants authority, fetches links, or executes records.

mod json;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_RECORD_BYTES: usize = 16_000;
pub const MAX_ENTRIES: usize = 256;
pub type Result<T> = std::result::Result<T, &'static str>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    world: String,
    records: Vec<String>,
}

/// This is separately supplied trusted local configuration, never read from a record.
/// The caller must establish current copying permission and review selected payloads.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Permit {
    world: String,
    audience: Vec<String>,
    event_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub event_id: String,
    pub sha256: String,
    pub record_utf8: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub protocol_version: String,
    pub r#type: String,
    pub archive_version: String,
    pub world: String,
    pub audience: Vec<String>,
    pub selection: String,
    pub completeness: String,
    pub entries: Vec<Entry>,
}

const SCHEMAS: &[&str] = &[
    include_str!("../../../schemas/archive-bundle.schema.json"),
    include_str!("../../../schemas/event.schema.json"),
    include_str!("../../../schemas/message.schema.json"),
    include_str!("../../../schemas/collaboration-artifact.schema.json"),
    include_str!("../../../schemas/collaboration-objection.schema.json"),
    include_str!("../../../schemas/collaboration-decline.schema.json"),
];

fn validators() -> &'static Vec<jsonschema::Validator> {
    static VALIDATORS: OnceLock<Vec<jsonschema::Validator>> = OnceLock::new();
    VALIDATORS.get_or_init(|| {
        let schemas: Vec<Value> = SCHEMAS
            .iter()
            .map(|schema| serde_json::from_str(schema).expect("bundled schema parses"))
            .collect();
        let mut builder = jsonschema::Registry::new();
        for schema in &schemas {
            builder = builder
                .add(
                    schema["$id"].as_str().expect("schema has id"),
                    schema.clone(),
                )
                .expect("bundled schema registers");
        }
        let registry = builder.prepare().expect("bundled registry prepares");
        schemas
            .iter()
            .map(|schema| {
                jsonschema::options()
                    .with_registry(&registry)
                    .should_validate_formats(true)
                    .build(schema)
                    .expect("bundled schema compiles")
            })
            .collect()
    })
}

fn bounded_json(input: &str) -> Result<Value> {
    if input.len() > MAX_INPUT_BYTES {
        return Err("input_too_large");
    }
    json::parse(input)
}

fn labels(values: &[String]) -> bool {
    !values.is_empty()
        && values.len() <= MAX_ENTRIES
        && values
            .iter()
            .all(|value| !value.is_empty() && value.len() <= 1024)
        && values.iter().collect::<BTreeSet<_>>().len() == values.len()
}

fn digest(input: &str) -> String {
    format!("{:x}", Sha256::digest(input.as_bytes()))
}

fn integer(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        value
            .as_f64()
            .filter(|number| {
                *number >= 0.0 && *number <= 9_007_199_254_740_991.0 && number.fract() == 0.0
            })
            .map(|number| number as u64)
    })
}

fn record(input: &str, world: &str) -> Result<Value> {
    if input.len() > MAX_RECORD_BYTES {
        return Err("record_too_large");
    }
    let event = json::parse(input)?;
    if !validators()[1].is_valid(&event) || event["world"] != world {
        return Err("invalid_event");
    }
    let known = match event["kind"].as_str() {
        Some("message.recorded") => Some((2, "message", "message")),
        Some("artifact.recorded") => Some((3, "artifact_revision", "artifact_revision")),
        Some("objection.recorded") => Some((4, "objection", "objection")),
        Some("decline.recorded") => Some((5, "decline", "decline")),
        Some("event.redacted" | "artifact.withdrawn") => {
            if event["body"]
                .as_object()
                .is_none_or(|body| !body.is_empty())
            {
                return Err("invalid_tombstone");
            }
            None
        }
        _ => None,
    };
    if let Some((schema, field, kind)) = known {
        let submitted = &event["body"][field];
        if !validators()[schema].is_valid(submitted)
            || submitted["type"] != kind
            || submitted["world"] != world
            || event["actor"] != submitted["from"]
            || event["actor"].as_str().is_none()
            || (kind == "artifact_revision" && integer(&submitted["revision"]).is_none())
        {
            return Err("invalid_nested_record");
        }
    }
    Ok(event)
}

fn checked_records(entries: &[Entry], world: &str) -> Result<Vec<Value>> {
    if entries.is_empty() || entries.len() > MAX_ENTRIES {
        return Err("entry_limit");
    }
    let mut ids = BTreeSet::new();
    let mut revisions = BTreeSet::new();
    let mut previous = None;
    let mut events = Vec::new();
    for entry in entries {
        if entry.event_id.is_empty() || entry.event_id.len() > 1024 {
            return Err("invalid_event_id");
        }
        if digest(&entry.record_utf8) != entry.sha256 {
            return Err("digest_mismatch");
        }
        let event = record(&entry.record_utf8, world)?;
        if event["id"] != entry.event_id || !ids.insert(entry.event_id.clone()) {
            return Err("event_identity_conflict");
        }
        let sequence = integer(&event["sequence"]).ok_or("invalid_sequence")?;
        if previous.is_some_and(|old| sequence <= old) {
            return Err("sequence_conflict");
        }
        previous = Some(sequence);
        if event["kind"] == "artifact.recorded" {
            let artifact = &event["body"]["artifact_revision"];
            let key = (
                artifact["from"].as_str().unwrap().to_owned(),
                artifact["artifact_id"].as_str().unwrap().to_owned(),
                integer(&artifact["revision"]).unwrap(),
            );
            if !revisions.insert(key) {
                return Err("revision_identity_conflict");
            }
        }
        events.push(event);
    }
    Ok(events)
}

/// Select only explicitly allowlisted records. The permit is a local operator assertion,
/// not inferred from readership, audiences in untrusted records, or a portable grant.
pub fn export(snapshot: &str, permit: &str) -> Result<Bundle> {
    let snapshot: Snapshot =
        serde_json::from_value(bounded_json(snapshot)?).map_err(|_| "invalid_snapshot")?;
    let permit: Permit =
        serde_json::from_value(bounded_json(permit)?).map_err(|_| "invalid_permit")?;
    if snapshot.world.is_empty()
        || snapshot.world.len() > 1024
        || permit.world != snapshot.world
        || !labels(&permit.audience)
        || !labels(&permit.event_ids)
    {
        return Err("invalid_permit");
    }
    if snapshot.records.is_empty() || snapshot.records.len() > MAX_ENTRIES {
        return Err("entry_limit");
    }
    let mut entries = Vec::new();
    for raw in snapshot.records {
        let event = record(&raw, &snapshot.world)?;
        let event_id = event["id"].as_str().ok_or("invalid_event")?.to_owned();
        entries.push(Entry {
            event_id,
            sha256: digest(&raw),
            record_utf8: raw,
        });
    }
    checked_records(&entries, &snapshot.world)?;
    let wanted: BTreeSet<_> = permit.event_ids.iter().collect();
    entries.retain(|entry| wanted.contains(&entry.event_id));
    if entries.len() != wanted.len() {
        return Err("selection_unavailable");
    }
    let bundle = Bundle {
        protocol_version: "0.1-draft".into(),
        r#type: "archive_bundle".into(),
        archive_version: "archive-bundle/0.1-draft".into(),
        world: snapshot.world,
        audience: permit.audience,
        selection: "operator-selected".into(),
        completeness: "partial".into(),
        entries,
    };
    // The schema constrains wrapper fields; bytes and semantic relationships are checked here.
    validate(&serde_json::to_string(&bundle).map_err(|_| "serialization_failed")?)?;
    Ok(bundle)
}

/// Check shape, exact copied bytes and internal identities. No authentication or permission check.
pub fn validate(input: &str) -> Result<Bundle> {
    let value = bounded_json(input)?;
    if !validators()[0].is_valid(&value) {
        return Err("invalid_bundle");
    }
    let bundle: Bundle = serde_json::from_value(value).map_err(|_| "invalid_bundle")?;
    if !labels(&bundle.audience) || bundle.world.len() > 1024 {
        return Err("invalid_bundle");
    }
    checked_records(&bundle.entries, &bundle.world)?;
    Ok(bundle)
}

/// Recompute a disposable view from originals. No supplied index or summary is trusted.
pub fn inspect(input: &str) -> Result<Value> {
    let bundle = validate(input)?;
    let events = checked_records(&bundle.entries, &bundle.world)?;
    let mut artifact_ids = BTreeMap::new();
    let mut event_ids = BTreeMap::new();
    let mut artifacts = Vec::new();
    let mut relations = Vec::new();
    for event in &events {
        event_ids.insert(
            event["id"].as_str().unwrap(),
            event["kind"].as_str().unwrap(),
        );
        if event["kind"] == "artifact.recorded" {
            let artifact = &event["body"]["artifact_revision"];
            artifact_ids.insert(
                (
                    artifact["from"].as_str().unwrap(),
                    artifact["artifact_id"].as_str().unwrap(),
                    integer(&artifact["revision"]).unwrap(),
                ),
                event["id"].as_str().unwrap(),
            );
            artifacts.push(json!({"event_id":event["id"], "from":artifact["from"],
                "artifact_id":artifact["artifact_id"], "revision":artifact["revision"]}));
        }
    }
    for event in &events {
        let (field, relation) = match event["kind"].as_str() {
            Some("artifact.recorded") => ("artifact_revision", "derived_from"),
            Some("objection.recorded") => ("objection", "objection_target"),
            Some("decline.recorded") => ("decline", "decline_target"),
            _ => continue,
        };
        let submitted = &event["body"][field];
        let target = if relation == "derived_from" {
            submitted.get("derived_from").cloned()
        } else {
            Some(
                json!({"from":submitted["target_from"], "artifact_id":submitted["artifact_id"],
                "revision":submitted["revision"]}),
            )
        };
        if let Some(target) = target {
            let found = artifact_ids.get(&(
                target["from"].as_str().unwrap(),
                target["artifact_id"].as_str().unwrap(),
                integer(&target["revision"]).unwrap(),
            ));
            relations.push(
                json!({"from_event_id":event["id"], "relation":relation, "target":target,
                "availability":if found.is_some() {"included"} else {"unavailable"},
                "target_event_id":found}),
            );
        }
    }
    Ok(
        json!({"archive_version":bundle.archive_version, "world":bundle.world,
        "bundle_sha256":digest(input),
        "audience":bundle.audience, "completeness":"partial", "copy_integrity":"matched",
        "source_authenticity":"unverified", "authority_transferred":false,
        "events":events.iter().map(|event| json!({"event_id":event["id"],
            "sequence":event["sequence"], "kind":event["kind"]})).collect::<Vec<_>>(),
        "artifacts":artifacts, "relations":relations,
        "unknown_kinds":event_ids.values().filter(|kind| !matches!(**kind,
            "message.recorded" | "artifact.recorded" | "objection.recorded" |
            "decline.recorded" | "event.redacted" | "artifact.withdrawn")).count()}),
    )
}
