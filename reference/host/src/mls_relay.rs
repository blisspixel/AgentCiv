//! Untrusted zero-plaintext Delivery Service core (`mls-relay/0.1-draft`) for Stage 2A of #209.
//!
//! This module provides a pure-Rust SQLite-backed relay store isolated in `mls_*` tables,
//! separate from `http-commons/0.1-draft`. The relay never inspects or decrypts MLS payloads:
//! it validates opaque canonical RFC 4648 base64 ciphertexts, enforces single-use KeyPackage
//! consumption, serializes group epoch commits with optimistic concurrency (`BEGIN IMMEDIATE`),
//! computes RFC 9162 Merkle checkpoints over ciphertext SHA-256 hashes, and hard-prunes rows
//! after a bounded 24-hour TTL.

use std::fmt;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

use crate::merkle;

/// Wire profile identifier for the untrusted PQ-MLS Delivery Service relay.
pub const PROFILE_ID: &str = "mls-relay/0.1-draft";

/// Default and maximum ring-buffer retention window in seconds (24 hours).
pub const DEFAULT_RETENTION_SECONDS: i64 = 86_400;

/// Maximum decoded byte length of an opaque MLS KeyPackage, Commit, Welcome, or PrivateMessage.
/// Accommodates PQ-MLS `X25519MLKEM768` + `ML-DSA-65` payloads while bounding memory usage.
pub const MAX_OPAQUE_BLOB_BYTES: usize = 65_536;

/// Maximum live unconsumed KeyPackages retained per principal.
pub const MAX_KEY_PACKAGES_PER_PRINCIPAL: usize = 32;

/// Maximum active (unexpired) groups permitted in a single relay store.
pub const MAX_GROUPS: usize = 256;

/// Maximum number of Welcome envelopes attached to a single Commit transaction.
pub const MAX_WELCOMES_PER_COMMIT: usize = 128;

/// Maximum total envelopes (`commit`, `private_message`, `welcome`) retained per group.
pub const MAX_ENVELOPES_PER_GROUP: usize = 4_096;

/// Default page limit when fetching envelopes or welcomes.
pub const DEFAULT_FETCH_LIMIT: usize = 100;

/// Upper bound on page limit when fetching envelopes or welcomes.
pub const MAX_FETCH_LIMIT: usize = 256;

/// Maximum ASCII byte length of canonical identifiers (`group_id`, `principal_id`, `kp_id`).
pub const MAX_IDENTIFIER_BYTES: usize = 128;

/// Supported PQ hybrid and classical MLS ciphersuite identifiers.
pub const SUPPORTED_CIPHERSUITES: &[&str] = &[
    "MLS_256_X25519MLKEM768_CHACHA20POLY1305_SHA512_MLDSA65",
    "MLS_256_X25519MLKEM768_AES256GCM_SHA512_MLDSA65",
    "MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519",
    "MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519",
];

const RFC9162_LEAF_PREFIX: u8 = 0x00;

/// Fail-closed runtime configuration for [`MlsRelayStore`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RelayConfig {
    /// When `false` (the default), all mutating relay operations fail closed with
    /// [`RelayError::WritesDisabled`].
    pub writes_enabled: bool,
    /// Hard TTL in seconds for stored groups, KeyPackages, and envelopes.
    /// Must be within `1..=DEFAULT_RETENTION_SECONDS`.
    pub retention_seconds: i64,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            writes_enabled: false,
            retention_seconds: DEFAULT_RETENTION_SECONDS,
        }
    }
}

/// Typed errors returned by [`MlsRelayStore`] operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RelayError {
    /// Mutating operations are disabled by the fail-closed kill-switch (HTTP 503).
    WritesDisabled,
    /// Malformed identifier, base64 payload, or query parameter (HTTP 400).
    InvalidInput(String),
    /// Decoded opaque blob exceeds [`MAX_OPAQUE_BLOB_BYTES`] (HTTP 413).
    PayloadTooLarge { size: usize, max: usize },
    /// Ciphersuite tag is not in [`SUPPORTED_CIPHERSUITES`] (HTTP 400).
    UnsupportedCiphersuite(String),
    /// Target group or resource does not exist or has expired (HTTP 404).
    NotFound(String),
    /// Resource identifier already exists (HTTP 409).
    Conflict(String),
    /// Submitted Commit or PrivateMessage epoch does not match the group's active epoch (HTTP 409).
    EpochConflict { current_epoch: u64 },
    /// Bounded per-principal KeyPackage or active-group quota exceeded (HTTP 429).
    QuotaExceeded(String),
    /// Underlying SQLite storage failure (HTTP 500).
    Storage(String),
}

impl RelayError {
    /// Suggested HTTP status code for this relay error.
    pub const fn status_code(&self) -> u16 {
        match self {
            Self::WritesDisabled => 503,
            Self::InvalidInput(_) | Self::UnsupportedCiphersuite(_) => 400,
            Self::PayloadTooLarge { .. } => 413,
            Self::NotFound(_) => 404,
            Self::Conflict(_) | Self::EpochConflict { .. } => 409,
            Self::QuotaExceeded(_) => 429,
            Self::Storage(_) => 500,
        }
    }

    /// Stable machine-readable error code.
    pub const fn error_code(&self) -> &'static str {
        match self {
            Self::WritesDisabled => "writes_disabled",
            Self::InvalidInput(_) => "invalid_input",
            Self::PayloadTooLarge { .. } => "payload_too_large",
            Self::UnsupportedCiphersuite(_) => "unsupported_ciphersuite",
            Self::NotFound(_) => "not_found",
            Self::Conflict(_) => "conflict",
            Self::EpochConflict { .. } => "epoch_conflict",
            Self::QuotaExceeded(_) => "quota_exceeded",
            Self::Storage(_) => "storage_error",
        }
    }
}

impl fmt::Display for RelayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WritesDisabled => write!(f, "relay writes are disabled"),
            Self::InvalidInput(detail) => write!(f, "invalid input: {detail}"),
            Self::PayloadTooLarge { size, max } => {
                write!(f, "opaque blob size {size} exceeds maximum {max} bytes")
            }
            Self::UnsupportedCiphersuite(suite) => {
                write!(f, "unsupported MLS ciphersuite: {suite}")
            }
            Self::NotFound(detail) => write!(f, "not found: {detail}"),
            Self::Conflict(detail) => write!(f, "conflict: {detail}"),
            Self::EpochConflict { current_epoch } => {
                write!(f, "epoch conflict: current epoch is {current_epoch}")
            }
            Self::QuotaExceeded(detail) => write!(f, "quota exceeded: {detail}"),
            Self::Storage(detail) => write!(f, "storage error: {detail}"),
        }
    }
}

impl std::error::Error for RelayError {}

/// Stored opaque MLS group metadata (no plaintext, no membership roster).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GroupRecord {
    pub group_id: String,
    pub ciphersuite: String,
    pub current_epoch: u64,
    pub created_at: i64,
    pub expires_at: i64,
}

/// Stored opaque single-use MLS KeyPackage.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct KeyPackageRecord {
    pub kp_id: String,
    pub principal_id: String,
    pub ciphersuite: String,
    pub blob_b64: String,
    pub blob_sha256: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub consumed: bool,
}

/// Kind of opaque MLS envelope stored by the Delivery Service.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvelopeKind {
    Commit,
    PrivateMessage,
    Welcome,
}

impl EnvelopeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::PrivateMessage => "private_message",
            Self::Welcome => "welcome",
        }
    }

    fn parse(raw: &str) -> Result<Self, RelayError> {
        match raw {
            "commit" => Ok(Self::Commit),
            "private_message" => Ok(Self::PrivateMessage),
            "welcome" => Ok(Self::Welcome),
            other => Err(RelayError::Storage(format!(
                "unexpected envelope kind in database: {other}"
            ))),
        }
    }
}

/// Stored opaque MLS envelope (`commit`, `private_message`, or `welcome`).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EnvelopeRecord {
    pub seq: u64,
    pub group_id: String,
    pub kind: EnvelopeKind,
    pub epoch: u64,
    pub recipient_id: Option<String>,
    pub blob_b64: String,
    pub blob_sha256: String,
    pub created_at: i64,
    pub expires_at: i64,
}

/// Opaque Welcome envelope submitted alongside an atomic Commit.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WelcomeSubmission {
    pub recipient_id: String,
    pub blob_b64: String,
}

/// Receipt returned after an atomic Commit advances a group's epoch.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommitReceipt {
    pub group_id: String,
    pub previous_epoch: u64,
    pub new_epoch: u64,
    pub commit_seq: u64,
    pub commit_sha256: String,
    pub welcome_seqs: Vec<u64>,
}

/// RFC 9162 Merkle checkpoint over a group's ordered ciphertext SHA-256 leaves.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RelayCheckpoint {
    pub origin: String,
    pub group_id: String,
    pub tree_size: u64,
    pub root_hash_hex: String,
    pub root_hash_b64: String,
    pub c2sp_note: String,
    pub consistency_from_size: Option<u64>,
    pub consistency_proof_hex: Option<Vec<String>>,
}

/// Counts of expired rows hard-deleted by [`MlsRelayStore::prune_expired`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PruneReport {
    pub envelopes_deleted: usize,
    pub key_packages_deleted: usize,
    pub groups_deleted: usize,
}

/// Untrusted, zero-plaintext Delivery Service core backed by SQLite (`mls_*` tables).
pub struct MlsRelayStore {
    conn: Connection,
    config: RelayConfig,
}

