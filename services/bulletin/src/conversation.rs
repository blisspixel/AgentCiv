//! Bounded read-only HTML conversation projection over existing public posts.

use crate::{Binding, Database, PAGE_SIZE, Problem, json, post, reply_sequence, rows};
use serde_json::Value;

// The parsed message is already validated on publication. Normalize numeric
// spellings accepted by reply_sequence, including post:01 and post:+1. Scrubbing
// a removed message also clears this derived index entry; no reply edge is kept.
const REPLY_KEY: &str = "CASE WHEN removed IS NULL AND json_valid(message) THEN CAST(substr(json_extract(message, '$.body.reply_to'), 6) AS INTEGER) END";

pub(super) fn initialize_index(db: &impl Database) -> Result<(), Problem> {
    db.query(
        &format!("CREATE INDEX IF NOT EXISTS post_reply ON posts({REPLY_KEY}, sequence)"),
        vec![],
    )?;
    Ok(())
}

/// A selected post and at most fifty active direct replies, in host sequence
/// order. This is not a complete tree, snapshot, new JSON endpoint, or history
/// contract. The existing removed row remains selectable by its known URL.
pub fn conversation(
    db: &impl Database,
    sequence: i64,
    after: Option<i64>,
    before: Option<i64>,
) -> Result<Value, Problem> {
    let selected = post(db, sequence)?;
    if after.is_some_and(|value| value < 0)
        || before.is_some_and(|value| value <= 0)
        || (after.is_some() && before.is_some())
    {
        return Err(Problem::new(400, "invalid_page"));
    }
    let ascending = after.is_some();
    let mut bindings = vec![Binding::Integer(sequence)];
    let condition = if let Some(after) = after {
        bindings.push(Binding::Integer(after));
        " AND sequence > ?"
    } else if let Some(before) = before {
        bindings.push(Binding::Integer(before));
        " AND sequence < ?"
    } else {
        ""
    };
    let order = if ascending { "ASC" } else { "DESC" };
    let mut replies = rows(
        db,
        &format!(
            "SELECT * FROM posts WHERE {REPLY_KEY} = ?{condition} ORDER BY sequence {order} LIMIT 51"
        ),
        bindings,
    )?;
    for reply in &replies {
        let message: Value = serde_json::from_str(&reply.message)
            .map_err(|_| Problem::new(500, "storage_failed"))?;
        if reply.removed.is_some()
            || message["body"]["reply_to"]
                .as_str()
                .and_then(|value| reply_sequence(value).ok())
                != Some(sequence)
        {
            return Err(Problem::new(500, "storage_failed"));
        }
    }
    let has_more = replies.len() > PAGE_SIZE;
    replies.truncate(PAGE_SIZE);
    if !ascending {
        replies.reverse();
    }
    let next_after = replies
        .last()
        .map_or(after.unwrap_or(0), |reply| reply.sequence);
    let next_before = replies
        .first()
        .map_or(before.unwrap_or(0), |reply| reply.sequence);
    Ok(json!({"post":selected.public(), "replies":{
        "posts":replies.iter().map(crate::Post::public).collect::<Vec<_>>(),
        "has_more":has_more,"ascending":ascending,
        "next_after":next_after,"next_before":next_before}}))
}
