//! Optional bounded history reading. The existing draft schemas remain authoritative.
//! Reading does not authorize copying, authenticate content, or execute records.

pub mod http;

use agentciv_archive::{
    MAX_ENTRIES, MAX_INPUT_BYTES, MAX_RECORD_BYTES, parse_unique, validate_event,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, value::RawValue};
use std::collections::BTreeSet;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub type Result<T> = std::result::Result<T, Error>;

/// Only fixed diagnostics can cross this boundary; transport errors carry no payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Configuration,
    InputUnavailable,
    InputLimit,
    InvalidJson,
    InvalidUtf8,
    InvalidOrigin,
    InvalidDiscovery,
    InvalidEndpoint,
    Transport,
    Http,
    Authentication,
    Forbidden,
    CursorExpired,
    ContentType,
    CacheControl,
    CredentialReflection,
    InvalidPage,
    InvalidEvent,
    EventConflict,
    SequenceConflict,
    RevisionConflict,
    CursorConflict,
    PageLimit,
    EventLimit,
    ResponseLimit,
    RecordLimit,
    TotalLimit,
    Deadline,
    Output,
}

impl Error {
    pub fn code(self) -> &'static str {
        match self {
            Self::Configuration => "invalid_configuration",
            Self::InputUnavailable => "input_unavailable",
            Self::InputLimit => "input_too_large",
            Self::InvalidJson => "invalid_json",
            Self::InvalidUtf8 => "invalid_utf8",
            Self::InvalidOrigin => "invalid_origin",
            Self::InvalidDiscovery => "invalid_discovery",
            Self::InvalidEndpoint => "invalid_endpoint",
            Self::Transport => "transport_failed",
            Self::Http => "http_failed",
            Self::Authentication => "authentication_required",
            Self::Forbidden => "forbidden",
            Self::CursorExpired => "cursor_expired",
            Self::ContentType => "invalid_content_type",
            Self::CacheControl => "missing_no_store",
            Self::CredentialReflection => "credential_reflected",
            Self::InvalidPage => "invalid_page",
            Self::InvalidEvent => "invalid_event",
            Self::EventConflict => "event_identity_conflict",
            Self::SequenceConflict => "sequence_conflict",
            Self::RevisionConflict => "revision_identity_conflict",
            Self::CursorConflict => "cursor_conflict",
            Self::PageLimit => "page_limit",
            Self::EventLimit => "event_limit",
            Self::ResponseLimit => "response_limit",
            Self::RecordLimit => "record_limit",
            Self::TotalLimit => "total_byte_limit",
            Self::Deadline => "deadline_exceeded",
            Self::Output => "output_failed",
        }
    }
}

/// Local resource choices, not additional wire requirements.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Budgets {
    pub max_pages: usize,
    pub max_events: usize,
    pub max_response_bytes: usize,
    pub max_record_bytes: usize,
    pub max_total_bytes: usize,
    pub seconds: u64,
}

impl Default for Budgets {
    fn default() -> Self {
        Self {
            max_pages: 20,
            max_events: MAX_ENTRIES,
            max_response_bytes: 262_144,
            max_record_bytes: MAX_RECORD_BYTES,
            max_total_bytes: 1_048_576,
            seconds: 30,
        }
    }
}