impl MlsRelayStore {
    /// Open (or create) a SQLite-backed relay store at `path`.
    pub fn open(path: &Path, config: RelayConfig) -> Result<Self, RelayError> {
        validate_config(config)?;
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .map_err(|err| RelayError::Storage(format!("create_dir_all failed: {err}")))?;
        }
        let conn = Connection::open(path)
            .map_err(|err| RelayError::Storage(format!("open sqlite failed: {err}")))?;
        Self::from_connection(conn, config)
    }

    /// Open an isolated in-memory SQLite relay store (primarily for tests).
    pub fn open_in_memory(config: RelayConfig) -> Result<Self, RelayError> {
        validate_config(config)?;
        let conn = Connection::open_in_memory()
            .map_err(|err| RelayError::Storage(format!("open in-memory sqlite failed: {err}")))?;
        Self::from_connection(conn, config)
    }

    /// Initialize the `mls_*` schema on an existing [`rusqlite::Connection`].
    pub fn from_connection(conn: Connection, config: RelayConfig) -> Result<Self, RelayError> {
        validate_config(config)?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(|err| RelayError::Storage(format!("busy_timeout failed: {err}")))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=FULL;
             PRAGMA foreign_keys=ON;
             CREATE TABLE IF NOT EXISTS mls_groups (
               group_id TEXT PRIMARY KEY,
               ciphersuite TEXT NOT NULL,
               current_epoch INTEGER NOT NULL DEFAULT 0,
               created_at INTEGER NOT NULL,
               expires_at INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS mls_key_packages (
               kp_id TEXT PRIMARY KEY,
               principal_id TEXT NOT NULL,
               ciphersuite TEXT NOT NULL,
               blob_b64 TEXT NOT NULL,
               blob_sha256 TEXT NOT NULL,
               created_at INTEGER NOT NULL,
               expires_at INTEGER NOT NULL,
               consumed INTEGER NOT NULL DEFAULT 0
             );
             CREATE TABLE IF NOT EXISTS mls_envelopes (
               seq INTEGER PRIMARY KEY AUTOINCREMENT,
               group_id TEXT NOT NULL REFERENCES mls_groups(group_id) ON DELETE CASCADE,
               kind TEXT NOT NULL CHECK (kind IN ('commit', 'private_message', 'welcome')),
               epoch INTEGER NOT NULL,
               recipient_id TEXT,
               blob_b64 TEXT NOT NULL,
               blob_sha256 TEXT NOT NULL,
               created_at INTEGER NOT NULL,
               expires_at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_mls_envelopes_group_seq
               ON mls_envelopes (group_id, seq);
             CREATE INDEX IF NOT EXISTS idx_mls_envelopes_group_epoch
               ON mls_envelopes (group_id, epoch);
             CREATE INDEX IF NOT EXISTS idx_mls_envelopes_expires_at
               ON mls_envelopes (expires_at);
             CREATE INDEX IF NOT EXISTS idx_mls_key_packages_principal_live
               ON mls_key_packages (principal_id, consumed, expires_at);
             CREATE INDEX IF NOT EXISTS idx_mls_key_packages_expires_at
               ON mls_key_packages (expires_at);
             CREATE INDEX IF NOT EXISTS idx_mls_groups_expires_at
               ON mls_groups (expires_at);
             CREATE INDEX IF NOT EXISTS idx_mls_envelopes_recipient_welcome
               ON mls_envelopes (recipient_id, kind, seq);",
        )
        .map_err(|err| RelayError::Storage(format!("schema init failed: {err}")))?;
        Ok(Self { conn, config })
    }

    /// Return the current relay configuration.
    pub const fn config(&self) -> RelayConfig {
        self.config
    }

    /// Toggle the fail-closed write switch at runtime.
    pub fn set_writes_enabled(&mut self, enabled: bool) {
        self.config.writes_enabled = enabled;
    }

    /// Publish an opaque single-use MLS KeyPackage for `principal_id`.
    pub fn publish_key_package(
        &mut self,
        kp_id: &str,
        principal_id: &str,
        ciphersuite: &str,
        blob_b64: &str,
        now: i64,
    ) -> Result<KeyPackageRecord, RelayError> {
        self.require_writes_enabled()?;
        validate_identifier("kp_id", kp_id)?;
        validate_principal_id(principal_id)?;
        validate_ciphersuite(ciphersuite)?;
        let decoded = decode_canonical_base64(blob_b64)?;
        let blob_sha256 = merkle::encode_hex(&merkle::sha256_bytes(&decoded));
        let expires_at = compute_expires_at(now, self.config.retention_seconds)?;

        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|err| RelayError::Storage(format!("begin immediate failed: {err}")))?;

        let existing: Option<String> = tx
            .query_row(
                "SELECT kp_id FROM mls_key_packages WHERE kp_id = ?1",
                params![kp_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|err| RelayError::Storage(format!("kp lookup failed: {err}")))?;
        if existing.is_some() {
            return Err(RelayError::Conflict(format!(
                "key package already exists: {kp_id}"
            )));
        }

        let live_count: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM mls_key_packages
                 WHERE principal_id = ?1 AND consumed = 0 AND expires_at > ?2",
                params![principal_id, now],
                |row| row.get(0),
            )
            .map_err(|err| RelayError::Storage(format!("kp count failed: {err}")))?;
        if live_count >= MAX_KEY_PACKAGES_PER_PRINCIPAL as i64 {
            return Err(RelayError::QuotaExceeded(format!(
                "principal {principal_id} already has {MAX_KEY_PACKAGES_PER_PRINCIPAL} live key packages"
            )));
        }

        tx.execute(
            "INSERT INTO mls_key_packages
               (kp_id, principal_id, ciphersuite, blob_b64, blob_sha256, created_at, expires_at, consumed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
            params![
                kp_id,
                principal_id,
                ciphersuite,
                blob_b64,
                blob_sha256,
                now,
                expires_at
            ],
        )
        .map_err(|err| RelayError::Storage(format!("kp insert failed: {err}")))?;

        tx.commit()
            .map_err(|err| RelayError::Storage(format!("kp commit failed: {err}")))?;

        Ok(KeyPackageRecord {
            kp_id: kp_id.to_owned(),
            principal_id: principal_id.to_owned(),
            ciphersuite: ciphersuite.to_owned(),
            blob_b64: blob_b64.to_owned(),
            blob_sha256,
            created_at: now,
            expires_at,
            consumed: false,
        })
    }

    /// Atomically fetch and consume one live KeyPackage for `principal_id`.
    ///
    /// Guarantees single-use delivery so the same KeyPackage is never handed out twice.
    pub fn fetch_and_consume_key_package(
        &mut self,
        principal_id: &str,
        now: i64,
    ) -> Result<Option<KeyPackageRecord>, RelayError> {
        self.require_writes_enabled()?;
        validate_principal_id(principal_id)?;

        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|err| RelayError::Storage(format!("begin immediate failed: {err}")))?;

        let candidate: Option<KeyPackageRecord> = tx
            .query_row(
                "SELECT kp_id, principal_id, ciphersuite, blob_b64, blob_sha256, created_at, expires_at
                 FROM mls_key_packages
                 WHERE principal_id = ?1 AND consumed = 0 AND expires_at > ?2
                 ORDER BY created_at ASC, rowid ASC
                 LIMIT 1",
                params![principal_id, now],
                |row| {
                    Ok(KeyPackageRecord {
                        kp_id: row.get(0)?,
                        principal_id: row.get(1)?,
                        ciphersuite: row.get(2)?,
                        blob_b64: row.get(3)?,
                        blob_sha256: row.get(4)?,
                        created_at: row.get(5)?,
                        expires_at: row.get(6)?,
                        consumed: true,
                    })
                },
            )
            .optional()
            .map_err(|err| RelayError::Storage(format!("kp select failed: {err}")))?;

        let Some(record) = candidate else {
            tx.commit()
                .map_err(|err| RelayError::Storage(format!("kp empty commit failed: {err}")))?;
            return Ok(None);
        };

        let deleted = tx
            .execute(
                "DELETE FROM mls_key_packages WHERE kp_id = ?1 AND consumed = 0",
                params![record.kp_id],
            )
            .map_err(|err| RelayError::Storage(format!("kp consume delete failed: {err}")))?;
        if deleted != 1 {
            return Err(RelayError::Storage(
                "key package consumption race detected".to_owned(),
            ));
        }

        tx.commit()
            .map_err(|err| RelayError::Storage(format!("kp consume commit failed: {err}")))?;

        Ok(Some(record))
    }

    /// Create a new opaque MLS group at epoch 0 with a bounded 24-hour TTL.
    pub fn create_group(
        &mut self,
        group_id: &str,
        ciphersuite: &str,
        now: i64,
    ) -> Result<GroupRecord, RelayError> {
        self.require_writes_enabled()?;
        validate_identifier("group_id", group_id)?;
        validate_ciphersuite(ciphersuite)?;
        let expires_at = compute_expires_at(now, self.config.retention_seconds)?;

        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|err| RelayError::Storage(format!("begin immediate failed: {err}")))?;

        let existing: Option<String> = tx
            .query_row(
                "SELECT group_id FROM mls_groups WHERE group_id = ?1",
                params![group_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|err| RelayError::Storage(format!("group lookup failed: {err}")))?;
        if existing.is_some() {
            return Err(RelayError::Conflict(format!(
                "group already exists: {group_id}"
            )));
        }

        let active_groups: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM mls_groups WHERE expires_at > ?1",
                params![now],
                |row| row.get(0),
            )
            .map_err(|err| RelayError::Storage(format!("group count failed: {err}")))?;
        if active_groups >= MAX_GROUPS as i64 {
            return Err(RelayError::QuotaExceeded(format!(
                "active group limit of {MAX_GROUPS} reached"
            )));
        }

        tx.execute(
            "INSERT INTO mls_groups (group_id, ciphersuite, current_epoch, created_at, expires_at)
             VALUES (?1, ?2, 0, ?3, ?4)",
            params![group_id, ciphersuite, now, expires_at],
        )
        .map_err(|err| RelayError::Storage(format!("group insert failed: {err}")))?;

        tx.commit()
            .map_err(|err| RelayError::Storage(format!("group commit failed: {err}")))?;

        Ok(GroupRecord {
            group_id: group_id.to_owned(),
            ciphersuite: ciphersuite.to_owned(),
            current_epoch: 0,
            created_at: now,
            expires_at,
        })
    }

    /// Look up a live (unexpired) group by `group_id`.
    pub fn get_group(&self, group_id: &str, now: i64) -> Result<Option<GroupRecord>, RelayError> {
        validate_identifier("group_id", group_id)?;
        let row = self
            .conn
            .query_row(
                "SELECT group_id, ciphersuite, current_epoch, created_at, expires_at
                 FROM mls_groups
                 WHERE group_id = ?1 AND expires_at > ?2",
                params![group_id, now],
                |row| {
                    let epoch_i64: i64 = row.get(2)?;
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        epoch_i64,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(|err| RelayError::Storage(format!("group select failed: {err}")))?;

        match row {
            None => Ok(None),
            Some((group_id, ciphersuite, epoch_i64, created_at, expires_at)) => {
                let current_epoch = u64::try_from(epoch_i64)
                    .map_err(|_| RelayError::Storage("negative epoch in database".to_owned()))?;
                Ok(Some(GroupRecord {
                    group_id,
                    ciphersuite,
                    current_epoch,
                    created_at,
                    expires_at,
                }))
            }
        }
    }

    /// Atomically submit an opaque MLS Commit with strict optimistic epoch concurrency.
    ///
    /// Requires `expected_epoch == current_epoch`. On mismatch, returns
    /// [`RelayError::EpochConflict`] with zero state mutation. On match, advances
    /// `current_epoch` to `expected_epoch + 1` and stores the `commit` envelope plus
    /// any attached `welcome` envelopes in one `BEGIN IMMEDIATE` transaction.
    pub fn submit_commit(
        &mut self,
        group_id: &str,
        expected_epoch: u64,
        commit_blob_b64: &str,
        welcomes: &[WelcomeSubmission],
        now: i64,
    ) -> Result<CommitReceipt, RelayError> {
        self.require_writes_enabled()?;
        validate_identifier("group_id", group_id)?;
        if welcomes.len() > MAX_WELCOMES_PER_COMMIT {
            return Err(RelayError::InvalidInput(format!(
                "welcome count {} exceeds maximum {MAX_WELCOMES_PER_COMMIT}",
                welcomes.len()
            )));
        }

        let commit_bytes = decode_canonical_base64(commit_blob_b64)?;
        let commit_sha256 = merkle::encode_hex(&merkle::sha256_bytes(&commit_bytes));

        let mut prepared_welcomes = Vec::with_capacity(welcomes.len());
        for welcome in welcomes {
            validate_principal_id(&welcome.recipient_id)?;
            let decoded = decode_canonical_base64(&welcome.blob_b64)?;
            let sha256 = merkle::encode_hex(&merkle::sha256_bytes(&decoded));
            prepared_welcomes.push((
                welcome.recipient_id.as_str(),
                welcome.blob_b64.as_str(),
                sha256,
            ));
        }

        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|err| RelayError::Storage(format!("begin immediate failed: {err}")))?;

        let group_row: Option<(i64, i64)> = tx
            .query_row(
                "SELECT current_epoch, expires_at FROM mls_groups WHERE group_id = ?1 AND expires_at > ?2",
                params![group_id, now],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|err| RelayError::Storage(format!("group epoch lookup failed: {err}")))?;

        let Some((current_epoch_i64, group_expires_at)) = group_row else {
            return Err(RelayError::NotFound(format!("group not found: {group_id}")));
        };
        let current_epoch = u64::try_from(current_epoch_i64)
            .map_err(|_| RelayError::Storage("negative epoch in database".to_owned()))?;

        if current_epoch != expected_epoch {
            return Err(RelayError::EpochConflict { current_epoch });
        }

        let existing_envelopes: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM mls_envelopes WHERE group_id = ?1",
                params![group_id],
                |row| row.get(0),
            )
            .map_err(|err| RelayError::Storage(format!("envelope count failed: {err}")))?;
        let existing_count = usize::try_from(existing_envelopes)
            .map_err(|_| RelayError::Storage("negative envelope count".to_owned()))?;
        let inserted_count = 1usize.saturating_add(prepared_welcomes.len());
        if existing_count.saturating_add(inserted_count) > MAX_ENVELOPES_PER_GROUP {
            return Err(RelayError::QuotaExceeded(format!(
                "group {group_id} would exceed {MAX_ENVELOPES_PER_GROUP} envelope limit"
            )));
        }

        let new_epoch = current_epoch
            .checked_add(1)
            .ok_or_else(|| RelayError::Storage("epoch counter overflow".to_owned()))?;
        let new_epoch_i64 = i64::try_from(new_epoch)
            .map_err(|_| RelayError::Storage("epoch exceeds SQLite integer range".to_owned()))?;

        let updated = tx
            .execute(
                "UPDATE mls_groups SET current_epoch = ?1 WHERE group_id = ?2 AND current_epoch = ?3",
                params![new_epoch_i64, group_id, current_epoch_i64],
            )
            .map_err(|err| RelayError::Storage(format!("epoch advance failed: {err}")))?;
        if updated != 1 {
            return Err(RelayError::EpochConflict { current_epoch });
        }

        tx.execute(
            "INSERT INTO mls_envelopes
               (group_id, kind, epoch, recipient_id, blob_b64, blob_sha256, created_at, expires_at)
             VALUES (?1, 'commit', ?2, NULL, ?3, ?4, ?5, ?6)",
            params![
                group_id,
                new_epoch_i64,
                commit_blob_b64,
                commit_sha256,
                now,
                group_expires_at
            ],
        )
        .map_err(|err| RelayError::Storage(format!("commit envelope insert failed: {err}")))?;
        let commit_seq = u64::try_from(tx.last_insert_rowid())
            .map_err(|_| RelayError::Storage("negative envelope seq".to_owned()))?;

        let mut welcome_seqs = Vec::with_capacity(prepared_welcomes.len());
        for (recipient_id, blob_b64, blob_sha256) in prepared_welcomes {
            tx.execute(
                "INSERT INTO mls_envelopes
                   (group_id, kind, epoch, recipient_id, blob_b64, blob_sha256, created_at, expires_at)
                 VALUES (?1, 'welcome', ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    group_id,
                    new_epoch_i64,
                    recipient_id,
                    blob_b64,
                    blob_sha256,
                    now,
                    group_expires_at
                ],
            )
            .map_err(|err| RelayError::Storage(format!("welcome envelope insert failed: {err}")))?;
            let seq = u64::try_from(tx.last_insert_rowid())
                .map_err(|_| RelayError::Storage("negative welcome seq".to_owned()))?;
            welcome_seqs.push(seq);
        }

        tx.commit()
            .map_err(|err| RelayError::Storage(format!("commit tx failed: {err}")))?;

        Ok(CommitReceipt {
            group_id: group_id.to_owned(),
            previous_epoch: expected_epoch,
            new_epoch,
            commit_seq,
            commit_sha256,
            welcome_seqs,
        })
    }

    /// Submit an opaque MLS `PrivateMessage` envelope for the group's active epoch.
    pub fn submit_private_message(
        &mut self,
        group_id: &str,
        epoch: u64,
        blob_b64: &str,
        now: i64,
    ) -> Result<EnvelopeRecord, RelayError> {
        self.require_writes_enabled()?;
        validate_identifier("group_id", group_id)?;
        let decoded = decode_canonical_base64(blob_b64)?;
        let blob_sha256 = merkle::encode_hex(&merkle::sha256_bytes(&decoded));
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|err| RelayError::Storage(format!("begin immediate failed: {err}")))?;

        let group_row: Option<(i64, i64)> = tx
            .query_row(
                "SELECT current_epoch, expires_at FROM mls_groups WHERE group_id = ?1 AND expires_at > ?2",
                params![group_id, now],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|err| RelayError::Storage(format!("group epoch lookup failed: {err}")))?;

        let Some((current_epoch_i64, group_expires_at)) = group_row else {
            return Err(RelayError::NotFound(format!("group not found: {group_id}")));
        };
        let current_epoch = u64::try_from(current_epoch_i64)
            .map_err(|_| RelayError::Storage("negative epoch in database".to_owned()))?;

        if current_epoch != epoch {
            return Err(RelayError::EpochConflict { current_epoch });
        }

        let existing_envelopes: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM mls_envelopes WHERE group_id = ?1",
                params![group_id],
                |row| row.get(0),
            )
            .map_err(|err| RelayError::Storage(format!("envelope count failed: {err}")))?;
        let existing_count = usize::try_from(existing_envelopes)
            .map_err(|_| RelayError::Storage("negative envelope count".to_owned()))?;
        if existing_count.saturating_add(1) > MAX_ENVELOPES_PER_GROUP {
            return Err(RelayError::QuotaExceeded(format!(
                "group {group_id} would exceed {MAX_ENVELOPES_PER_GROUP} envelope limit"
            )));
        }

        tx.execute(
            "INSERT INTO mls_envelopes
               (group_id, kind, epoch, recipient_id, blob_b64, blob_sha256, created_at, expires_at)
             VALUES (?1, 'private_message', ?2, NULL, ?3, ?4, ?5, ?6)",
            params![
                group_id,
                current_epoch_i64,
                blob_b64,
                blob_sha256,
                now,
                group_expires_at
            ],
        )
        .map_err(|err| RelayError::Storage(format!("private_message insert failed: {err}")))?;
        let seq = u64::try_from(tx.last_insert_rowid())
            .map_err(|_| RelayError::Storage("negative envelope seq".to_owned()))?;

        tx.commit()
            .map_err(|err| RelayError::Storage(format!("private_message commit failed: {err}")))?;

        Ok(EnvelopeRecord {
            seq,
            group_id: group_id.to_owned(),
            kind: EnvelopeKind::PrivateMessage,
            epoch,
            recipient_id: None,
            blob_b64: blob_b64.to_owned(),
            blob_sha256,
            created_at: now,
            expires_at: group_expires_at,
        })
    }

    /// Fetch bounded paginated group envelopes (`commit` and `private_message`) after `after_seq`,
    /// skipping expired rows.
    pub fn fetch_envelopes(
        &self,
        group_id: &str,
        after_seq: u64,
        limit: usize,
        now: i64,
    ) -> Result<Vec<EnvelopeRecord>, RelayError> {
        validate_identifier("group_id", group_id)?;
        let bounded_limit = validate_fetch_limit(limit)?;
        let after_seq_i64 = i64::try_from(after_seq)
            .map_err(|_| RelayError::InvalidInput("after_seq out of range".to_owned()))?;

        let group_exists: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM mls_groups WHERE group_id = ?1 AND expires_at > ?2",
                params![group_id, now],
                |row| row.get(0),
            )
            .optional()
            .map_err(|err| RelayError::Storage(format!("group check failed: {err}")))?;
        if group_exists.is_none() {
            return Err(RelayError::NotFound(format!("group not found: {group_id}")));
        }

        let mut stmt = self
            .conn
            .prepare(
                "SELECT seq, group_id, kind, epoch, recipient_id, blob_b64, blob_sha256, created_at, expires_at
                 FROM mls_envelopes
                 WHERE group_id = ?1
                   AND kind IN ('commit', 'private_message')
                   AND seq > ?2
                   AND expires_at > ?3
                 ORDER BY seq ASC
                 LIMIT ?4",
            )
            .map_err(|err| RelayError::Storage(format!("fetch_envelopes prepare failed: {err}")))?;

        collect_envelopes(
            &mut stmt,
            params![group_id, after_seq_i64, now, bounded_limit],
        )
    }

    /// Fetch bounded paginated `welcome` envelopes addressed to `recipient_id`, skipping expired rows.
    pub fn fetch_welcomes(
        &self,
        recipient_id: &str,
        after_seq: u64,
        limit: usize,
        now: i64,
    ) -> Result<Vec<EnvelopeRecord>, RelayError> {
        validate_principal_id(recipient_id)?;
        let bounded_limit = validate_fetch_limit(limit)?;
        let after_seq_i64 = i64::try_from(after_seq)
            .map_err(|_| RelayError::InvalidInput("after_seq out of range".to_owned()))?;

        let mut stmt = self
            .conn
            .prepare(
                "SELECT seq, group_id, kind, epoch, recipient_id, blob_b64, blob_sha256, created_at, expires_at
                 FROM mls_envelopes
                 WHERE recipient_id = ?1
                   AND kind = 'welcome'
                   AND seq > ?2
                   AND expires_at > ?3
                 ORDER BY seq ASC
                 LIMIT ?4",
            )
            .map_err(|err| RelayError::Storage(format!("fetch_welcomes prepare failed: {err}")))?;

        collect_envelopes(
            &mut stmt,
            params![recipient_id, after_seq_i64, now, bounded_limit],
        )
    }

    /// Compute an RFC 9162 Merkle tree checkpoint and optional consistency proof over the group's
    /// ordered live ciphertext `blob_sha256` leaves (`commit` and `private_message` envelopes).
    ///
    /// Formatted as `c2sp.org/tlog-checkpoint` with origin `agentciv-mls/<group_id>`.
    pub fn checkpoint(
        &self,
        group_id: &str,
        old_size: Option<u64>,
        now: i64,
    ) -> Result<RelayCheckpoint, RelayError> {
        validate_identifier("group_id", group_id)?;

        let group_exists: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM mls_groups WHERE group_id = ?1 AND expires_at > ?2",
                params![group_id, now],
                |row| row.get(0),
            )
            .optional()
            .map_err(|err| RelayError::Storage(format!("group check failed: {err}")))?;
        if group_exists.is_none() {
            return Err(RelayError::NotFound(format!("group not found: {group_id}")));
        }

        let mut stmt = self
            .conn
            .prepare(
                "SELECT blob_sha256
                 FROM mls_envelopes
                 WHERE group_id = ?1
                   AND kind IN ('commit', 'private_message')
                   AND expires_at > ?2
                 ORDER BY seq ASC",
            )
            .map_err(|err| {
                RelayError::Storage(format!("checkpoint query prepare failed: {err}"))
            })?;

        let rows = stmt
            .query_map(params![group_id, now], |row| row.get::<_, String>(0))
            .map_err(|err| RelayError::Storage(format!("checkpoint query failed: {err}")))?;

        let mut leaves = Vec::new();
        for row in rows {
            let hex_digest =
                row.map_err(|err| RelayError::Storage(format!("checkpoint row failed: {err}")))?;
            let blob_sha256 = merkle::decode_hex(&hex_digest).ok_or_else(|| {
                RelayError::Storage(format!("corrupt blob_sha256 in database: {hex_digest}"))
            })?;
            leaves.push(ciphertext_leaf_hash(&blob_sha256));
        }

        let tree_size = leaves.len() as u64;
        let root = merkle::root_hash(&leaves);
        let origin = relay_checkpoint_origin(group_id);
        let c2sp_note = merkle::format_c2sp_checkpoint(&origin, tree_size, &root);

        let (consistency_from_size, consistency_proof_hex) = match old_size {
            None => (None, None),
            Some(prev) => {
                if prev > tree_size {
                    return Err(RelayError::InvalidInput(format!(
                        "old_size {prev} exceeds current tree_size {tree_size}"
                    )));
                }
                if prev == 0 {
                    (Some(0), Some(Vec::new()))
                } else {
                    let prev_usize = usize::try_from(prev).map_err(|_| {
                        RelayError::InvalidInput("old_size exceeds addressable range".to_owned())
                    })?;
                    let proof =
                        merkle::consistency_proof(&leaves, prev_usize).ok_or_else(|| {
                            RelayError::InvalidInput(format!(
                                "cannot generate consistency proof from {prev} to {tree_size}"
                            ))
                        })?;
                    let encoded = proof.iter().map(merkle::encode_hex).collect();
                    (Some(prev), Some(encoded))
                }
            }
        };

        Ok(RelayCheckpoint {
            origin,
            group_id: group_id.to_owned(),
            tree_size,
            root_hash_hex: merkle::encode_hex(&root),
            root_hash_b64: merkle::encode_base64(&root),
            c2sp_note,
            consistency_from_size,
            consistency_proof_hex,
        })
    }

    /// Hard-delete expired `mls_envelopes`, `mls_key_packages`, and `mls_groups` (`expires_at <= now`).
    ///
    /// Runs even when `writes_enabled` is `false` so hard 24-hour TTL expiry is always enforceable.
    pub fn prune_expired(&mut self, now: i64) -> Result<PruneReport, RelayError> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|err| RelayError::Storage(format!("begin immediate failed: {err}")))?;

        let envelopes_deleted = tx
            .execute(
                "DELETE FROM mls_envelopes WHERE expires_at <= ?1",
                params![now],
            )
            .map_err(|err| RelayError::Storage(format!("prune envelopes failed: {err}")))?;
        let key_packages_deleted = tx
            .execute(
                "DELETE FROM mls_key_packages WHERE expires_at <= ?1",
                params![now],
            )
            .map_err(|err| RelayError::Storage(format!("prune key packages failed: {err}")))?;
        let groups_deleted = tx
            .execute(
                "DELETE FROM mls_groups WHERE expires_at <= ?1",
                params![now],
            )
            .map_err(|err| RelayError::Storage(format!("prune groups failed: {err}")))?;

        tx.commit()
            .map_err(|err| RelayError::Storage(format!("prune commit failed: {err}")))?;

        Ok(PruneReport {
            envelopes_deleted,
            key_packages_deleted,
            groups_deleted,
        })
    }

    fn require_writes_enabled(&self) -> Result<(), RelayError> {
        if !self.config.writes_enabled {
            return Err(RelayError::WritesDisabled);
        }
        Ok(())
    }
}

