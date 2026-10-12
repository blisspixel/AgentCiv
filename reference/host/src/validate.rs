//! JSON Schema checks for submitted messages. Schemas in the repository stay authoritative.

use serde_json::{Value, value::RawValue};
use std::collections::BTreeMap;
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
pub fn collaboration_error(record: &Value, bytes: &[u8]) -> Option<&'static str> {
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
    // Check original numeric lexemes before any floating-point rounding can admit a fraction.
    let cited = match kind {
        "artifact_revision" => object
            .get("derived_from")
            .map(|citation| &citation["revision"]),
        _ => object.get("revision"),
    };
    if cited.is_some() && exact_citation_revision(record, bytes).is_none() {
        return Some("invalid_record");
    }
    None
}

/// An exact in-range citation comparison key, without changing the submitted record.
/// Decimal and exponent spellings are integers when their mathematical value is integral.
pub fn exact_citation_revision(record: &Value, bytes: &[u8]) -> Option<i64> {
    let mut fields: BTreeMap<String, Box<RawValue>> = serde_json::from_slice(bytes).ok()?;
    if record["type"] == "artifact_revision" {
        fields = serde_json::from_str(fields.remove("derived_from")?.get()).ok()?;
    }
    exact_revision(fields.remove("revision")?.get())
}

fn exact_revision(raw: &str) -> Option<i64> {
    if raw.starts_with('-') {
        return None;
    }
    let (coefficient, exponent) = raw.split_once(['e', 'E']).unwrap_or((raw, "0"));
    let (whole, fraction) = coefficient.split_once('.').unwrap_or((coefficient, ""));
    let combined = format!("{whole}{fraction}");
    let digits = combined.trim_start_matches('0');
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let shift = exponent
        .parse::<i64>()
        .ok()?
        .checked_sub(fraction.len().try_into().ok()?)?;
    let integer = if shift < 0 {
        let places: usize = shift.checked_neg()?.try_into().ok()?;
        if places >= digits.len()
            || !digits[digits.len() - places..]
                .bytes()
                .all(|byte| byte == b'0')
        {
            return None;
        }
        digits[..digits.len() - places].to_owned()
    } else {
        let places: usize = shift.try_into().ok()?;
        if digits.len().checked_add(places)? > 16 {
            return None;
        }
        format!("{digits}{}", "0".repeat(places))
    };
    integer
        .parse::<i64>()
        .ok()
        .filter(|value| (1..=9_007_199_254_740_991).contains(value))
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

/// Fixed hints only. Schemas decide acceptance; values and arbitrary keys stay private.
pub fn invalid_record_title(record: &Value) -> &'static str {
    for (field, title) in [
        (
            "protocol_version",
            "Invalid record: check /protocol_version",
        ),
        ("type", "Invalid record: check /type"),
        ("id", "Invalid record: check /id"),
        ("world", "Invalid record: check /world"),
        ("from", "Invalid record: check /from"),
    ] {
        if record
            .get(field)
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            return title;
        }
    }
    if record["type"] != "withdrawal" {
        if record.get("body").and_then(Value::as_object).is_none() {
            return "Invalid record: check /body";
        }
        if record
            .get("to")
            .and_then(Value::as_array)
            .is_none_or(|items| {
                items.is_empty()
                    || items.iter().enumerate().any(|(index, item)| {
                        item.as_str().is_none_or(str::is_empty) || items[..index].contains(item)
                    })
            })
        {
            return "Invalid record: check /to";
        }
    }
    if record["type"] == "artifact_revision" && record.get("revision").is_some() {
        return "Invalid record: /revision is host-assigned";
    }
    "Invalid record: check required fields and their declared types"
}

pub fn world_matches(record: &Value, world_id: &str) -> bool {
    record
        .get("world")
        .and_then(Value::as_str)
        .is_none_or(|world| world == world_id)
}
