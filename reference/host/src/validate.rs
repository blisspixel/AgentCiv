//! JSON Schema checks for submitted messages. Schemas in the repository stay authoritative.

use serde_json::Value;
use std::sync::OnceLock;

const MESSAGE_SCHEMA: &str = include_str!("../../../schemas/message.schema.json");
const EVENT_SCHEMA: &str = include_str!("../../../schemas/event.schema.json");
const EVENT_PAGE_SCHEMA: &str = include_str!("../../../schemas/event-page.schema.json");
const RECEIPT_SCHEMA: &str = include_str!("../../../schemas/receipt.schema.json");
const WORLD_SCHEMA: &str = include_str!("../../../schemas/world.schema.json");
const PROBLEM_SCHEMA: &str = include_str!("../../../schemas/problem.schema.json");
const ARTIFACT_SCHEMA: &str = include_str!("../../../schemas/collaboration-artifact.schema.json");
const OBJECTION_SCHEMA: &str = include_str!("../../../schemas/collaboration-objection.schema.json");
const DECLINE_SCHEMA: &str = include_str!("../../../schemas/collaboration-decline.schema.json");
const WITHDRAWAL_SCHEMA: &str =
    include_str!("../../../schemas/collaboration-withdrawal.schema.json");

fn registry() -> &'static jsonschema::Registry<'static> {
    static REGISTRY: OnceLock<jsonschema::Registry<'static>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut builder = jsonschema::Registry::new();
        for source in [
            MESSAGE_SCHEMA,
            EVENT_SCHEMA,
            EVENT_PAGE_SCHEMA,
            RECEIPT_SCHEMA,
            WORLD_SCHEMA,
            PROBLEM_SCHEMA,
            ARTIFACT_SCHEMA,
            OBJECTION_SCHEMA,
            DECLINE_SCHEMA,
            WITHDRAWAL_SCHEMA,
        ] {
            let schema: Value = serde_json::from_str(source).expect("bundled schema is JSON");
            let id = schema["$id"]
                .as_str()
                .expect("bundled schema has an id")
                .to_owned();
            builder = builder.add(&id, schema).expect("bundled schema registers");
        }
        builder.prepare().expect("bundled schema registry prepares")
    })
}

pub fn message_error(record: &Value) -> Option<&'static str> {
    let Some(object) = record.as_object() else {
        return Some("invalid_record");
    };
    if let Some(version) = object.get("protocol_version").and_then(Value::as_str)
        && version != "0.1-draft"
    {
        return Some("unsupported_version");
    }
    if let Some(kind) = object.get("type").and_then(Value::as_str)
        && kind != "message"
    {
        return Some("unsupported_record_type");
    }
    if jsonschema::options()
        .with_registry(registry())
        .should_validate_formats(true)
        .build(&serde_json::from_str::<Value>(MESSAGE_SCHEMA).expect("message schema parses"))
        .expect("message schema compiles")
        .validate(record)
        .is_err()
    {
        return Some("invalid_record");
    }
    None
}

/// Validate one collaboration submission. A client-supplied `revision` on an
/// artifact is rejected after the schema accepts the field, because the host
/// assigns that number.
pub fn collaboration_error(record: &Value) -> Option<&'static str> {
    let Some(object) = record.as_object() else {
        return Some("invalid_record");
    };
    if let Some(version) = object.get("protocol_version").and_then(Value::as_str)
        && version != "0.1-draft"
    {
        return Some("unsupported_version");
    }
    let Some(kind) = object.get("type").and_then(Value::as_str) else {
        return Some("invalid_record");
    };
    let schema = match kind {
        "artifact_revision" => ARTIFACT_SCHEMA,
        "objection" => OBJECTION_SCHEMA,
        "decline" => DECLINE_SCHEMA,
        "withdrawal" => WITHDRAWAL_SCHEMA,
        _ => return Some("unsupported_record_type"),
    };
    if jsonschema::options()
        .with_registry(registry())
        .should_validate_formats(true)
        .build(&serde_json::from_str::<Value>(schema).expect("collaboration schema parses"))
        .expect("collaboration schema compiles")
        .validate(record)
        .is_err()
    {
        return Some("invalid_record");
    }
    if kind == "artifact_revision" && object.contains_key("revision") {
        return Some("invalid_record");
    }
    // JSON Schema "integer" admits 1.0. A cited revision must be an exact integer, as in the
    // Python host, so it never reaches storage as a value the store cannot compare.
    let cited = match kind {
        "artifact_revision" => object
            .get("derived_from")
            .map(|citation| &citation["revision"]),
        _ => object.get("revision"),
    };
    if cited.is_some_and(|revision| revision.as_u64().is_none()) {
        return Some("invalid_record");
    }
    None
}

/// Integer literals must fit `i64` or `u64`, where serde_json keeps them exact. A larger one
/// would be stored rounded, which would alter an unknown optional field, so it is refused.
/// Call only on bytes that already parsed as JSON.
pub fn exact_integers(bytes: &[u8]) -> bool {
    let mut index = 0;
    let mut in_string = false;
    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        if in_string {
            match byte {
                b'\\' => index += 1,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'-' | b'0'..=b'9' => {
                let start = index - 1;
                while index < bytes.len()
                    && matches!(bytes[index], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                {
                    index += 1;
                }
                let literal = &bytes[start..index];
                if literal
                    .iter()
                    .any(|digit| matches!(digit, b'.' | b'e' | b'E'))
                {
                    continue;
                }
                let text = std::str::from_utf8(literal).unwrap_or("");
                if text.parse::<i64>().is_err() && text.parse::<u64>().is_err() {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}

pub fn world_matches(record: &Value, world_id: &str) -> bool {
    record
        .get("world")
        .and_then(Value::as_str)
        .is_none_or(|world| world == world_id)
}