/// Canonical `c2sp.org/tlog-checkpoint` origin string for a PQ-MLS relay group.
pub fn relay_checkpoint_origin(group_id: &str) -> String {
    format!("agentciv-mls/{group_id}")
}

/// Compute the RFC 9162 leaf hash `SHA-256(0x00 || blob_sha256)` for a ciphertext digest.
pub fn ciphertext_leaf_hash(blob_sha256: &[u8; 32]) -> [u8; 32] {
    let mut input = [0u8; 33];
    input[0] = RFC9162_LEAF_PREFIX;
    input[1..].copy_from_slice(blob_sha256);
    merkle::sha256_bytes(&input)
}

/// Return current Unix epoch seconds.
pub fn current_unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|dur| dur.as_secs() as i64)
        .unwrap_or(0)
}

/// Decode strict canonical RFC 4648 standard base64 with `=` padding and enforce
/// decoded byte bounds `1..=MAX_OPAQUE_BLOB_BYTES`.
pub fn decode_canonical_base64(input: &str) -> Result<Vec<u8>, RelayError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() {
        return Err(RelayError::InvalidInput(
            "opaque blob must not be empty".to_owned(),
        ));
    }
    if !bytes.len().is_multiple_of(4) {
        return Err(RelayError::InvalidInput(
            "base64 length must be a multiple of 4".to_owned(),
        ));
    }

    // Fast pre-check on maximum encoded length before allocating.
    let max_encoded_len = MAX_OPAQUE_BLOB_BYTES.div_ceil(3) * 4;
    if bytes.len() > max_encoded_len {
        let approx_size = (bytes.len() / 4) * 3;
        return Err(RelayError::PayloadTooLarge {
            size: approx_size,
            max: MAX_OPAQUE_BLOB_BYTES,
        });
    }

    let mut out = Vec::with_capacity((bytes.len() / 4) * 3);
    let last_chunk_start = bytes.len() - 4;

    for (offset, chunk) in bytes.as_chunks::<4>().0.iter().enumerate() {
        let is_last = offset * 4 == last_chunk_start;
        let c0 = decode_b64_char(chunk[0]).ok_or_else(|| {
            RelayError::InvalidInput("invalid base64 character in position 0".to_owned())
        })?;
        let c1 = decode_b64_char(chunk[1]).ok_or_else(|| {
            RelayError::InvalidInput("invalid base64 character in position 1".to_owned())
        })?;

        match (chunk[2], chunk[3]) {
            (b'=', b'=') => {
                if !is_last {
                    return Err(RelayError::InvalidInput(
                        "base64 padding allowed only in final quantum".to_owned(),
                    ));
                }
                if (c1 & 0x0f) != 0 {
                    return Err(RelayError::InvalidInput(
                        "non-canonical base64 trailing bits before ==".to_owned(),
                    ));
                }
                out.push((c0 << 2) | (c1 >> 4));
            }
            (b'=', _) => {
                return Err(RelayError::InvalidInput(
                    "malformed base64 padding".to_owned(),
                ));
            }
            (b2, b'=') => {
                if !is_last {
                    return Err(RelayError::InvalidInput(
                        "base64 padding allowed only in final quantum".to_owned(),
                    ));
                }
                let c2 = decode_b64_char(b2).ok_or_else(|| {
                    RelayError::InvalidInput("invalid base64 character in position 2".to_owned())
                })?;
                if (c2 & 0x03) != 0 {
                    return Err(RelayError::InvalidInput(
                        "non-canonical base64 trailing bits before =".to_owned(),
                    ));
                }
                out.push((c0 << 2) | (c1 >> 4));
                out.push((c1 << 4) | (c2 >> 2));
            }
            (b2, b3) => {
                let c2 = decode_b64_char(b2).ok_or_else(|| {
                    RelayError::InvalidInput("invalid base64 character in position 2".to_owned())
                })?;
                let c3 = decode_b64_char(b3).ok_or_else(|| {
                    RelayError::InvalidInput("invalid base64 character in position 3".to_owned())
                })?;
                out.push((c0 << 2) | (c1 >> 4));
                out.push((c1 << 4) | (c2 >> 2));
                out.push((c2 << 6) | c3);
            }
        }
    }

    if out.len() > MAX_OPAQUE_BLOB_BYTES {
        return Err(RelayError::PayloadTooLarge {
            size: out.len(),
            max: MAX_OPAQUE_BLOB_BYTES,
        });
    }

    Ok(out)
}

