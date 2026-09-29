//! SQLite storage for one local world. Writes use a single `BEGIN IMMEDIATE` transaction.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

use crate::Visibility;

const PAGE_LIMIT: i64 = 100;

#[derive(Debug)]
pub enum StoreError {
    Storage,
}

#[derive(Debug)]
pub enum SubmitError {
    Conflict,
    Storage,
}

#[derive(Debug)]
pub enum ReadError {
    InvalidCursor,
    Forbidden,
    Expired,
    Storage,
}

#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
}

struct Policy {
    visibility: Visibility,
    retention_seconds: i64,
    revision: i64,
}

impl Store {
    pub fn open(
        path: &Path,
        visibility: Visibility,
        retention_seconds: i64,
        grants: &[(String, bool, bool)],
    ) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|_| StoreError::Storage)?;
        }
        let conn = Connection::open(path).map_err(|_| StoreError::Storage)?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(|_| StoreError::Storage)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=FULL;
             PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS meta (
               key TEXT PRIMARY KEY,
               value TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS events (
               sequence INTEGER PRIMARY KEY,
               id TEXT NOT NULL UNIQUE,
               actor TEXT NOT NULL,
               event_json TEXT NOT NULL,
               message_json TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS retries (
               principal TEXT NOT NULL,
               message_id TEXT NOT NULL,
               request_bytes BLOB NOT NULL,
               receipt_json TEXT NOT NULL,
               created_unix INTEGER NOT NULL,
               PRIMARY KEY (principal, message_id)
             );
             CREATE TABLE IF NOT EXISTS cursors (
               token TEXT PRIMARY KEY,
               principal TEXT NOT NULL,
               last_sequence INTEGER NOT NULL,
               revision INTEGER NOT NULL
             );",
        )
        .map_err(|_| StoreError::Storage)?;
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .map_err(|_| StoreError::Storage)?;
        let synchronous: i64 = conn
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .map_err(|_| StoreError::Storage)?;
        if !mode.eq_ignore_ascii_case("wal") || synchronous != 2 {
            return Err(StoreError::Storage);
        }
        let store = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        store.sync_policy(visibility, retention_seconds, grants)?;
        Ok(store)
    }

    pub fn submit(
        &self,
        principal: &str,
        world_id: &str,
        bytes: &[u8],
    ) -> Result<Value, SubmitError> {
        self.with_write(|conn| submit_tx(conn, principal, world_id, bytes, None))
    }

    #[cfg(test)]
    pub fn submit_at(
        &self,
        principal: &str,
        world_id: &str,
        bytes: &[u8],
        now: i64,
    ) -> Result<Value, SubmitError> {
        self.with_write(|conn| submit_tx(conn, principal, world_id, bytes, Some(now)))
    }

    /// Insert a row and roll it back. Used to prove a failed commit is not reported as recorded.
    #[cfg(test)]
    pub fn abandon_write(
        &self,
        principal: &str,
        world_id: &str,
        bytes: &[u8],
    ) -> Result<(), StoreError> {
        self.with_write(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE")
                .map_err(|_| StoreError::Storage)?;
            let inserted = insert_event(conn, principal, world_id, bytes, 0);
            conn.execute_batch("ROLLBACK")
                .map_err(|_| StoreError::Storage)?;
            inserted.map(|_| ()).map_err(|_| StoreError::Storage)
        })
        .map_err(|_| StoreError::Storage)
    }

    #[cfg(test)]
    pub fn event_count(&self) -> Result<i64, StoreError> {
        self.with_read(|conn| {
            conn.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
                .map_err(|_| StoreError::Storage)
        })
    }

    pub fn read_page(
        &self,
        principal: &str,
        world_id: &str,
        after: Option<&str>,
        can_read: bool,
    ) -> Result<Value, ReadError> {
        if !can_read {
            return Err(ReadError::Forbidden);
        }
        self.with_write(|conn| read_page_tx(conn, principal, world_id, after))
            .map_err(|error| match error {
                ReadError::Storage => ReadError::Storage,
                other => other,
            })
    }

    fn sync_policy(
        &self,
        visibility: Visibility,
        retention_seconds: i64,
        grants: &[(String, bool, bool)],
    ) -> Result<(), StoreError> {
        let fingerprint = fingerprint(visibility, retention_seconds, grants);
        self.with_write(|conn| {
            let stored = meta(conn, "policy_fingerprint")?;
            let revision = meta(conn, "policy_revision")?
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(0);
            let revision = if stored.as_deref() == Some(fingerprint.as_str()) {
                revision.max(1)
            } else {
                revision + 1
            };
            put_meta(conn, "policy_fingerprint", &fingerprint)?;
            put_meta(conn, "policy_revision", &revision.to_string())?;
            put_meta(conn, "visibility", visibility.as_str())?;
            put_meta(conn, "retention_seconds", &retention_seconds.to_string())?;
            Ok(())
        })
    }

    #[cfg(test)]
    fn with_read<T>(
        &self,
        body: impl FnOnce(&Connection) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        let conn = self.conn.lock().map_err(|_| StoreError::Storage)?;
        body(&conn)
    }

    fn with_write<T, E>(&self, body: impl FnOnce(&mut Connection) -> Result<T, E>) -> Result<T, E>
    where
        E: From<StoreError>,
    {
        let mut conn = self.conn.lock().map_err(|_| StoreError::Storage)?;
        body(&mut conn)
    }
}

