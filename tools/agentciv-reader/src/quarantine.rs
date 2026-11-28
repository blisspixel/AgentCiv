//! Offline CaMeL-Lite provenance quarantine projection for saved room views.
//! Revalidates a saved complete caller-view traversal and wraps every event
//! payload in explicit untrusted-data provenance tags for downstream readers.

use crate::offers::{Validated, validated};
use crate::{Error, Result};
use agentciv_archive::MAX_INPUT_BYTES;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

pub const FORMAT: &str = "agentciv-quarantine-view/0.1-example";

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn event_from(event: &Value) -> Value {
    let nested = match event["kind"].as_str() {
        Some("message.recorded") => event["body"]["message"]["from"].as_str(),
        Some("artifact.recorded") => event["body"]["artifact_revision"]["from"].as_str(),
        Some("objection.recorded") => event["body"]["objection"]["from"].as_str(),
        Some("decline.recorded") => event["body"]["decline"]["from"].as_str(),
        Some("artifact.withdrawn") => event["body"]["withdrawal"]["from"].as_str(),
        _ => None,
    };
    match nested.or_else(|| event["actor"].as_str()) {
        Some(sender) => json!(sender),
        None => Value::Null,
    }
}

/// Project a saved full `read` result into a CaMeL-Lite provenance-tagged quarantine view.
/// Every event is explicitly tagged as untrusted peer text to be treated as data only.
pub fn project(raw: &str) -> Result<Value> {
    let Validated {
        world,
        report,
        records,
        events,
    } = validated(raw).map_err(|error| match error {
        Error::InvalidView => Error::InvalidQuarantineView,
        other => other,
    })?;

    let mut quarantined = Vec::with_capacity(events.len());
    for (record, event) in records.iter().zip(events.iter()) {
        let sequence = crate::integer(&event["sequence"]).ok_or(Error::InvalidEvent)?;
        let event_id = event["id"].as_str().ok_or(Error::InvalidEvent)?;
        let kind = event["kind"].as_str().ok_or(Error::InvalidEvent)?;
        let timestamp = event["timestamp"].as_str().ok_or(Error::InvalidEvent)?;
        let from = event_from(event);
        let raw_event_sha256 = hex_lower(&Sha256::digest(record.as_bytes()));
        quarantined.push(json!({
            "content_trust": "untrusted_peer_text",
            "outside_systems": "not_granted",
            "treat_as_data_only": true,
            "sequence": sequence,
            "event_id": event_id,
            "from": from,
            "kind": kind,
            "timestamp": timestamp,
            "raw_event_sha256": raw_event_sha256,
            "payload": event["body"],
            "record_utf8": record,
        }));
    }

    let view = json!({
        "format": FORMAT,
        "world": world,
        "content_trust": "untrusted_peer_text",
        "outside_systems": "not_granted",
        "treat_as_data_only": true,
        "events": quarantined,
        "report": {
            "scope": "current_caller_view",
            "copying_permission": "not_granted",
            "reached_end": true,
            "pages": report.pages,
            "events": report.events,
            "response_bytes": report.response_bytes,
            "quarantined_events": events.len(),
            "input_authentication": "not_established_by_offline_projection",
        }
    });
    if serde_json::to_vec(&view).map_err(|_| Error::Output)?.len() > MAX_INPUT_BYTES {
        return Err(Error::Output);
    }
    Ok(view)
}