fn decode_b64_char(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn validate_config(config: RelayConfig) -> Result<(), RelayError> {
    if config.retention_seconds < 1 || config.retention_seconds > DEFAULT_RETENTION_SECONDS {
        return Err(RelayError::InvalidInput(format!(
            "retention_seconds must be within 1..={DEFAULT_RETENTION_SECONDS}"
        )));
    }
    Ok(())
}

fn compute_expires_at(now: i64, retention_seconds: i64) -> Result<i64, RelayError> {
    now.checked_add(retention_seconds).ok_or_else(|| {
        RelayError::InvalidInput("timestamp overflow computing expires_at".to_owned())
    })
}

fn validate_ciphersuite(ciphersuite: &str) -> Result<(), RelayError> {
    if SUPPORTED_CIPHERSUITES.contains(&ciphersuite) {
        Ok(())
    } else {
        Err(RelayError::UnsupportedCiphersuite(ciphersuite.to_owned()))
    }
}

fn validate_identifier(field: &str, value: &str) -> Result<(), RelayError> {
    if value.is_empty() || value.len() > MAX_IDENTIFIER_BYTES {
        return Err(RelayError::InvalidInput(format!(
            "{field} length must be within 1..={MAX_IDENTIFIER_BYTES}"
        )));
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b':'))
    {
        return Err(RelayError::InvalidInput(format!(
            "{field} contains unsupported characters"
        )));
    }
    Ok(())
}

