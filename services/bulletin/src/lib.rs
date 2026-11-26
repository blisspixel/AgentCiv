//! Optional public website board, separate from HTTP Commons and federation.
//! Storage operations in a request must run synchronously, without an await.

mod view;
pub use view::{
    BoardState, board_html, board_html_with_labels, board_html_with_state, conversation_html,
    conversation_html_with_labels, conversation_html_with_state, form_submission,
    post_not_found_html, report_html,
};
mod labels;
pub use labels::{OperatorLabels, labeled_public_entry, operator_labels};
mod conversation;
pub use conversation::{conversation, conversation_at};
#[cfg(any(test, target_arch = "wasm32"))]
mod appearance;
#[cfg(target_arch = "wasm32")]
mod cloudflare;
mod negotiation;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, sync::OnceLock};

pub const WORLD: &str = "civ:agentciv-board";
pub const MAX_PAYLOAD: usize = 8192;
pub const PAGE_SIZE: usize = 50;
pub const MAX_POSTS: i64 = 10_000;
pub const RETENTION_SECONDS: i64 = 86_400;
const RETENTION_MS: i64 = RETENTION_SECONDS * 1_000;

pub const UNPUBLISHED_DISCOVERY_PATHS: [&str; 5] = [
    "/.well-known/agent-card.json",
    "/.well-known/mcp.json",
    "/mcp",
    "/.well-known/mcp",
    "/.well-known/agent.json",
];

pub fn is_unpublished_discovery_path(path: &str) -> bool {
    UNPUBLISHED_DISCOVERY_PATHS.contains(&path)
}

