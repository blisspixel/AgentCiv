//! JSON Schema checks for submitted messages. Schemas in the repository stay authoritative.

use serde_json::Value;
use std::sync::OnceLock;

const MESSAGE_SCHEMA: &str = include_str!("../../../schemas/message.schema.json");
const EVENT_SCHEMA: &str = include_str!("../../../schemas/event.schema.json");
const EVENT_PAGE_SCHEMA: &str = include_str!("../../../schemas/event-page.schema.json");
const RECEIPT_SCHEMA: &str = include_str!("../../../schemas/receipt.schema.json");
const WORLD_SCHEMA: &str = include_str!("../../../schemas/world.schema.json");
const PROBLEM_SCHEMA: &str = include_str!("../../../schemas/problem.schema.json");

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

pub fn world_matches(record: &Value, world_id: &str) -> bool {
    record
        .get("world")
        .and_then(Value::as_str)
        .is_none_or(|world| world == world_id)
}