fn validate_principal_id(value: &str) -> Result<(), RelayError> {
    if value.is_empty() || value.len() > MAX_IDENTIFIER_BYTES {
        return Err(RelayError::InvalidInput(format!(
            "principal_id length must be within 1..={MAX_IDENTIFIER_BYTES}"
        )));
    }
    if !value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b':' | b'@' | b'#'))
    {
        return Err(RelayError::InvalidInput(
            "principal_id contains unsupported characters".to_owned(),
        ));
    }
    Ok(())
}

fn validate_fetch_limit(limit: usize) -> Result<i64, RelayError> {
    if limit == 0 {
        return Err(RelayError::InvalidInput(
            "limit must be at least 1".to_owned(),
        ));
    }
    let clamped = limit.min(MAX_FETCH_LIMIT);
    Ok(clamped as i64)
}

fn collect_envelopes<P: rusqlite::Params>(
    stmt: &mut rusqlite::Statement<'_>,
    params: P,
) -> Result<Vec<EnvelopeRecord>, RelayError> {
    let rows = stmt
        .query_map(params, |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
            ))
        })
        .map_err(|err| RelayError::Storage(format!("envelope query failed: {err}")))?;

    let mut out = Vec::new();
    for row in rows {
        let (
            seq_i64,
            group_id,
            kind_raw,
            epoch_i64,
            recipient_id,
            blob_b64,
            blob_sha256,
            created_at,
            expires_at,
        ) = row.map_err(|err| RelayError::Storage(format!("envelope row failed: {err}")))?;
        let seq = u64::try_from(seq_i64)
            .map_err(|_| RelayError::Storage("negative seq in database".to_owned()))?;
        let epoch = u64::try_from(epoch_i64)
            .map_err(|_| RelayError::Storage("negative epoch in database".to_owned()))?;
        let kind = EnvelopeKind::parse(&kind_raw)?;
        out.push(EnvelopeRecord {
            seq,
            group_id,
            kind,
            epoch,
            recipient_id,
            blob_b64,
            blob_sha256,
            created_at,
            expires_at,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PQ_SUITE: &str = "MLS_256_X25519MLKEM768_CHACHA20POLY1305_SHA512_MLDSA65";
    const CLASSICAL_SUITE: &str = "MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519";
    const T0: i64 = 1_764_200_000;

    fn enabled_store() -> MlsRelayStore {
        MlsRelayStore::open_in_memory(RelayConfig {
            writes_enabled: true,
            retention_seconds: DEFAULT_RETENTION_SECONDS,
        })
        .expect("open enabled store")
    }

    fn b64(bytes: &[u8]) -> String {
        merkle::encode_base64(bytes)
    }

    #[test]
    fn writes_disabled_by_default_blocks_all_mutations_with_503() {
        let mut store = MlsRelayStore::open_in_memory(RelayConfig::default()).expect("open");
        assert!(!store.config().writes_enabled);
        assert_eq!(store.config().retention_seconds, DEFAULT_RETENTION_SECONDS);

        let err_kp = store
            .publish_key_package("kp:1", "alice#01", PQ_SUITE, &b64(b"kp-bytes"), T0)
            .expect_err("publish_key_package must fail when writes disabled");
        assert_eq!(err_kp, RelayError::WritesDisabled);
        assert_eq!(err_kp.status_code(), 503);
        assert_eq!(err_kp.error_code(), "writes_disabled");

        let err_consume = store
            .fetch_and_consume_key_package("alice#01", T0)
            .expect_err("fetch_and_consume_key_package must fail when writes disabled");
        assert_eq!(err_consume, RelayError::WritesDisabled);

        let err_group = store
            .create_group("grp:alpha", PQ_SUITE, T0)
            .expect_err("create_group must fail when writes disabled");
        assert_eq!(err_group, RelayError::WritesDisabled);

        let err_commit = store
            .submit_commit("grp:alpha", 0, &b64(b"commit-0"), &[], T0)
            .expect_err("submit_commit must fail when writes disabled");
        assert_eq!(err_commit, RelayError::WritesDisabled);

        let err_pm = store
            .submit_private_message("grp:alpha", 0, &b64(b"ciphertext-0"), T0)
            .expect_err("submit_private_message must fail when writes disabled");
        assert_eq!(err_pm, RelayError::WritesDisabled);

        // Pruning expired rows remains allowed even when writes are disabled.
        let report = store.prune_expired(T0 + 1).expect("prune_expired");
        assert_eq!(report, PruneReport::default());
    }

    #[test]
    fn key_package_single_use_consumption_and_per_principal_cap() {
        let mut store = enabled_store();

        // Publish two KeyPackages for alice.
        let kp1 = store
            .publish_key_package("kp:a1", "alice#11", PQ_SUITE, &b64(b"opaque-kp-1"), T0)
            .expect("publish kp1");
        let kp2 = store
            .publish_key_package(
                "kp:a2",
                "alice#11",
                CLASSICAL_SUITE,
                &b64(b"opaque-kp-2"),
                T0 + 1,
            )
            .expect("publish kp2");
        assert_eq!(
            kp1.blob_sha256,
            merkle::encode_hex(&merkle::sha256_bytes(b"opaque-kp-1"))
        );
        assert!(!kp1.consumed);

        // Duplicate kp_id rejected with 409 conflict.
        let dup = store
            .publish_key_package("kp:a1", "alice#11", PQ_SUITE, &b64(b"other"), T0 + 2)
            .expect_err("duplicate kp_id");
        assert_eq!(dup.status_code(), 409);

        // First fetch returns kp1 and atomically marks it consumed.
        let got1 = store
            .fetch_and_consume_key_package("alice#11", T0 + 2)
            .expect("fetch 1")
            .expect("kp1 present");
        assert_eq!(got1.kp_id, "kp:a1");
        assert!(got1.consumed);

        // Second fetch returns kp2, never kp1 again.
        let got2 = store
            .fetch_and_consume_key_package("alice#11", T0 + 2)
            .expect("fetch 2")
            .expect("kp2 present");
        assert_eq!(got2.kp_id, kp2.kp_id);
        assert!(got2.consumed);

        // Third fetch returns None.
        let got3 = store
            .fetch_and_consume_key_package("alice#11", T0 + 2)
            .expect("fetch 3");
        assert!(got3.is_none());

        // Enforce MAX_KEY_PACKAGES_PER_PRINCIPAL (32) live unconsumed KeyPackages for bob.
        for i in 0..MAX_KEY_PACKAGES_PER_PRINCIPAL {
            store
                .publish_key_package(
                    &format!("kp:bob:{i}"),
                    "bob#22",
                    PQ_SUITE,
                    &b64(format!("bob-kp-{i}").as_bytes()),
                    T0 + i as i64,
                )
                .expect("within cap");
        }
        let over_cap = store
            .publish_key_package(
                "kp:bob:overflow",
                "bob#22",
                PQ_SUITE,
                &b64(b"overflow"),
                T0 + 50,
            )
            .expect_err("exceeding 32 live KeyPackages must fail");
        assert_eq!(over_cap.status_code(), 429);
        assert_eq!(over_cap.error_code(), "quota_exceeded");

        // Consuming one frees a slot for a new KeyPackage.
        assert!(
            store
                .fetch_and_consume_key_package("bob#22", T0 + 51)
                .expect("consume one")
                .is_some()
        );
        store
            .publish_key_package(
                "kp:bob:replacement",
                "bob#22",
                PQ_SUITE,
                &b64(b"ok"),
                T0 + 52,
            )
            .expect("slot freed after consumption");
    }

    #[test]
    fn optimistic_epoch_concurrency_rejects_competing_commit_and_stale_private_message() {
        let mut store = enabled_store();
        let group = store
            .create_group("grp:room-1", PQ_SUITE, T0)
            .expect("create group");
        assert_eq!(group.current_epoch, 0);

        // First commit at epoch 0 succeeds (0 -> 1) and delivers Welcome to bob.
        let receipt = store
            .submit_commit(
                "grp:room-1",
                0,
                &b64(b"commit-epoch-0-to-1"),
                &[WelcomeSubmission {
                    recipient_id: "bob#22".to_owned(),
                    blob_b64: b64(b"welcome-bob-epoch-1"),
                }],
                T0 + 10,
            )
            .expect("first commit succeeds");
        assert_eq!(receipt.previous_epoch, 0);
        assert_eq!(receipt.new_epoch, 1);
        assert_eq!(receipt.welcome_seqs.len(), 1);

        // Competing commit at stale epoch 0 fails with EpochConflict { current_epoch: 1 }
        // and writes ZERO envelopes (neither commit nor welcome).
        let conflict = store
            .submit_commit(
                "grp:room-1",
                0,
                &b64(b"competing-commit-epoch-0"),
                &[WelcomeSubmission {
                    recipient_id: "carol#33".to_owned(),
                    blob_b64: b64(b"welcome-carol-should-not-exist"),
                }],
                T0 + 11,
            )
            .expect_err("competing commit at epoch 0 must fail");
        assert_eq!(conflict, RelayError::EpochConflict { current_epoch: 1 });
        assert_eq!(conflict.status_code(), 409);
        assert_eq!(conflict.error_code(), "epoch_conflict");

        // Verify zero side effects from rejected commit.
        let envelopes = store
            .fetch_envelopes("grp:room-1", 0, DEFAULT_FETCH_LIMIT, T0 + 12)
            .expect("fetch envelopes");
        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].seq, receipt.commit_seq);
        assert_eq!(envelopes[0].kind, EnvelopeKind::Commit);
        assert_eq!(envelopes[0].epoch, 1);

        let carol_welcomes = store
            .fetch_welcomes("carol#33", 0, DEFAULT_FETCH_LIMIT, T0 + 12)
            .expect("carol welcomes");
        assert!(carol_welcomes.is_empty());

        let bob_welcomes = store
            .fetch_welcomes("bob#22", 0, DEFAULT_FETCH_LIMIT, T0 + 12)
            .expect("bob welcomes");
        assert_eq!(bob_welcomes.len(), 1);
        assert_eq!(bob_welcomes[0].kind, EnvelopeKind::Welcome);
        assert_eq!(bob_welcomes[0].epoch, 1);

        // Stale-epoch PrivateMessage (epoch 0 when active epoch is 1) rejected with 409.
        let stale_pm = store
            .submit_private_message("grp:room-1", 0, &b64(b"stale-msg"), T0 + 13)
            .expect_err("stale PrivateMessage must fail");
        assert_eq!(stale_pm, RelayError::EpochConflict { current_epoch: 1 });
        assert_eq!(stale_pm.status_code(), 409);

        // Current-epoch PrivateMessage (epoch 1) succeeds.
        let pm = store
            .submit_private_message("grp:room-1", 1, &b64(b"epoch-1-ciphertext"), T0 + 14)
            .expect("current epoch PrivateMessage succeeds");
        assert_eq!(pm.epoch, 1);
        assert_eq!(pm.kind, EnvelopeKind::PrivateMessage);
    }

    #[test]
    fn rfc9162_ciphertext_merkle_checkpoint_consistency_and_equivocation_detection() {
        let mut store = enabled_store();
        store
            .create_group("grp:audit", PQ_SUITE, T0)
            .expect("create group");

        // Empty group checkpoint.
        let cp0 = store
            .checkpoint("grp:audit", Some(0), T0 + 1)
            .expect("empty checkpoint");
        assert_eq!(cp0.origin, "agentciv-mls/grp:audit");
        assert_eq!(cp0.tree_size, 0);
        assert_eq!(
            cp0.root_hash_hex,
            merkle::encode_hex(&merkle::empty_root_hash())
        );
        assert!(cp0.c2sp_note.starts_with("agentciv-mls/grp:audit\n0\n"));

        // Epoch 0 -> 1 commit + 2 private messages (tree_size = 3).
        store
            .submit_commit("grp:audit", 0, &b64(b"c1"), &[], T0 + 2)
            .expect("c1");
        store
            .submit_private_message("grp:audit", 1, &b64(b"m1"), T0 + 3)
            .expect("m1");
        store
            .submit_private_message("grp:audit", 1, &b64(b"m2"), T0 + 4)
            .expect("m2");

        let cp3 = store.checkpoint("grp:audit", Some(0), T0 + 5).expect("cp3");
        assert_eq!(cp3.tree_size, 3);
        let root3 = merkle::decode_hex(&cp3.root_hash_hex).expect("root3 hex");

        // Epoch 1 -> 2 commit + 2 more private messages (tree_size = 6).
        store
            .submit_commit("grp:audit", 1, &b64(b"c2"), &[], T0 + 6)
            .expect("c2");
        store
            .submit_private_message("grp:audit", 2, &b64(b"m3"), T0 + 7)
            .expect("m3");
        store
            .submit_private_message("grp:audit", 2, &b64(b"m4"), T0 + 8)
            .expect("m4");

        let cp6 = store
            .checkpoint("grp:audit", Some(3), T0 + 9)
            .expect("cp6 with consistency proof from 3");
        assert_eq!(cp6.tree_size, 6);
        let root6 = merkle::decode_hex(&cp6.root_hash_hex).expect("root6 hex");
        let proof_hashes: Vec<[u8; 32]> = cp6
            .consistency_proof_hex
            .as_ref()
            .expect("proof present")
            .iter()
            .map(|h| merkle::decode_hex(h).expect("valid proof hex"))
            .collect();

        // Client independently verifies RFC 9162 append-only consistency from size 3 to 6.
        assert!(merkle::verify_consistency_proof(
            3,
            6,
            &root3,
            &root6,
            &proof_hashes
        ));

        // Client independently recomputes root6 from fetched opaque envelopes.
        let fetched = store
            .fetch_envelopes("grp:audit", 0, 100, T0 + 10)
            .expect("fetch all");
        assert_eq!(fetched.len(), 6);
        let client_leaves: Vec<[u8; 32]> = fetched
            .iter()
            .map(|env| {
                let digest = merkle::decode_hex(&env.blob_sha256).expect("sha256 hex");
                ciphertext_leaf_hash(&digest)
            })
            .collect();
        assert_eq!(merkle::root_hash(&client_leaves), root6);

        // Simulate adversarial relay equivocation: mutate an earlier ciphertext hash in SQLite.
        let forged_digest = merkle::encode_hex(&merkle::sha256_bytes(b"forged-m1"));
        store
            .conn
            .execute(
                "UPDATE mls_envelopes SET blob_sha256 = ?1 WHERE seq = ?2",
                params![forged_digest, fetched[1].seq as i64],
            )
            .expect("tamper db row");

        let cp6_equivocated = store
            .checkpoint("grp:audit", Some(3), T0 + 11)
            .expect("equivocated checkpoint");
        let equivocated_root6 =
            merkle::decode_hex(&cp6_equivocated.root_hash_hex).expect("equivocated root hex");
        let equivocated_proof: Vec<[u8; 32]> = cp6_equivocated
            .consistency_proof_hex
            .as_ref()
            .expect("proof present")
            .iter()
            .map(|h| merkle::decode_hex(h).expect("hex"))
            .collect();

        // Consistency verification against the client's previously witnessed root3 MUST FAIL.
        assert!(!merkle::verify_consistency_proof(
            3,
            6,
            &root3,
            &equivocated_root6,
            &equivocated_proof
        ));

        // Requesting consistency proof with old_size > tree_size fails cleanly.
        let bad_old = store
            .checkpoint("grp:audit", Some(99), T0 + 11)
            .expect_err("old_size > tree_size must fail");
        assert_eq!(bad_old.status_code(), 400);
    }

    #[test]
    fn hard_24h_expiry_pruning_removes_expired_groups_envelopes_and_key_packages() {
        let mut store = enabled_store();
        store
            .publish_key_package("kp:old", "alice#11", PQ_SUITE, &b64(b"kp-old"), T0)
            .expect("publish old kp");
        store
            .create_group("grp:ephemeral", PQ_SUITE, T0)
            .expect("create group");
        store
            .submit_commit(
                "grp:ephemeral",
                0,
                &b64(b"commit-old"),
                &[WelcomeSubmission {
                    recipient_id: "bob#22".to_owned(),
                    blob_b64: b64(b"welcome-old"),
                }],
                T0 + 5,
            )
            .expect("submit commit");
        store
            .submit_private_message("grp:ephemeral", 1, &b64(b"pm-old"), T0 + 10)
            .expect("submit pm");

        // Fresh records created later remain alive when earlier ones expire.
        let later = T0 + 1000;
        store
            .publish_key_package("kp:fresh", "alice#11", PQ_SUITE, &b64(b"kp-fresh"), later)
            .expect("publish fresh kp");
        store
            .create_group("grp:fresh", PQ_SUITE, later)
            .expect("create fresh group");

        // Advance clock past T0 + 86400 + 10 so all T0..T0+10 records are expired, while `later` is alive.
        let prune_at = T0 + DEFAULT_RETENTION_SECONDS + 10;

        // Even before pruning, expired group and welcomes are invisible to reads.
        assert!(
            store
                .get_group("grp:ephemeral", prune_at)
                .expect("get")
                .is_none()
        );
        assert!(
            store
                .fetch_welcomes("bob#22", 0, 10, prune_at)
                .expect("welcomes")
                .is_empty()
        );

        let report = store.prune_expired(prune_at).expect("prune");
        assert_eq!(report.envelopes_deleted, 3); // 1 commit + 1 welcome + 1 private_message
        assert_eq!(report.key_packages_deleted, 1); // kp:old
        assert_eq!(report.groups_deleted, 1); // grp:ephemeral

        // Fresh records survive pruning.
        assert!(
            store
                .get_group("grp:fresh", prune_at)
                .expect("fresh group")
                .is_some()
        );
        let fresh_kp = store
            .fetch_and_consume_key_package("alice#11", prune_at)
            .expect("fresh kp")
            .expect("fresh kp present");
        assert_eq!(fresh_kp.kp_id, "kp:fresh");
    }

    #[test]
    fn validation_rejects_oversized_empty_non_canonical_base64_and_bad_ciphersuite() {
        let mut store = enabled_store();
        store
            .create_group("grp:val", PQ_SUITE, T0)
            .expect("create group");

        // Oversized blob (MAX_OPAQUE_BLOB_BYTES + 1 = 65537 bytes) -> 413.
        let oversized = vec![0xabu8; MAX_OPAQUE_BLOB_BYTES + 1];
        let err_big = store
            .submit_private_message("grp:val", 0, &b64(&oversized), T0 + 1)
            .expect_err("oversized blob must fail");
        assert_eq!(err_big.status_code(), 413);
        assert_eq!(err_big.error_code(), "payload_too_large");

        // Exact MAX_OPAQUE_BLOB_BYTES (65536 bytes) succeeds.
        let exact_max = vec![0x42u8; MAX_OPAQUE_BLOB_BYTES];
        let ok_max = store
            .submit_private_message("grp:val", 0, &b64(&exact_max), T0 + 2)
            .expect("exact max blob succeeds");
        assert_eq!(
            ok_max.blob_sha256,
            merkle::encode_hex(&merkle::sha256_bytes(&exact_max))
        );

        // Empty blob -> 400.
        let err_empty = store
            .submit_private_message("grp:val", 0, "", T0 + 3)
            .expect_err("empty blob");
        assert_eq!(err_empty.status_code(), 400);

        // Non-canonical base64: unpadded ("YQ"), non-zero trailing bits ("YB==" instead of "YA=="),
        // whitespace ("YQ==\n"), URL-safe alphabet ("-_=="), invalid mid-padding ("AA==AAAA").
        for bad in [
            "YQ", "YB==", "YQB=", "YQ==\n", "-_==", "AA==AAAA", "AA=A", "!!!!",
        ] {
            let err = decode_canonical_base64(bad)
                .expect_err(&format!("non-canonical base64 {bad:?} must fail"));
            assert_eq!(err.status_code(), 400);
        }

        // Valid canonical base64 roundtrips cleanly for 1, 2, 3 byte lengths.
        for sample in [b"a".as_slice(), b"ab".as_slice(), b"abc".as_slice()] {
            let enc = b64(sample);
            assert_eq!(decode_canonical_base64(&enc).expect("valid"), sample);
        }

        // Unsupported ciphersuite -> 400.
        let err_suite = store
            .create_group("grp:bad-suite", "MLS_128_FAKE_CIPHERSUITE", T0 + 4)
            .expect_err("unsupported ciphersuite");
        assert_eq!(err_suite.status_code(), 400);
        assert_eq!(err_suite.error_code(), "unsupported_ciphersuite");

        // Invalid identifiers and invalid fetch limit -> 400.
        assert_eq!(
            store
                .create_group("../escape", PQ_SUITE, T0 + 5)
                .expect_err("bad group_id")
                .status_code(),
            400
        );
        assert_eq!(
            store
                .fetch_envelopes("grp:val", 0, 0, T0 + 5)
                .expect_err("zero limit")
                .status_code(),
            400
        );

        // Invalid retention config (> 24h or <= 0) rejected on open.
        assert_eq!(
            MlsRelayStore::open_in_memory(RelayConfig {
                writes_enabled: true,
                retention_seconds: DEFAULT_RETENTION_SECONDS + 1,
            })
            .err()
            .expect("over-24h retention")
            .status_code(),
            400
        );
        assert_eq!(
            MlsRelayStore::open_in_memory(RelayConfig {
                writes_enabled: true,
                retention_seconds: 0,
            })
            .err()
            .expect("zero retention")
            .status_code(),
            400
        );
    }

    #[test]
    fn active_group_cap_and_disk_persistence() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("relay.sqlite");

        {
            let mut store = MlsRelayStore::open(
                &db_path,
                RelayConfig {
                    writes_enabled: true,
                    retention_seconds: DEFAULT_RETENTION_SECONDS,
                },
            )
            .expect("open disk store");

            for i in 0..MAX_GROUPS {
                store
                    .create_group(&format!("grp:{i}"), PQ_SUITE, T0)
                    .expect("within MAX_GROUPS");
            }
            let over = store
                .create_group("grp:overflow", PQ_SUITE, T0)
                .expect_err("exceeding MAX_GROUPS must fail");
            assert_eq!(over.status_code(), 429);
            assert_eq!(over.error_code(), "quota_exceeded");
        }

        // Reopen from disk and verify persistence + default-closed toggle.
        let mut reopened = MlsRelayStore::open(&db_path, RelayConfig::default()).expect("reopen");
        assert!(reopened.get_group("grp:0", T0 + 1).expect("get").is_some());
        reopened.set_writes_enabled(true);
        assert!(reopened.config().writes_enabled);
        let _ = current_unix_seconds();
    }

    #[test]
    fn error_formatting_pagination_and_not_found_paths() {
        let mut store = enabled_store();
        assert_eq!(PROFILE_ID, "mls-relay/0.1-draft");
        assert_eq!(EnvelopeKind::Commit.as_str(), "commit");
        assert_eq!(EnvelopeKind::PrivateMessage.as_str(), "private_message");
        assert_eq!(EnvelopeKind::Welcome.as_str(), "welcome");

        // Unknown group -> 404 across commit, private_message, fetch_envelopes, checkpoint.
        assert_eq!(
            store
                .submit_commit("grp:missing", 0, &b64(b"c"), &[], T0)
                .expect_err("missing group commit")
                .status_code(),
            404
        );
        assert_eq!(
            store
                .submit_private_message("grp:missing", 0, &b64(b"m"), T0)
                .expect_err("missing group pm")
                .status_code(),
            404
        );
        assert_eq!(
            store
                .fetch_envelopes("grp:missing", 0, 10, T0)
                .expect_err("missing group fetch")
                .status_code(),
            404
        );
        assert_eq!(
            store
                .checkpoint("grp:missing", None, T0)
                .expect_err("missing group checkpoint")
                .status_code(),
            404
        );

        // Duplicate group -> 409.
        store.create_group("grp:dup", PQ_SUITE, T0).expect("create");
        let dup_err = store
            .create_group("grp:dup", PQ_SUITE, T0)
            .expect_err("duplicate group");
        assert_eq!(dup_err.status_code(), 409);
        assert_eq!(dup_err.error_code(), "conflict");

        // Paginated fetch_envelopes with after_seq.
        let m1 = store
            .submit_private_message("grp:dup", 0, &b64(b"p1"), T0 + 1)
            .expect("p1");
        let m2 = store
            .submit_private_message("grp:dup", 0, &b64(b"p2"), T0 + 2)
            .expect("p2");
        let page1 = store
            .fetch_envelopes("grp:dup", 0, 1, T0 + 3)
            .expect("page1");
        assert_eq!(page1.len(), 1);
        assert_eq!(page1[0].seq, m1.seq);
        let page2 = store
            .fetch_envelopes("grp:dup", m1.seq, 10, T0 + 3)
            .expect("page2");
        assert_eq!(page2.len(), 1);
        assert_eq!(page2[0].seq, m2.seq);

        // Checkpoint without old_size returns None consistency proof.
        let cp = store.checkpoint("grp:dup", None, T0 + 4).expect("cp");
        assert_eq!(cp.tree_size, 2);
        assert!(cp.consistency_from_size.is_none());
        assert!(cp.consistency_proof_hex.is_none());

        // Exercise Display + error_code + status_code on every RelayError variant.
        let variants = [
            RelayError::WritesDisabled,
            RelayError::InvalidInput("bad".to_owned()),
            RelayError::PayloadTooLarge { size: 10, max: 5 },
            RelayError::UnsupportedCiphersuite("x".to_owned()),
            RelayError::NotFound("y".to_owned()),
            RelayError::Conflict("z".to_owned()),
            RelayError::EpochConflict { current_epoch: 3 },
            RelayError::QuotaExceeded("q".to_owned()),
            RelayError::Storage("s".to_owned()),
        ];
        for err in variants {
            assert!(!err.to_string().is_empty());
            assert!(!err.error_code().is_empty());
            assert!(err.status_code() >= 400);
        }
    }

    #[test]
    fn envelope_expiry_bound_to_group_and_cascade_prune_leaves_zero_orphans() {
        let mut store = enabled_store();
        let group = store
            .create_group("grp:cascade", PQ_SUITE, T0)
            .expect("create group");
        let group_expiry = T0 + DEFAULT_RETENTION_SECONDS;
        assert_eq!(group.expires_at, group_expiry);

        // Submit commit, welcome, and private message well after group creation.
        let later = T0 + 3_600;
        store
            .submit_commit(
                "grp:cascade",
                0,
                &b64(b"commit-later"),
                &[WelcomeSubmission {
                    recipient_id: "bob#22".to_owned(),
                    blob_b64: b64(b"welcome-later"),
                }],
                later,
            )
            .expect("commit");
        let pm = store
            .submit_private_message("grp:cascade", 1, &b64(b"pm-later"), later + 10)
            .expect("pm");
        assert_eq!(pm.expires_at, group_expiry);

        let envs = store
            .fetch_envelopes("grp:cascade", 0, 10, later + 20)
            .expect("fetch envelopes");
        assert_eq!(envs.len(), 2);
        assert!(envs.iter().all(|e| e.expires_at == group_expiry));
        let welcomes = store
            .fetch_welcomes("bob#22", 0, 10, later + 20)
            .expect("fetch welcomes");
        assert_eq!(welcomes.len(), 1);
        assert_eq!(welcomes[0].expires_at, group_expiry);

        // At group_expiry, prune_expired removes the group and all envelopes (zero orphans).
        let report = store.prune_expired(group_expiry).expect("prune");
        assert_eq!(report.groups_deleted, 1);
        assert_eq!(report.envelopes_deleted, 3);

        let remaining_envelopes: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM mls_envelopes WHERE group_id = ?1",
                params!["grp:cascade"],
                |row| row.get(0),
            )
            .expect("count remaining envelopes");
        assert_eq!(remaining_envelopes, 0);

        // Recreating the same group_id starts cleanly with empty log and tree_size == 0.
        let recreated = store
            .create_group("grp:cascade", PQ_SUITE, group_expiry + 1)
            .expect("recreate group");
        assert_eq!(recreated.current_epoch, 0);
        assert!(
            store
                .fetch_envelopes("grp:cascade", 0, 10, group_expiry + 1)
                .expect("empty envelopes")
                .is_empty()
        );
        let cp = store
            .checkpoint("grp:cascade", None, group_expiry + 1)
            .expect("recreated checkpoint");
        assert_eq!(cp.tree_size, 0);
        assert!(
            store
                .fetch_welcomes("bob#22", 0, 10, group_expiry + 1)
                .expect("no stale welcomes")
                .is_empty()
        );
    }

    #[test]
    fn consumed_key_package_is_hard_deleted_immediately() {
        let mut store = enabled_store();
        store
            .publish_key_package("kp:purge", "alice#11", PQ_SUITE, &b64(b"secret-kp"), T0)
            .expect("publish");

        let fetched = store
            .fetch_and_consume_key_package("alice#11", T0 + 1)
            .expect("consume")
            .expect("kp returned");
        assert_eq!(fetched.kp_id, "kp:purge");
        assert!(fetched.consumed);

        // Row must be completely absent from mls_key_packages immediately after consumption.
        let row_count: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM mls_key_packages WHERE kp_id = ?1",
                params!["kp:purge"],
                |row| row.get(0),
            )
            .expect("count kp rows");
        assert_eq!(row_count, 0);

        // Second fetch returns None.
        assert!(
            store
                .fetch_and_consume_key_package("alice#11", T0 + 2)
                .expect("second fetch")
                .is_none()
        );
    }

    #[test]
    fn per_group_envelope_cap_rejects_commit_and_private_message_with_zero_writes() {
        let mut store = enabled_store();
        let group = store
            .create_group("grp:cap", PQ_SUITE, T0)
            .expect("create group");

        // Pre-populate MAX_ENVELOPES_PER_GROUP - 1 envelopes in one fast SQLite transaction.
        let prefill = MAX_ENVELOPES_PER_GROUP - 1;
        let payload_b64 = b64(b"x");
        let payload_sha256 = merkle::encode_hex(&merkle::sha256_bytes(b"x"));
        {
            let tx = store
                .conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .expect("begin prefill");
            let mut stmt = tx
                .prepare(
                    "INSERT INTO mls_envelopes
                       (group_id, kind, epoch, recipient_id, blob_b64, blob_sha256, created_at, expires_at)
                     VALUES (?1, 'private_message', 0, NULL, ?2, ?3, ?4, ?5)",
                )
                .expect("prepare prefill");
            for _ in 0..prefill {
                stmt.execute(params![
                    "grp:cap",
                    payload_b64,
                    payload_sha256,
                    T0,
                    group.expires_at
                ])
                .expect("insert prefill");
            }
            drop(stmt);
            tx.commit().expect("commit prefill");
        }

        // A Commit with 1 Welcome needs 2 slots (4095 + 2 > 4096) -> must fail with 429 and zero writes.
        let commit_over = store
            .submit_commit(
                "grp:cap",
                0,
                &b64(b"commit-over"),
                &[WelcomeSubmission {
                    recipient_id: "bob#22".to_owned(),
                    blob_b64: b64(b"welcome-over"),
                }],
                T0 + 1,
            )
            .expect_err("commit + welcome exceeding cap must fail");
        assert_eq!(commit_over.status_code(), 429);
        assert_eq!(commit_over.error_code(), "quota_exceeded");
        assert_eq!(
            store
                .get_group("grp:cap", T0 + 1)
                .expect("get")
                .expect("present")
                .current_epoch,
            0
        );

        // Filling the exact final slot (4095 -> 4096) succeeds.
        store
            .submit_private_message("grp:cap", 0, &b64(b"final-slot"), T0 + 2)
            .expect("final slot succeeds");

        // Any further PrivateMessage or Commit (even with 0 welcomes) fails with 429.
        let pm_over = store
            .submit_private_message("grp:cap", 0, &b64(b"overflow-pm"), T0 + 3)
            .expect_err("pm exceeding cap must fail");
        assert_eq!(pm_over.status_code(), 429);
        assert_eq!(pm_over.error_code(), "quota_exceeded");

        let commit_no_welcome_over = store
            .submit_commit("grp:cap", 0, &b64(b"overflow-commit"), &[], T0 + 3)
            .expect_err("commit exceeding cap must fail");
        assert_eq!(commit_no_welcome_over.status_code(), 429);
        assert_eq!(commit_no_welcome_over.error_code(), "quota_exceeded");

        let total_rows: i64 = store
            .conn
            .query_row(
                "SELECT COUNT(*) FROM mls_envelopes WHERE group_id = ?1",
                params!["grp:cap"],
                |row| row.get(0),
            )
            .expect("count total rows");
        assert_eq!(total_rows as usize, MAX_ENVELOPES_PER_GROUP);
    }
}