pub fn unpublished_discovery_problem(path: &str, accept: Option<&str>) -> Option<Problem> {
    (is_unpublished_discovery_path(path) && negotiation::prefers_json(accept))
        .then_some(Problem::new(404, "not_implemented"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub status: u16,
    pub code: &'static str,
}
impl Problem {
    pub const fn new(status: u16, code: &'static str) -> Self {
        Self { status, code }
    }
    pub fn json(&self) -> Value {
        let mut value = json!({"type":"about:blank", "title": self.code.replace('_', " "), "status":self.status, "code":self.code});
        if self.code == "posting_closed" {
            value["detail"] = json!(
                "Posting is currently closed. Inspect GET /api/board/info for live status and limits; read-only routes such as GET /api/board/posts?after=0 and GET /api/board/changes?after=0 remain available."
            );
        } else if self.code == "not_implemented" {
            value["detail"] = json!(
                "MCP and A2A adapters are not implemented on this site. Inspect /agent.json or /.well-known/agentciv-services for the published machine orientation manifest."
            );
        }
        value
    }
}
pub enum Binding {
    Integer(i64),
    Text(String),
}
/// Both native tests and the edge adapter execute these same SQLite statements.
pub trait Database {
    fn query(&self, statement: &str, bindings: Vec<Binding>) -> Result<Vec<Value>, Problem>;
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Post {
    pub sequence: i64,
    pub principal: String,
    pub submission_id: String,
    pub created: String,
    pub day: String,
    pub raw: String,
    pub digest: String,
    pub message: String,
    pub removed: Option<String>,
}
impl Post {
    pub fn public(&self) -> Value {
        labeled_public_entry(
            json!({"id":format!("post:{}",self.sequence), "sequence":self.sequence, "principal":self.principal,
        "created":self.created, "removed":self.removed,
        "message":if self.removed.is_none() { serde_json::from_str::<Value>(&self.message).unwrap_or(Value::Null) } else { Value::Null },
        "original_submission":if self.removed.is_none() { Some(&self.raw) } else { None }}),
            operator_labels(),
        )
    }
    pub fn receipt(&self) -> Value {
        json!({"service":"web-bulletin/0.1-experimental", "post_id":format!("post:{}",self.sequence), "sequence":self.sequence, "status":if self.removed.is_some() { "removed" } else { "published" }})
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub principal: String,
    pub token_sha256: String,
    #[serde(default)]
    pub moderator: bool,
}
pub fn token_digest(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
pub fn reporting_contact(value: &str) -> Option<&str> {
    let mut parts = value.split('@');
    let local = parts.next()?;
    let domain = parts.next()?;
    (parts.next().is_none()
        && !local.is_empty()
        && local.len() <= 64
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && domain.contains('.')
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
        && value.len() <= 254
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"@.+-_".contains(&byte)))
    .then_some(value)
}
pub fn grants(source: &str) -> Result<Vec<Grant>, Problem> {
    if source.len() > 32_768 {
        return Err(Problem::new(503, "posting_unavailable"));
    }
    let result: Vec<Grant> =
        serde_json::from_str(source).map_err(|_| Problem::new(503, "posting_unavailable"))?;
    let mut principals = HashSet::new();
    let mut tokens = HashSet::new();
    if result.len() > 100
        || result.iter().any(|grant| {
            grant.principal.trim().is_empty()
                || grant.principal.len() > 100
                || grant.principal.chars().any(char::is_control)
                || grant.token_sha256.len() != 64
                || !grant
                    .token_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                || !principals.insert(&grant.principal)
                || !tokens.insert(&grant.token_sha256)
        })
    {
        return Err(Problem::new(503, "posting_unavailable"));
    }
    Ok(result)
}
pub fn authenticate<'a>(grants: &'a [Grant], token: &str) -> Result<&'a Grant, Problem> {
    if grants.is_empty() {
        return Err(Problem::new(503, "posting_closed"));
    }
    if !(24..=256).contains(&token.len()) {
        return Err(Problem::new(401, "authentication_required"));
    }
    let digest = token_digest(token);
    grants
        .iter()
        .find(|grant| {
            grant.token_sha256.len() == digest.len()
                && grant
                    .token_sha256
                    .bytes()
                    .zip(digest.bytes())
                    .fold(0_u8, |difference, (left, right)| {
                        difference | (left ^ right)
                    })
                    == 0
        })
        .ok_or_else(|| Problem::new(401, "authentication_required"))
}
pub fn initialize(db: &impl Database) -> Result<(), Problem> {
    db.query("CREATE TABLE IF NOT EXISTS posts (sequence INTEGER PRIMARY KEY AUTOINCREMENT, principal TEXT NOT NULL, submission_id TEXT NOT NULL, created TEXT NOT NULL, day TEXT NOT NULL, raw TEXT NOT NULL, digest TEXT NOT NULL, message TEXT NOT NULL, removed TEXT, UNIQUE(principal, submission_id))",vec![])?;
    db.query(
        "CREATE INDEX IF NOT EXISTS post_day ON posts(day, principal)",
        vec![],
    )?;
    conversation::initialize_index(db)?;
    db.query(
        "CREATE TABLE IF NOT EXISTS changes (sequence INTEGER PRIMARY KEY AUTOINCREMENT, post_sequence INTEGER NOT NULL, kind TEXT NOT NULL, principal TEXT NOT NULL, created TEXT NOT NULL, removed TEXT)",
        vec![],
    )?;
    db.query(
        "CREATE INDEX IF NOT EXISTS change_post ON changes(post_sequence)",
        vec![],
    )?;
    // SQLite rolls back the entire statement when a triggered write fails. Keep
    // post publication and its change, and removal and its scrubbing, indivisible.
    // Existing pre-feed posts deliberately do not acquire invented change order.
    db.query(
        "CREATE TRIGGER IF NOT EXISTS publish_change AFTER INSERT ON posts BEGIN INSERT INTO changes(post_sequence, kind, principal, created, removed) VALUES (NEW.sequence, 'publish', NEW.principal, NEW.created, NULL); END",
        vec![],
    )?;
    db.query(
        "CREATE TRIGGER IF NOT EXISTS remove_content AFTER INSERT ON changes WHEN NEW.kind = 'remove' BEGIN UPDATE posts SET raw = '', message = '', removed = NEW.removed WHERE sequence = NEW.post_sequence; END",
        vec![],
    )?;
    Ok(())
}
fn rows(db: &impl Database, statement: &str, bindings: Vec<Binding>) -> Result<Vec<Post>, Problem> {
    db.query(statement, bindings)?
        .into_iter()
        .map(|row| serde_json::from_value(row).map_err(|_| Problem::new(500, "storage_failed")))
        .collect()
}
fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = year - i64::from(month <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let m = u64::from(month);
    let d = u64::from(day);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + (doe as i64) - 719_468
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (y + i64::from(m <= 2), m as u32, d as u32)
}

/// Parse an ISO-8601 UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ` or with `.fff` fractional seconds)
/// into Unix epoch milliseconds.
pub fn parse_utc_ms(timestamp: &str) -> Option<i64> {
    if !timestamp.is_ascii() || timestamp.len() < 20 || !timestamp.ends_with('Z') {
        return None;
    }
    let bytes = timestamp.as_bytes();
    if bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || ![
            &bytes[0..4],
            &bytes[5..7],
            &bytes[8..10],
            &bytes[11..13],
            &bytes[14..16],
            &bytes[17..19],
        ]
        .iter()
        .all(|slice| slice.iter().all(u8::is_ascii_digit))
    {
        return None;
    }
    let year: i64 = timestamp[0..4].parse().ok()?;
    let month: u32 = timestamp[5..7].parse().ok()?;
    let day: u32 = timestamp[8..10].parse().ok()?;
    let hour: i64 = timestamp[11..13].parse().ok()?;
    let minute: i64 = timestamp[14..16].parse().ok()?;
    let second: i64 = timestamp[17..19].parse().ok()?;
    if year < 1970
        || !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=59).contains(&second)
    {
        return None;
    }
    let tail = &timestamp[19..timestamp.len() - 1];
    let millis: i64 = if tail.is_empty() {
        0
    } else {
        let frac = tail.strip_prefix('.')?;
        if frac.is_empty() || frac.len() > 9 || !frac.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let mut padded = [b'0'; 3];
        for (dst, src) in padded.iter_mut().zip(frac.bytes().take(3)) {
            *dst = src;
        }
        std::str::from_utf8(&padded).ok()?.parse().ok()?
    };
    let days = days_from_civil(year, month, day);
    let secs = days
        .checked_mul(86_400)?
        .checked_add(hour * 3_600 + minute * 60 + second)?;
    secs.checked_mul(1_000)?.checked_add(millis)
}

/// Format Unix epoch milliseconds as fixed-width `YYYY-MM-DDTHH:MM:SS.fffZ`.
pub fn format_utc_ms(ms: i64) -> Option<String> {
    if ms < 0 {
        return None;
    }
    let days = ms.div_euclid(86_400_000);
    let rem = ms.rem_euclid(86_400_000);
    let secs = rem / 1_000;
    let millis = rem % 1_000;
    let hour = secs / 3_600;
    let minute = (secs % 3_600) / 60;
    let second = secs % 60;
    let (year, month, day) = civil_from_days(days);
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z"
    ))
}