impl Budgets {
    pub fn validate(&self) -> Result<()> {
        if !(1..=256).contains(&self.max_pages)
            || !(1..=MAX_ENTRIES).contains(&self.max_events)
            || !(1..=MAX_INPUT_BYTES).contains(&self.max_response_bytes)
            || !(1..=MAX_RECORD_BYTES).contains(&self.max_record_bytes)
            || !(1..=MAX_INPUT_BYTES).contains(&self.max_total_bytes)
            || !(1..=150).contains(&self.seconds)
        {
            return Err(Error::Configuration);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Traversal {
    #[default]
    All,
    FirstPage,
}

#[derive(Debug, Serialize)]
pub struct Snapshot {
    pub world: String,
    pub records: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    /// Event endpoint requests, excluding discovery. No automatic retries occur.
    pub pages: usize,
    pub events: usize,
    /// Received JSON body bytes, including discovery for the HTTP adapter.
    pub response_bytes: usize,
    pub reached_end: bool,
    pub scope: &'static str,
    pub copying_permission: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ReadResult {
    pub snapshot: Snapshot,
    pub report: Report,
}

const SCHEMAS: &[&str] = &[
    include_str!("../../../schemas/world.schema.json"),
    include_str!("../../../schemas/event-page.schema.json"),
    include_str!("../../../schemas/event.schema.json"),
    include_str!("../../../schemas/message.schema.json"),
];

pub(crate) fn validators() -> &'static Vec<jsonschema::Validator> {
    static VALIDATORS: OnceLock<Vec<jsonschema::Validator>> = OnceLock::new();
    VALIDATORS.get_or_init(|| {
        let schemas: Vec<Value> = SCHEMAS
            .iter()
            .map(|source| serde_json::from_str(source).expect("bundled schema parses"))
            .collect();
        let mut builder = jsonschema::Registry::new();
        for schema in &schemas {
            builder = builder
                .add(schema["$id"].as_str().expect("schema id"), schema.clone())
                .expect("bundled schema registers");
        }
        let registry = builder.prepare().expect("bundled registry prepares");
        schemas[..2]
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

pub(crate) fn reflected(raw: &str, value: &Value, secret: Option<&str>) -> bool {
    fn contains(value: &Value, secret: &str) -> bool {
        match value {
            Value::String(value) => value.contains(secret),
            Value::Array(values) => values.iter().any(|value| contains(value, secret)),
            Value::Object(values) => values
                .iter()
                .any(|(key, value)| key.contains(secret) || contains(value, secret)),
            _ => false,
        }
    }
    secret.is_some_and(|secret| {
        !secret.is_empty() && (raw.contains(secret) || contains(value, secret))
    })
}

fn integer(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        value
            .as_f64()
            .filter(|value| value.is_finite() && *value >= 0.0 && value.fract() == 0.0)
            .map(|value| value as u64)
    })
}

#[derive(Deserialize)]
struct RawPage {
    events: Vec<Box<RawValue>>,
}

/// Fetch one page with the opaque cursor, remaining time, and response byte allowance.
/// A custom callback must bound its own work; this function cannot cancel arbitrary code.
/// Failed traversals return no snapshot. FirstPage is an explicit partial selection.
pub fn collect<F>(
    world: &str,
    budgets: &Budgets,
    traversal: Traversal,
    secret: Option<&str>,
    fetch: F,
) -> Result<ReadResult>
where
    F: FnMut(Option<&str>, Duration, usize) -> Result<Vec<u8>>,
{
    budgets.validate()?;
    collect_until(
        world,
        budgets,
        traversal,
        secret,
        Instant::now() + Duration::from_secs(budgets.seconds),
        0,
        fetch,
    )
}

pub(crate) fn collect_until<F>(
    world: &str,
    budgets: &Budgets,
    traversal: Traversal,
    secret: Option<&str>,
    deadline: Instant,
    mut bytes: usize,
    mut fetch: F,
) -> Result<ReadResult>
where
    F: FnMut(Option<&str>, Duration, usize) -> Result<Vec<u8>>,
{
    if world.is_empty() || world.len() > 1024 {
        return Err(Error::Configuration);
    }
    let mut cursor = None;
    let mut cursors = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut revisions = BTreeSet::new();
    let mut previous = None;
    let mut records = Vec::new();
    for page_count in 1..=budgets.max_pages {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or(Error::Deadline)?;
        let allowance = budgets
            .max_total_bytes
            .checked_sub(bytes)
            .filter(|left| *left > 0)
            .ok_or(Error::TotalLimit)?;
        let response = fetch(
            cursor.as_deref(),
            remaining,
            budgets.max_response_bytes.min(allowance),
        )?;
        if Instant::now() >= deadline {
            return Err(Error::Deadline);
        }
        if response.len() > budgets.max_response_bytes {
            return Err(Error::ResponseLimit);
        }
        if response.len() > allowance {
            return Err(Error::TotalLimit);
        }
        bytes += response.len();
        let raw = std::str::from_utf8(&response).map_err(|_| Error::InvalidUtf8)?;
        let page = parse_unique(raw).map_err(|_| Error::InvalidJson)?;
        if reflected(raw, &page, secret) {
            return Err(Error::CredentialReflection);
        }
        if !validators()[1].is_valid(&page) || page["world"] != world {
            return Err(Error::InvalidPage);
        }
        let originals: RawPage = serde_json::from_str(raw).map_err(|_| Error::InvalidPage)?;
        for original in originals.events {
            if records.len() == budgets.max_events {
                return Err(Error::EventLimit);
            }
            let raw_event = original.get();
            if raw_event.len() > budgets.max_record_bytes {
                return Err(Error::RecordLimit);
            }
            let event = validate_event(raw_event, world).map_err(|_| Error::InvalidEvent)?;
            let id = event["id"].as_str().ok_or(Error::InvalidEvent)?;
            if !ids.insert(id.to_owned()) {
                return Err(Error::EventConflict);
            }
            let sequence = integer(&event["sequence"]).ok_or(Error::InvalidEvent)?;
            if previous.is_some_and(|previous| sequence <= previous) {
                return Err(Error::SequenceConflict);
            }
            previous = Some(sequence);
            if event["kind"] == "artifact.recorded" {
                let artifact = &event["body"]["artifact_revision"];
                let identity = (
                    artifact["from"]
                        .as_str()
                        .ok_or(Error::InvalidEvent)?
                        .to_owned(),
                    artifact["artifact_id"]
                        .as_str()
                        .ok_or(Error::InvalidEvent)?
                        .to_owned(),
                    integer(&artifact["revision"]).ok_or(Error::InvalidEvent)?,
                );
                if !revisions.insert(identity) {
                    return Err(Error::RevisionConflict);
                }
            }
            records.push(raw_event.to_owned());
        }
        let next = page["next_cursor"].as_str().ok_or(Error::InvalidPage)?;
        let reached_end = page["has_more"] == false;
        if Instant::now() >= deadline {
            return Err(Error::Deadline);
        }
        if reached_end || traversal == Traversal::FirstPage {
            return Ok(ReadResult {
                report: Report {
                    pages: page_count,
                    events: records.len(),
                    response_bytes: bytes,
                    reached_end,
                    scope: "current_caller_view",
                    copying_permission: "not_granted",
                },
                snapshot: Snapshot {
                    world: world.to_owned(),
                    records,
                },
            });
        }
        if !cursors.insert(next.to_owned()) {
            return Err(Error::CursorConflict);
        }
        cursor = Some(next.to_owned());
    }
    Err(Error::PageLimit)
}

#[cfg(test)]
mod tests;
