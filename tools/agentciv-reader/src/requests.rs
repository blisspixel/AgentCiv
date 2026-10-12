//! Offline incoming-request syntax only, using the existing shared draft schemas.
//! No host state, authority, acceptance, receipt or retry outcome is established.

use crate::{Error, Result};
use agentciv_archive::{exact_integer_field, parse_unique};
use serde_json::Value;
use std::sync::OnceLock;

/// Local manual-room request ceiling, distinct from the 16000-byte stored-event bound.
pub const MAX_REQUEST_BYTES: usize = 16_384;

const SCHEMAS: &[&str] = &[
    include_str!("../../../schemas/message.schema.json"),
    include_str!("../../../schemas/collaboration-artifact.schema.json"),
    include_str!("../../../schemas/collaboration-objection.schema.json"),
    include_str!("../../../schemas/collaboration-decline.schema.json"),
    include_str!("../../../schemas/collaboration-withdrawal.schema.json"),
];

fn compile(schema: &Value) -> Result<jsonschema::Validator> {
    jsonschema::options()
        .offline()
        .should_validate_formats(true)
        .build(schema)
        .map_err(|_| Error::Configuration)
}

fn validators() -> &'static Vec<jsonschema::Validator> {
    static VALIDATORS: OnceLock<Vec<jsonschema::Validator>> = OnceLock::new();
    VALIDATORS.get_or_init(|| {
        SCHEMAS
            .iter()
            .map(|source| {
                let schema: Value = serde_json::from_str(source).expect("bundled schema parses");
                compile(&schema).expect("bundled request schema compiles offline")
            })
            .collect()
    })
}

/// Check one bounded incoming message or collaboration request without returning its content.
/// Unknown fields are schema data, never schema instructions or external links to retrieve.
/// Cited integer controls use their exact lexemes; unknown numeric bytes stay in the original.
/// A parser representation/depth refusal is a local limit, not a universal host rejection.
/// Success establishes syntax only, not grants, visible targets, authenticity or host acceptance.
pub fn check(raw: &str) -> Result<()> {
    if raw.len() > MAX_REQUEST_BYTES {
        return Err(Error::InputLimit);
    }
    let request = parse_unique(raw).map_err(|_| Error::InvalidJson)?;
    let kind = request["type"].as_str().ok_or(Error::InvalidRequest)?;
    let schema = match kind {
        "message" => 0,
        "artifact_revision" => 1,
        "objection" => 2,
        "decline" => 3,
        "withdrawal" => 4,
        _ => return Err(Error::InvalidRequest),
    };
    if !validators()[schema].is_valid(&request) {
        return Err(Error::InvalidRequest);
    }
    let revision = match kind {
        "artifact_revision" => {
            if request.get("revision").is_some() {
                return Err(Error::InvalidRequest);
            }
            request
                .get("derived_from")
                .map(|_| &["derived_from", "revision"][..])
        }
        "objection" | "decline" | "withdrawal" => Some(&["revision"][..]),
        _ => None,
    };
    if revision.is_some_and(|path| exact_integer_field(raw, path).is_none_or(|value| value == 0)) {
        return Err(Error::InvalidRequest);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