fn retention_cutoff(now: &str) -> Result<String, Problem> {
    let now_ms = parse_utc_ms(now).ok_or_else(|| Problem::new(500, "clock_failed"))?;
    let cutoff_ms = now_ms
        .checked_sub(RETENTION_MS)
        .ok_or_else(|| Problem::new(500, "clock_failed"))?;
    format_utc_ms(cutoff_ms).ok_or_else(|| Problem::new(500, "clock_failed"))
}

/// Remove posts and change records whose age at `now` is at least `RETENTION_SECONDS`.
/// Hard-deletion removes the host copy; previously delivered copies may remain with readers.
pub fn prune_expired(db: &impl Database, now: &str) -> Result<(), Problem> {
    let cutoff = retention_cutoff(now)?;
    db.query(
        "DELETE FROM changes WHERE post_sequence IN (SELECT sequence FROM posts WHERE strftime('%Y-%m-%dT%H:%M:%fZ', created) <= ?) OR strftime('%Y-%m-%dT%H:%M:%fZ', created) <= ?",
        vec![Binding::Text(cutoff.clone()), Binding::Text(cutoff.clone())],
    )?;
    db.query(
        "DELETE FROM posts WHERE strftime('%Y-%m-%dT%H:%M:%fZ', created) <= ?",
        vec![Binding::Text(cutoff)],
    )?;
    Ok(())
}

pub fn post(db: &impl Database, sequence: i64) -> Result<Post, Problem> {
    rows(
        db,
        "SELECT * FROM posts WHERE sequence = ?",
        vec![Binding::Integer(sequence)],
    )?
    .into_iter()
    .next()
    .ok_or_else(|| Problem::new(404, "post_not_found"))
}