impl From<StoreError> for SubmitError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::Storage => Self::Storage,
        }
    }
}

impl From<StoreError> for ReadError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::Storage => Self::Storage,
        }
    }
}

fn submit_tx(
    conn: &mut Connection,
    principal: &str,
    world_id: &str,
    bytes: &[u8],
    now_override: Option<i64>,
) -> Result<Value, SubmitError> {
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|_| SubmitError::Storage)?;
    let outcome = (|| {
        let now = match now_override {
            Some(now) => now,
            None => conn
                .query_row("SELECT unixepoch('now')", [], |row| row.get(0))
                .map_err(|_| SubmitError::Storage)?,
        };
        let retention = policy(conn)?.retention_seconds;
        let message: Value = serde_json::from_slice(bytes).map_err(|_| SubmitError::Storage)?;
        let message_id = message["id"]
            .as_str()
            .ok_or(SubmitError::Storage)?
            .to_owned();
        if let Some(saved) = retry_row(conn, principal, &message_id)? {
            if saved.created.saturating_add(retention) > now {
                if saved.bytes == bytes {
                    return Ok(saved.receipt);
                }
                return Err(SubmitError::Conflict);
            }
            conn.execute(
                "DELETE FROM retries WHERE principal = ?1 AND message_id = ?2",
                params![principal, message_id],
            )
            .map_err(|_| SubmitError::Storage)?;
        }
        let (receipt, _) = insert_event(conn, principal, world_id, bytes, now)?;
        conn.execute(
            "INSERT INTO retries (principal, message_id, request_bytes, receipt_json, created_unix)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![principal, message_id, bytes, receipt.to_string(), now],
        )
        .map_err(|_| SubmitError::Storage)?;
        Ok(receipt)
    })();
    match outcome {
        Ok(receipt) => {
            conn.execute_batch("COMMIT")
                .map_err(|_| SubmitError::Storage)?;
            Ok(receipt)
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn insert_event(
    conn: &Connection,
    principal: &str,
    world_id: &str,
    bytes: &[u8],
    now: i64,
) -> Result<(Value, i64), StoreError> {
    let message: Value = serde_json::from_slice(bytes).map_err(|_| StoreError::Storage)?;
    let sequence: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM events",
            [],
            |row| row.get(0),
        )
        .map_err(|_| StoreError::Storage)?;
    let id: String = conn
        .query_row("SELECT 'event:' || lower(hex(randomblob(16)))", [], |row| {
            row.get(0)
        })
        .map_err(|_| StoreError::Storage)?;
    let timestamp = unix_to_rfc3339(now).ok_or(StoreError::Storage)?;
    let event = json!({
        "protocol_version": "0.1-draft",
        "type": "event",
        "id": id,
        "world": world_id,
        "sequence": sequence,
        "timestamp": timestamp,
        "kind": "message.recorded",
        "actor": principal,
        "body": {"message": message}
    });
    conn.execute(
        "INSERT INTO events (sequence, id, actor, event_json, message_json) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![sequence, id, principal, event.to_string(), message.to_string()],
    )
    .map_err(|_| StoreError::Storage)?;
    let receipt = json!({
        "protocol_version": "0.1-draft",
        "type": "receipt",
        "world": world_id,
        "record_id": message["id"],
        "event_id": id,
        "sequence": sequence,
        "status": "recorded"
    });
    Ok((receipt, sequence))
}

struct SavedRetry {
    created: i64,
    bytes: Vec<u8>,
    receipt: Value,
}

fn retry_row(
    conn: &Connection,
    principal: &str,
    message_id: &str,
) -> Result<Option<SavedRetry>, SubmitError> {
    let row = conn
        .query_row(
            "SELECT created_unix, request_bytes, receipt_json FROM retries
             WHERE principal = ?1 AND message_id = ?2",
            params![principal, message_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|_| SubmitError::Storage)?;
    match row {
        Some((created, bytes, receipt)) => {
            let receipt = serde_json::from_str(&receipt).map_err(|_| SubmitError::Storage)?;
            Ok(Some(SavedRetry {
                created,
                bytes,
                receipt,
            }))
        }
        None => Ok(None),
    }
}

fn read_page_tx(
    conn: &mut Connection,
    principal: &str,
    world_id: &str,
    after: Option<&str>,
) -> Result<Value, ReadError> {
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(|_| ReadError::Storage)?;
    let outcome = (|| {
        let current = policy(conn)?;
        let (start, token_principal) = match after {
            None => (0, None),
            Some(token) => {
                let row = conn
                    .query_row(
                        "SELECT principal, last_sequence, revision FROM cursors WHERE token = ?1",
                        params![token],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, i64>(1)?,
                                row.get::<_, i64>(2)?,
                            ))
                        },
                    )
                    .optional()
                    .map_err(|_| ReadError::Storage)?;
                let Some((owner, last, revision)) = row else {
                    return Err(ReadError::InvalidCursor);
                };
                if owner != principal {
                    return Err(ReadError::Forbidden);
                }
                if revision != current.revision {
                    return Err(ReadError::Expired);
                }
                (last, Some(owner))
            }
        };
        let _ = token_principal;
        let mut rows = conn
            .prepare(
                "SELECT sequence, actor, event_json, message_json FROM events
                 WHERE sequence > ?1 ORDER BY sequence ASC",
            )
            .map_err(|_| ReadError::Storage)?;
        let scanned = rows
            .query_map(params![start], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|_| ReadError::Storage)?;
        let mut visible = Vec::new();
        let mut scanned_through = start;
        for row in scanned {
            let (sequence, actor, event_json, message_json) =
                row.map_err(|_| ReadError::Storage)?;
            scanned_through = sequence;
            if visible_to(current.visibility, principal, &actor, &message_json) {
                visible.push(event_json);
            }
            if visible.len() == usize::try_from(PAGE_LIMIT).unwrap_or(100) + 1 {
                break;
            }
        }
        drop(rows);
        let has_more = visible.len() > usize::try_from(PAGE_LIMIT).unwrap_or(100);
        if has_more {
            visible.pop();
        }
        let last_sequence = if has_more {
            serde_json::from_str::<Value>(visible.last().ok_or(ReadError::Storage)?)
                .map_err(|_| ReadError::Storage)?["sequence"]
                .as_i64()
                .ok_or(ReadError::Storage)?
        } else {
            scanned_through
        };
        let token = new_token(conn)?;
        conn.execute(
            "INSERT INTO cursors (token, principal, last_sequence, revision) VALUES (?1, ?2, ?3, ?4)",
            params![token, principal, last_sequence, current.revision],
        )
        .map_err(|_| ReadError::Storage)?;
        let events = visible
            .iter()
            .map(|event| serde_json::from_str(event))
            .collect::<Result<Vec<Value>, _>>()
            .map_err(|_| ReadError::Storage)?;
        Ok(json!({
            "protocol_version": "0.1-draft",
            "type": "event_page",
            "world": world_id,
            "events": events,
            "next_cursor": token,
            "has_more": has_more
        }))
    })();
    match outcome {
        Ok(page) => {
            conn.execute_batch("COMMIT")
                .map_err(|_| ReadError::Storage)?;
            Ok(page)
        }
        Err(error) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn visible_to(visibility: Visibility, principal: &str, actor: &str, message_json: &str) -> bool {
    match visibility {
        Visibility::Members => true,
        Visibility::SenderOnly => actor == principal,
        Visibility::Addressed => {
            if actor == principal {
                return true;
            }
            serde_json::from_str::<Value>(message_json)
                .ok()
                .and_then(|message| message.get("to").cloned())
                .and_then(|to| to.as_array().cloned())
                .is_some_and(|to| to.iter().any(|item| item.as_str() == Some(principal)))
        }
    }
}

fn policy(conn: &Connection) -> Result<Policy, StoreError> {
    let visibility = Visibility::parse(&meta(conn, "visibility")?.ok_or(StoreError::Storage)?)
        .ok_or(StoreError::Storage)?;
    let retention_seconds = meta(conn, "retention_seconds")?
        .ok_or(StoreError::Storage)?
        .parse()
        .map_err(|_| StoreError::Storage)?;
    let revision = meta(conn, "policy_revision")?
        .ok_or(StoreError::Storage)?
        .parse()
        .map_err(|_| StoreError::Storage)?;
    Ok(Policy {
        visibility,
        retention_seconds,
        revision,
    })
}

fn fingerprint(
    visibility: Visibility,
    retention_seconds: i64,
    grants: &[(String, bool, bool)],
) -> String {
    let mut grants = grants.to_vec();
    grants.sort();
    let grants = grants
        .iter()
        .map(|(principal, read, write)| format!("{principal} {read} {write}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{}\n{retention_seconds}\n{grants}", visibility.as_str())
}

fn meta(conn: &Connection, key: &str) -> Result<Option<String>, StoreError> {
    conn.query_row(
        "SELECT value FROM meta WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
    .map_err(|_| StoreError::Storage)
}

fn put_meta(conn: &Connection, key: &str, value: &str) -> Result<(), StoreError> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map(|_| ())
    .map_err(|_| StoreError::Storage)
}

fn new_token(conn: &Connection) -> Result<String, ReadError> {
    conn.query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))
        .map_err(|_| ReadError::Storage)
}

fn unix_to_rfc3339(unix: i64) -> Option<String> {
    if unix < 0 {
        return None;
    }
    let day = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    let hour = secs / 3600;
    let minute = (secs % 3600) / 60;
    let second = secs % 60;
    let (year, month, day) = civil_from_days(day)?;
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
    ))
}

fn civil_from_days(days: i64) -> Option<(i64, u32, u32)> {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some((year, u32::try_from(month).ok()?, u32::try_from(day).ok()?))
}
