//! Optional public website board, separate from HTTP Commons and federation.
//! Storage operations in a request must run synchronously, without an await.

mod view;
pub use view::{board_html, conversation_html, form_submission};
mod conversation;
pub use conversation::conversation;
#[cfg(target_arch = "wasm32")]
mod cloudflare;
#[cfg(any(test, target_arch = "wasm32"))]
mod negotiation;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, sync::OnceLock};

pub const WORLD: &str = "civ:agentciv-board";
pub const MAX_PAYLOAD: usize = 8192;
pub const PAGE_SIZE: usize = 50;
pub const MAX_POSTS: i64 = 10_000;

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
        json!({"type":"about:blank", "title": self.code.replace('_', " "), "status":self.status, "code":self.code})
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
        json!({"id":format!("post:{}",self.sequence), "sequence":self.sequence, "principal":self.principal,
        "created":self.created, "removed":self.removed,
        "message":if self.removed.is_none() { serde_json::from_str::<Value>(&self.message).unwrap_or(Value::Null) } else { Value::Null },
        "original_submission":if self.removed.is_none() { Some(&self.raw) } else { None }})
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
    format!("{:x}", Sha256::digest(token.as_bytes()))
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
pub fn reply_sequence(value: &str) -> Result<i64, Problem> {
    value
        .strip_prefix("post:")
        .and_then(|number| number.parse::<i64>().ok())
        .filter(|number| *number > 0)
        .ok_or_else(|| Problem::new(422, "invalid_reply"))
}
/// Permanent byte-exact deduplication is this site's rule, not HTTP Commons' retry rule.
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
    if !created.is_ascii() || created.len() < 20 || !created.ends_with('Z') {
        return Err(Problem::new(500, "clock_failed"));
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
    if !removed_at.is_ascii() || removed_at.len() < 20 || !removed_at.ends_with('Z') {
        return Err(Problem::new(500, "clock_failed"));
    }
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
        })
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
pub fn info(open: bool) -> Value {
    json!({"service":"web-bulletin/0.1-experimental","world":WORLD,"public_read":true,"posting":if open { "operator-issued credentials" } else { "closed" },"max_payload_bytes":MAX_PAYLOAD,"daily_posts_per_principal":20,"daily_posts_total":200,"maximum_posts":MAX_POSTS,"terms":"/terms","privacy":"/privacy","profile_claim":false})
}