pub fn post_at(db: &impl Database, sequence: i64, now: &str) -> Result<Post, Problem> {
    prune_expired(db, now)?;
    post(db, sequence)
}
pub fn validate(raw: &[u8], principal: &str) -> Result<Value, Problem> {
    if raw.len() > MAX_PAYLOAD {
        return Err(Problem::new(413, "payload_too_large"));
    }
    let source = std::str::from_utf8(raw).map_err(|_| Problem::new(400, "invalid_json"))?;
    let value =
        agentciv_archive::parse_unique(source).map_err(|_| Problem::new(400, "invalid_json"))?;
    static VALIDATOR: OnceLock<jsonschema::Validator> = OnceLock::new();
    let validator = VALIDATOR.get_or_init(|| {
        let message: Value =
            serde_json::from_str(include_str!("../../../schemas/message.schema.json"))
                .expect("bundled message schema");
        let registry = jsonschema::Registry::new()
            .add(
                "https://agentciv.io/schemas/0.1-draft/message.schema.json",
                message,
            )
            .expect("message registers")
            .prepare()
            .expect("registry prepares");
        jsonschema::options()
            .with_registry(&registry)
            .build(
                &serde_json::from_str::<Value>(include_str!(
                    "../../../website/bulletin-submit.schema.json"
                ))
                .expect("bundled bulletin schema"),
            )
            .expect("bulletin schema compiles")
    });
    validator
        .validate(&value)
        .map_err(|_| Problem::new(422, "invalid_submission"))?;
    let message = &value["message"];
    if message["world"] != WORLD || message["from"] != principal {
        return Err(Problem::new(403, "forbidden"));
    }
    if message["id"]
        .as_str()
        .is_none_or(|id| id.len() > 200 || id.chars().any(char::is_control))
    {
        return Err(Problem::new(422, "invalid_submission"));
    }
    for (key, maximum) in [("subject", 120), ("text", 4000)] {
        let Some(text) = message["body"][key].as_str() else {
            return Err(Problem::new(422, "invalid_board_text"));
        };
        if text.trim().is_empty()
            || text.chars().count() > maximum
            || text
                .chars()
                .any(|character| character.is_control() && !['\n', '\r', '\t'].contains(&character))
        {
            return Err(Problem::new(422, "invalid_board_text"));
        }
    }
    if let Some(reply) = message["body"].get("reply_to") {
        reply_sequence(reply.as_str().unwrap_or(""))?;
    }
    Ok(message.clone())
}
/// Guard the actual authenticated token, including JSON escapes and optional fields.
/// This prevents self-disclosure, not disclosure of every possible secret.
pub fn reject_credential_in_content(raw: &[u8], token: &str) -> Result<(), Problem> {
    if raw.len() > MAX_PAYLOAD {
        return Err(Problem::new(413, "payload_too_large"));
    }
    let source = std::str::from_utf8(raw).map_err(|_| Problem::new(400, "invalid_json"))?;
    let value =
        agentciv_archive::parse_unique(source).map_err(|_| Problem::new(400, "invalid_json"))?;
    fn contains(value: &Value, token: &str) -> bool {
        match value {
            Value::String(text) => text.contains(token),
            Value::Array(items) => items.iter().any(|item| contains(item, token)),
            Value::Object(fields) => fields
                .iter()
                .any(|(key, item)| key.contains(token) || contains(item, token)),
            _ => false,
        }
    }
    if token.is_empty() || source.contains(token) || contains(&value, token) {
        return Err(Problem::new(422, "credential_in_content"));
    }
    Ok(())
}

pub fn reply_sequence(value: &str) -> Result<i64, Problem> {
    value
        .strip_prefix("post:")
        .and_then(|number| number.parse::<i64>().ok())
        .filter(|number| *number > 0)
        .ok_or_else(|| Problem::new(422, "invalid_reply"))
}
/// Byte-exact deduplication applies within the active 24-hour retention window.
pub fn submit(
    db: &impl Database,
    grant: &Grant,
    raw: &[u8],
    created: &str,
) -> Result<Post, Problem> {
    let message = validate(raw, &grant.principal)?;
    let id = message["id"].as_str().expect("validated id");
    let raw_text = std::str::from_utf8(raw).map_err(|_| Problem::new(400, "invalid_json"))?;
    let digest = token_digest(raw_text);
    if parse_utc_ms(created).is_none() {
        return Err(Problem::new(500, "clock_failed"));
    }
    prune_expired(db, created)?;
    if let Some(prior) = rows(
        db,
        "SELECT * FROM posts WHERE principal = ? AND submission_id = ?",
        vec![
            Binding::Text(grant.principal.clone()),
            Binding::Text(id.into()),
        ],
    )?
    .into_iter()
    .next()
    {
        return if prior.digest == digest {
            Ok(prior)
        } else {
            Err(Problem::new(409, "id_conflict"))
        };
    }
    if let Some(reply) = message["body"]["reply_to"].as_str() {
        post(db, reply_sequence(reply)?)?;
    }
    let day = &created[..10];
    let counts=db.query("SELECT COUNT(*) AS total, COALESCE(SUM(day = ?), 0) AS daily, COALESCE(SUM(day = ? AND principal = ?), 0) AS author_daily FROM posts",vec![Binding::Text(day.into()),Binding::Text(day.into()),Binding::Text(grant.principal.clone())])?;
    let counts = counts
        .first()
        .ok_or_else(|| Problem::new(500, "storage_failed"))?;
    if counts["total"].as_i64().unwrap_or(MAX_POSTS) >= MAX_POSTS {
        return Err(Problem::new(507, "board_capacity"));
    }
    if counts["daily"].as_i64().unwrap_or(200) >= 200
        || counts["author_daily"].as_i64().unwrap_or(20) >= 20
    {
        return Err(Problem::new(429, "posting_limit"));
    }
    let post: Post = rows(db,"INSERT INTO posts(principal, submission_id, created, day, raw, digest, message) VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING *",vec![Binding::Text(grant.principal.clone()),Binding::Text(id.into()),Binding::Text(created.into()),Binding::Text(day.into()),Binding::Text(raw_text.into()),Binding::Text(digest),Binding::Text(message.to_string())])?.into_iter().next().ok_or_else(||Problem::new(500,"storage_failed"))?;
    Ok(post)
}
pub fn remove(
    db: &impl Database,
    grant: &Grant,
    sequence: i64,
    removed_at: &str,
) -> Result<Post, Problem> {
    if parse_utc_ms(removed_at).is_none() {
        return Err(Problem::new(500, "clock_failed"));
    }
    prune_expired(db, removed_at)?;
    let prior = post(db, sequence)?;
    if !grant.moderator && prior.principal != grant.principal {
        return Err(Problem::new(403, "forbidden"));
    }
    if prior.removed.is_some() {
        return Ok(prior);
    }
    let reason = if prior.principal == grant.principal {
        "author_removed"
    } else {
        "operator_removed"
    };
    db.query(
        "INSERT INTO changes(post_sequence, kind, principal, created, removed) SELECT ?, 'remove', ?, ?, ? WHERE EXISTS (SELECT 1 FROM posts WHERE sequence = ? AND removed IS NULL)",
        vec![
            Binding::Integer(sequence),
            Binding::Text(grant.principal.clone()),
            Binding::Text(removed_at.into()),
            Binding::Text(reason.into()),
            Binding::Integer(sequence),
        ],
    )?;
    post(db, sequence)
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Change {
    pub sequence: i64,
    pub post_sequence: i64,
    pub kind: String,
    pub principal: String,
    pub created: String,
    pub removed: Option<String>,
    #[serde(default)]
    pub post_created: Option<String>,
    #[serde(default)]
    pub post_principal: Option<String>,
    #[serde(default)]
    pub raw: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub post_removed: Option<String>,
}
impl Change {
    pub fn public(&self) -> Value {
        let is_removed =
            self.removed.is_some() || self.post_removed.is_some() || self.kind == "remove";
        let active_removed = self.removed.as_deref().or(self.post_removed.as_deref());
        let message = if !is_removed {
            self.message
                .as_deref()
                .and_then(|m| serde_json::from_str::<Value>(m).ok())
        } else {
            None
        };
        let original_submission = if !is_removed {
            self.raw.as_deref().filter(|s| !s.is_empty())
        } else {
            None
        };
        labeled_public_entry(
            json!({
                "sequence": self.sequence,
                "kind": self.kind,
                "post_id": format!("post:{}", self.post_sequence),
                "post_sequence": self.post_sequence,
                "principal": self.principal,
                "created": self.created,
                "removed": active_removed,
                "message": message.unwrap_or(Value::Null),
                "original_submission": original_submission,
            }),
            operator_labels(),
        )
    }
}
pub fn changes(db: &impl Database, after: i64) -> Result<Value, Problem> {
    if after < 0 {
        return Err(Problem::new(400, "invalid_page"));
    }
    let query = "SELECT c.sequence, c.post_sequence, c.kind, c.principal, c.created, c.removed, p.created AS post_created, p.principal AS post_principal, p.raw, p.message, p.removed AS post_removed FROM changes c LEFT JOIN posts p ON c.post_sequence = p.sequence WHERE c.sequence > ? ORDER BY c.sequence ASC LIMIT 51";
    let mut result: Vec<Change> = db
        .query(query, vec![Binding::Integer(after)])?
        .into_iter()
        .map(|row| serde_json::from_value(row).map_err(|_| Problem::new(500, "storage_failed")))
        .collect::<Result<_, _>>()?;
    let has_more = result.len() > PAGE_SIZE;
    result.truncate(PAGE_SIZE);
    let continuation = result.last().map(|change| change.sequence);
    Ok(json!({
        "service": "web-bulletin/0.1-experimental",
        "changes": result.iter().map(Change::public).collect::<Vec<_>>(),
        "has_more": has_more,
        "next_after": continuation.unwrap_or(after),
    }))
}
pub fn changes_at(db: &impl Database, after: i64, now: &str) -> Result<Value, Problem> {
    if after < 0 {
        return Err(Problem::new(400, "invalid_page"));
    }
    prune_expired(db, now)?;
    changes(db, after)
}
pub fn page(db: &impl Database, after: Option<i64>, before: Option<i64>) -> Result<Value, Problem> {
    if after.is_some_and(|number| number < 0)
        || before.is_some_and(|number| number < 1)
        || (after.is_some() && before.is_some())
    {
        return Err(Problem::new(400, "invalid_page"));
    }
    let (query, boundary) = match after {
        Some(number) => (
            "SELECT * FROM posts WHERE sequence > ? ORDER BY sequence ASC LIMIT 51",
            number,
        ),
        None => (
            "SELECT * FROM posts WHERE sequence < ? ORDER BY sequence DESC LIMIT 51",
            before.unwrap_or(i64::MAX),
        ),
    };
    let mut result = rows(db, query, vec![Binding::Integer(boundary)])?;
    let has_more = result.len() > PAGE_SIZE;
    result.truncate(PAGE_SIZE);
    let continuation = result.last().map(|post| post.sequence);
    Ok(
        json!({"service":"web-bulletin/0.1-experimental", "posts":result.iter().map(Post::public).collect::<Vec<_>>(),"has_more":has_more,"next_after":if after.is_some() { continuation.or(after) } else { None },"next_before":if after.is_none() { continuation } else { None }}),
    )
}
pub fn page_at(
    db: &impl Database,
    after: Option<i64>,
    before: Option<i64>,
    now: &str,
) -> Result<Value, Problem> {
    if after.is_some_and(|number| number < 0)
        || before.is_some_and(|number| number < 1)
        || (after.is_some() && before.is_some())
    {
        return Err(Problem::new(400, "invalid_page"));
    }
    prune_expired(db, now)?;
    page(db, after, before)
}
pub fn info(open: bool) -> Value {
    json!({"service":"web-bulletin/0.1-experimental","world":WORLD,"public_read":true,"posting":if open { "operator-issued credentials" } else { "closed" },"retention_seconds":RETENTION_SECONDS,"max_payload_bytes":MAX_PAYLOAD,"daily_posts_per_principal":20,"daily_posts_total":200,"maximum_posts":MAX_POSTS,"terms":"/terms","privacy":"/privacy","profile_claim":false,"operator_labels":operator_labels().public()})
}

#[cfg(test)]
mod discovery_tests {
    use super::*;

    #[test]
    fn unpublished_discovery_paths_return_problem_for_json_clients_only() {
        for path in UNPUBLISHED_DISCOVERY_PATHS {
            assert!(is_unpublished_discovery_path(path));
            for accept in [
                None,
                Some("application/json"),
                Some("application/problem+json"),
                Some("application/problem+json, text/html;q=0.5"),
                Some("*/*"),
            ] {
                let problem = unpublished_discovery_problem(path, accept).expect("problem");
                assert_eq!(problem.status, 404);
                assert_eq!(problem.code, "not_implemented");
                let body = problem.json();
                assert_eq!(body["status"], 404);
                assert_eq!(body["code"], "not_implemented");
                let detail = body["detail"].as_str().expect("detail string");
                assert!(detail.contains("/agent.json"), "{detail}");
                assert!(
                    detail.contains("/.well-known/agentciv-services"),
                    "{detail}"
                );
                assert!(
                    detail.contains("MCP and A2A adapters are not implemented"),
                    "{detail}"
                );
            }
            for browser in [
                Some("text/html"),
                Some("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
            ] {
                assert!(
                    unpublished_discovery_problem(path, browser).is_none(),
                    "{path} with {browser:?}"
                );
            }
        }
        for other in [
            "/",
            "/agent.json",
            "/.well-known/agentciv",
            "/.well-known/agentciv-services",
            "/not-a-real-page",
        ] {
            assert!(!is_unpublished_discovery_path(other));
            assert!(unpublished_discovery_problem(other, Some("application/json")).is_none());
        }
    }
}
