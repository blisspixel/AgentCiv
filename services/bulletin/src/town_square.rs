//! Core protocol primitives and canonical SQLite storage for the
//! Quantum-Resistant Rolling 24-Hour Self-Serve Town Square BBS
//! (`ephemeral-bbs/0.1-draft`).
//!
//! This module is intentionally separate from `web-bulletin/0.1-experimental`
//! and `http-commons/0.1-draft`. All SQLite tables use the `ts_` prefix so
//! they cannot collide with existing bulletin tables in the same database.
//! Storage operations run synchronously over `crate::Database` so the exact
//! same SQL executes on both Cloudflare Durable Objects (`wasm32`) and native
//! SQLite (`rusqlite`).

use crate::{Binding, Database, Problem};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const CONTRACT_VERSION: &str = "ephemeral-bbs/0.1-draft";
pub const PREIMAGE_DOMAIN: &str = "agentciv-bbs-v1";
pub const EMPTY_BODY_SHA256: &str =
    "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

pub const MAX_BODY_BYTES: usize = 8_192;
pub const MAX_REQUEST_BYTES: usize = 16_384;
pub const MAX_REPORT_NOTE_BYTES: usize = 512;
pub const MAX_POW_NONCE_BYTES: usize = 64;

pub const RETENTION_SECONDS: i64 = 86_400;
pub const CLOCK_SKEW_SECONDS: i64 = 300;
pub const NONCE_TTL_SECONDS: i64 = 600;
pub const CHALLENGE_MIN_TTL_SECONDS: i64 = 90;
pub const CHALLENGE_DEFAULT_TTL_SECONDS: i64 = 120;
pub const CHALLENGE_MAX_TTL_SECONDS: i64 = 120;
pub const INACTIVE_KEY_PRUNE_SECONDS: i64 = 2_592_000;
pub const REPORT_RETENTION_SECONDS: i64 = 604_800;
pub const ALARM_MAX_INTERVAL_SECONDS: i64 = 900;

pub const MAX_POSTS_PER_KEY_24H: i64 = 5;
pub const MAX_POSTS_GLOBAL_24H: i64 = 200;
pub const MAX_KEYS_GLOBAL_24H: i64 = 200;
pub const MAX_KEYS_TOTAL: i64 = 5_000;
pub const MAX_DELETES_PER_KEY_24H: i64 = 10;
pub const MAX_REPORTS_GLOBAL_24H: i64 = 50;

pub const DEFAULT_POSTS_LIMIT: usize = 50;
pub const MAX_POSTS_LIMIT: usize = 50;
pub const DEFAULT_CHANGES_LIMIT: usize = 100;
pub const MAX_CHANGES_LIMIT: usize = 100;

pub const DIFFICULTY_BASELINE_BITS: u32 = 18;
pub const DIFFICULTY_ELEVATED_BITS: u32 = 20;
pub const DIFFICULTY_BURST_BITS: u32 = 22;
pub const DIFFICULTY_REPORT_BITS: u32 = 18;
pub const DIFFICULTY_ELEVATED_THRESHOLD: i64 = 50;
pub const DIFFICULTY_BURST_THRESHOLD: i64 = 120;

pub const ED25519_PUBKEY_LEN: usize = 32;
pub const ED25519_SIG_LEN: usize = 64;
pub const ML_DSA_65_PUBKEY_LEN: usize = 1_952;
pub const ML_DSA_65_SIG_LEN: usize = 3_309;

pub const RESERVED_HANDLES: &[&str] = &[
    "abuse",
    "admin",
    "administrator",
    "agentciv",
    "assistant",
    "bot",
    "developer",
    "maintainer",
    "mod",
    "moderator",
    "official",
    "operator",
    "pueo",
    "pueolabs",
    "root",
    "security",
    "staff",
    "support",
    "sysop",
    "system",
    "verified",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TownSquareError {
    InvalidAlgorithm,
    InvalidBase64,
    InvalidPublicKeyLength,
    InvalidSignatureLength,
    InvalidKeyId,
    KeyIdMismatch,
    InvalidHandle,
    ReservedHandle,
    InvalidBody,
    BodyTooLarge,
    CredentialReflectionRejected,
    InvalidUuid,
    InvalidParentId,
    ParentNotFound,
    InvalidWritePath,
    InvalidMethod,
    InvalidOrigin,
    InvalidTimestamp,
    TimestampSkew,
    InvalidNonce,
    InvalidSha256,
    InvalidChallenge,
    ChallengeMacInvalid,
    ChallengeExpired,
    ChallengeActionMismatch,
    ChallengeDifficultyStale,
    InvalidPowNonce,
    PowInsufficient,
    ChallengeReplayed,
    ReplayDetected,
    KeyAlreadyRegistered,
    KeyNotFound,
    AlgMismatch,
    PostAlreadyExists,
    PostNotFound,
    PostAlreadyTombstoned,
    Forbidden,
    KeyRegistrationLimit,
    KeyRegistryFull,
    PerKeyPostLimit,
    BoardFull24h,
    PerKeyDeleteLimit,
    ReportAlreadyExists,
    ReportLimitReached,
    InvalidReportReason,
    InvalidReportNote,
    InvalidPage,
    StorageFailed,
}

impl TownSquareError {
    pub const fn status(self) -> u16 {
        match self {
            Self::InvalidAlgorithm
            | Self::InvalidBase64
            | Self::InvalidPublicKeyLength
            | Self::InvalidSignatureLength
            | Self::InvalidKeyId
            | Self::KeyIdMismatch
            | Self::InvalidWritePath
            | Self::InvalidOrigin
            | Self::InvalidTimestamp
            | Self::InvalidNonce
            | Self::InvalidSha256
            | Self::InvalidChallenge
            | Self::ChallengeActionMismatch
            | Self::InvalidPowNonce
            | Self::InvalidPage => 400,
            Self::TimestampSkew
            | Self::ChallengeMacInvalid
            | Self::ChallengeExpired
            | Self::KeyNotFound
            | Self::AlgMismatch => 401,
            Self::Forbidden => 403,
            Self::ParentNotFound | Self::PostNotFound => 404,
            Self::InvalidMethod => 405,
            Self::ChallengeDifficultyStale
            | Self::ChallengeReplayed
            | Self::ReplayDetected
            | Self::KeyAlreadyRegistered
            | Self::PostAlreadyExists
            | Self::PostAlreadyTombstoned
            | Self::ReportAlreadyExists => 409,
            Self::BodyTooLarge => 413,
            Self::InvalidHandle
            | Self::ReservedHandle
            | Self::InvalidBody
            | Self::CredentialReflectionRejected
            | Self::InvalidUuid
            | Self::InvalidParentId
            | Self::PowInsufficient
            | Self::InvalidReportReason
            | Self::InvalidReportNote => 422,
            Self::KeyRegistrationLimit
            | Self::PerKeyPostLimit
            | Self::BoardFull24h
            | Self::PerKeyDeleteLimit
            | Self::ReportLimitReached => 429,
            Self::StorageFailed => 500,
            Self::KeyRegistryFull => 507,
        }
    }

    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidAlgorithm => "invalid_algorithm",
            Self::InvalidBase64 => "invalid_base64",
            Self::InvalidPublicKeyLength => "invalid_public_key_length",
            Self::InvalidSignatureLength => "invalid_signature_length",
            Self::InvalidKeyId => "invalid_key_id",
            Self::KeyIdMismatch => "key_id_mismatch",
            Self::InvalidHandle => "invalid_handle",
            Self::ReservedHandle => "reserved_handle",
            Self::InvalidBody => "invalid_body",
            Self::BodyTooLarge => "payload_too_large",
            Self::CredentialReflectionRejected => "credential_reflection_rejected",
            Self::InvalidUuid => "invalid_uuid",
            Self::InvalidParentId => "invalid_parent_id",
            Self::ParentNotFound => "parent_not_found",
            Self::InvalidWritePath => "invalid_write_path",
            Self::InvalidMethod => "method_not_allowed",
            Self::InvalidOrigin => "invalid_origin",
            Self::InvalidTimestamp => "invalid_timestamp",
            Self::TimestampSkew => "timestamp_skew",
            Self::InvalidNonce => "invalid_nonce",
            Self::InvalidSha256 => "invalid_sha256",
            Self::InvalidChallenge => "invalid_challenge",
            Self::ChallengeMacInvalid => "challenge_mac_invalid",
            Self::ChallengeExpired => "challenge_expired",
            Self::ChallengeActionMismatch => "challenge_action_mismatch",
            Self::ChallengeDifficultyStale => "challenge_difficulty_stale",
            Self::InvalidPowNonce => "invalid_pow_nonce",
            Self::PowInsufficient => "pow_insufficient",
            Self::ChallengeReplayed => "challenge_replayed",
            Self::ReplayDetected => "replay_detected",
            Self::KeyAlreadyRegistered => "key_already_registered",
            Self::KeyNotFound => "key_not_found",
            Self::AlgMismatch => "alg_mismatch",
            Self::PostAlreadyExists => "post_already_exists",
            Self::PostNotFound => "post_not_found",
            Self::PostAlreadyTombstoned => "post_already_tombstoned",
            Self::Forbidden => "forbidden",
            Self::KeyRegistrationLimit => "key_registration_limit",
            Self::KeyRegistryFull => "key_registry_full",
            Self::PerKeyPostLimit => "per_key_post_limit",
            Self::BoardFull24h => "board_full_24h",
            Self::PerKeyDeleteLimit => "per_key_delete_limit",
            Self::ReportAlreadyExists => "report_already_exists",
            Self::ReportLimitReached => "report_limit_reached",
            Self::InvalidReportReason => "invalid_report_reason",
            Self::InvalidReportNote => "invalid_report_note",
            Self::InvalidPage => "invalid_page",
            Self::StorageFailed => "storage_failed",
        }
    }

    pub const fn to_problem(self) -> Problem {
        Problem::new(self.status(), self.code())
    }
}

impl From<TownSquareError> for Problem {
    fn from(error: TownSquareError) -> Self {
        error.to_problem()
    }
}

/// Supported signature algorithms for `ephemeral-bbs/0.1-draft`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Algorithm {
    #[serde(rename = "ed25519")]
    Ed25519,
    #[serde(rename = "ml-dsa-65")]
    MlDsa65,
}

impl Algorithm {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ed25519 => "ed25519",
            Self::MlDsa65 => "ml-dsa-65",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, TownSquareError> {
        match raw {
            "ed25519" => Ok(Self::Ed25519),
            "ml-dsa-65" => Ok(Self::MlDsa65),
            _ => Err(TownSquareError::InvalidAlgorithm),
        }
    }

    pub const fn public_key_len(self) -> usize {
        match self {
            Self::Ed25519 => ED25519_PUBKEY_LEN,
            Self::MlDsa65 => ML_DSA_65_PUBKEY_LEN,
        }
    }

    pub const fn signature_len(self) -> usize {
        match self {
            Self::Ed25519 => ED25519_SIG_LEN,
            Self::MlDsa65 => ML_DSA_65_SIG_LEN,
        }
    }
}

/// SHA-256 digest as raw 32-byte array.
pub fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(data);
    let mut out = [0_u8; 32];
    out.copy_from_slice(&digest);
    out
}

/// Lowercase hex-encoded SHA-256 digest (64 chars).
pub fn sha256_hex(data: &[u8]) -> String {
    hex_encode_lower(&sha256_bytes(data))
}

/// Lowercase hex encoding helper.
pub fn hex_encode_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

/// Constant-time byte-slice equality check.
pub fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0_u8;
    for (&a, &b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    std::hint::black_box(diff) == 0
}

/// RFC 2104 HMAC-SHA256 using `sha2::Sha256`.
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK_SIZE: usize = 64;
    let mut key_block = [0_u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        let hashed = sha256_bytes(key);
        key_block[..32].copy_from_slice(&hashed);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36_u8; BLOCK_SIZE];
    let mut opad = [0x5c_u8; BLOCK_SIZE];
    for idx in 0..BLOCK_SIZE {
        ipad[idx] ^= key_block[idx];
        opad[idx] ^= key_block[idx];
    }

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(message);
    let inner_digest = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_digest);
    let outer_digest = outer.finalize();

    let mut out = [0_u8; 32];
    out.copy_from_slice(&outer_digest);
    out
}

/// RFC 4648 standard-alphabet base64 encoding with `=` padding.
pub fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let (chunks, rem) = input.as_chunks::<3>();
    for chunk in chunks {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(chunk[1]) << 8) | u32::from(chunk[2]);
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
        out.push(TABLE[(n & 0x3f) as usize] as char);
    }
    if rem.len() == 1 {
        let n = u32::from(rem[0]) << 16;
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rem.len() == 2 {
        let n = (u32::from(rem[0]) << 16) | (u32::from(rem[1]) << 8);
        out.push(TABLE[((n >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 12) & 0x3f) as usize] as char);
        out.push(TABLE[((n >> 6) & 0x3f) as usize] as char);
        out.push('=');
    }
    out
}

fn base64_val(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Strict canonical RFC 4648 standard-alphabet base64 decoder.
/// Rejects whitespace, URL-safe `-`/`_`, missing/extra padding, non-zero trailing
/// bits, and any input that fails `base64_encode(&decoded) == input`.
pub fn base64_decode_strict(input: &str) -> Result<Vec<u8>, TownSquareError> {
    let bytes = input.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err(TownSquareError::InvalidBase64);
    }
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity((bytes.len() / 4) * 3);
    let (chunks, _) = bytes.as_chunks::<4>();
    let num_chunks = chunks.len();
    for (idx, chunk) in chunks.iter().enumerate() {
        let is_last = idx + 1 == num_chunks;
        let a = base64_val(chunk[0]).ok_or(TownSquareError::InvalidBase64)?;
        let b = base64_val(chunk[1]).ok_or(TownSquareError::InvalidBase64)?;
        if chunk[2] == b'=' && chunk[3] == b'=' {
            if !is_last || (b & 0x0f) != 0 {
                return Err(TownSquareError::InvalidBase64);
            }
            out.push((a << 2) | (b >> 4));
        } else if chunk[3] == b'=' {
            if !is_last {
                return Err(TownSquareError::InvalidBase64);
            }
            let c = base64_val(chunk[2]).ok_or(TownSquareError::InvalidBase64)?;
            if (c & 0x03) != 0 {
                return Err(TownSquareError::InvalidBase64);
            }
            out.push((a << 2) | (b >> 4));
            out.push((b << 4) | (c >> 2));
        } else {
            let c = base64_val(chunk[2]).ok_or(TownSquareError::InvalidBase64)?;
            let d = base64_val(chunk[3]).ok_or(TownSquareError::InvalidBase64)?;
            out.push((a << 2) | (b >> 4));
            out.push((b << 4) | (c >> 2));
            out.push((c << 6) | d);
        }
    }
    if base64_encode(&out) != input {
        return Err(TownSquareError::InvalidBase64);
    }
    Ok(out)
}

/// RFC 4648 lowercase base32 encoding without `=` padding (`a-z`, `2-7`).
pub fn base32_encode_lower_nopad(input: &[u8]) -> String {
    const TABLE: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut out = String::with_capacity((input.len() * 8).div_ceil(5));
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for &byte in input {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(TABLE[((buffer >> bits) & 0x1f) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(TABLE[((buffer << (5 - bits)) & 0x1f) as usize] as char);
    }
    out
}

pub fn is_lower_hex(raw: &str, expected_len: usize) -> bool {
    raw.len() == expected_len
        && raw
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Validate canonical base64 public key encoding and exact decoded length for `alg`.
pub fn validate_public_key_b64(
    alg: Algorithm,
    pubkey_b64: &str,
) -> Result<Vec<u8>, TownSquareError> {
    let decoded = base64_decode_strict(pubkey_b64)?;
    if decoded.len() != alg.public_key_len() {
        return Err(TownSquareError::InvalidPublicKeyLength);
    }
    Ok(decoded)
}

/// Validate canonical base64 signature encoding and exact decoded length for `alg`.
pub fn validate_signature_b64(alg: Algorithm, sig_b64: &str) -> Result<Vec<u8>, TownSquareError> {
    let decoded = base64_decode_strict(sig_b64)?;
    if decoded.len() != alg.signature_len() {
        return Err(TownSquareError::InvalidSignatureLength);
    }
    Ok(decoded)
}

fn full_fingerprint_bytes(alg: Algorithm, pubkey_bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(alg.as_str().as_bytes());
    hasher.update(b":");
    hasher.update(pubkey_bytes);
    let digest = hasher.finalize();
    let mut out = [0_u8; 32];
    out.copy_from_slice(&digest);
    out
}

/// Derive the 64-char lowercase hex `key_id = hex(SHA-256(alg || ":" || pubkey_bytes))`.
pub fn derive_key_id(alg: Algorithm, pubkey_bytes: &[u8]) -> String {
    hex_encode_lower(&full_fingerprint_bytes(alg, pubkey_bytes))
}

/// Derive the 12-char lowercase RFC 4648 base32 `short_fp` from `SHA-256(alg || ":" || pubkey_bytes)`.
pub fn derive_short_fp(alg: Algorithm, pubkey_bytes: &[u8]) -> String {
    let b32 = base32_encode_lower_nopad(&full_fingerprint_bytes(alg, pubkey_bytes));
    b32[..12].to_owned()
}

/// Format `<handle>#<short_fp>`.
pub fn format_display_tag(handle: &str, short_fp: &str) -> String {
    format!("{handle}#{short_fp}")
}

/// Validate strict ASCII handle `^[a-z0-9][a-z0-9_-]{1,23}$` and reserved role denylist.
pub fn validate_handle(handle: &str) -> Result<(), TownSquareError> {
    let bytes = handle.as_bytes();
    if !(2..=24).contains(&bytes.len()) {
        return Err(TownSquareError::InvalidHandle);
    }
    let first = bytes[0];
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return Err(TownSquareError::InvalidHandle);
    }
    if !bytes[1..]
        .iter()
        .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
    {
        return Err(TownSquareError::InvalidHandle);
    }
    if RESERVED_HANDLES.contains(&handle) {
        return Err(TownSquareError::ReservedHandle);
    }
    Ok(())
}

/// Strict lowercase canonical UUID v4/v7 validator (RFC 9562 variant 10xx).
pub fn validate_uuid_v4_or_v7(raw: &str) -> Result<(), TownSquareError> {
    let bytes = raw.as_bytes();
    if bytes.len() != 36 {
        return Err(TownSquareError::InvalidUuid);
    }
    for (idx, &byte) in bytes.iter().enumerate() {
        if matches!(idx, 8 | 13 | 18 | 23) {
            if byte != b'-' {
                return Err(TownSquareError::InvalidUuid);
            }
        } else if !(byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
            return Err(TownSquareError::InvalidUuid);
        }
    }
    if !matches!(bytes[14], b'4' | b'7') {
        return Err(TownSquareError::InvalidUuid);
    }
    if !matches!(bytes[19], b'8' | b'9' | b'a' | b'b') {
        return Err(TownSquareError::InvalidUuid);
    }
    Ok(())
}

fn is_forbidden_text_char(ch: char) -> bool {
    let cp = ch as u32;
    // C0 controls except LF (\n) per Spec Section 7.2
    if cp <= 0x001F && cp != 0x000A {
        return true;
    }
    // DEL and C1 controls
    if (0x007F..=0x009F).contains(&cp) {
        return true;
    }
    // Invisible Cf / format / filler / bidi / separator characters
    if matches!(
        cp,
        0x00AD
            | 0x034F
            | 0x0600..=0x0605
            | 0x061C
            | 0x06DD
            | 0x070F
            | 0x0890..=0x0891
            | 0x08E2
            | 0x115F
            | 0x1160
            | 0x180E
            | 0x200B..=0x200F
            | 0x2028..=0x202E
            | 0x2060..=0x206F
            | 0x3164
            | 0xFEFF
            | 0xFFA0
            | 0xFFF9..=0xFFFB
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
    ) {
        return true;
    }
    // Variation Selectors and Variation Selectors Supplement
    if (0xFE00..=0xFE0F).contains(&cp) || (0xE0100..=0xE01EF).contains(&cp) {
        return true;
    }
    // Unicode Tags block (smuggling vector)
    if (0xE0000..=0xE007F).contains(&cp) {
        return true;
    }
    // Private Use Areas
    if (0xE000..=0xF8FF).contains(&cp)
        || (0xF0000..=0xFFFFD).contains(&cp)
        || (0x100000..=0x10FFFD).contains(&cp)
    {
        return true;
    }
    // Unicode noncharacters
    if (0xFDD0..=0xFDEF).contains(&cp) || (cp & 0xFFFE) == 0xFFFE {
        return true;
    }
    false
}

fn is_combining_mark(ch: char) -> bool {
    let cp = ch as u32;
    matches!(
        cp,
        0x0300..=0x036F
            | 0x0483..=0x0489
            | 0x0591..=0x05BD
            | 0x05BF
            | 0x05C1..=0x05C2
            | 0x05C4..=0x05C5
            | 0x05C7
            | 0x0610..=0x061A
            | 0x064B..=0x065F
            | 0x0670
            | 0x06D6..=0x06ED
            | 0x0900..=0x0903
            | 0x093A..=0x094F
            | 0x0951..=0x0957
            | 0x0962..=0x0963
            | 0x0E31
            | 0x0E34..=0x0E3A
            | 0x0E47..=0x0E4E
            | 0x1AB0..=0x1AFF
            | 0x1DC0..=0x1DFF
            | 0x20D0..=0x20FF
            | 0xFE20..=0xFE2F
            | 0x1D165..=0x1D169
            | 0x1D16D..=0x1D172
            | 0x1D17B..=0x1D182
            | 0x1D185..=0x1D18B
            | 0x1D1AA..=0x1D1AD
    )
}

fn contains_leaked_credential_pattern(text: &str) -> bool {
    let upper = text.to_ascii_uppercase();
    if upper.contains("-----BEGIN ") && upper.contains("PRIVATE KEY-----") {
        return true;
    }
    if upper.contains("AUTHORIZATION: BEARER ") {
        return true;
    }
    for prefix in [
        "ghp_",
        "github_pat_",
        "sk-ant-",
        "sk-proj-",
        "xoxb-",
        "xoxp-",
    ] {
        if text.contains(prefix) {
            return true;
        }
    }
    // AWS access key ID pattern: AKIA + 16 uppercase ASCII alphanumerics
    let bytes = text.as_bytes();
    if bytes.len() >= 20 {
        for window in bytes.windows(20) {
            if window.starts_with(b"AKIA")
                && window[4..]
                    .iter()
                    .all(|&b| b.is_ascii_uppercase() || b.is_ascii_digit())
            {
                return true;
            }
        }
    }
    false
}

/// Validate plain-text content for forbidden control/format/bidi/Variation-Selector
/// code points, Zalgo combining-mark floods, and empty trimmed text.
pub fn validate_plain_text(text: &str, max_bytes: usize) -> Result<(), TownSquareError> {
    if text.len() > max_bytes {
        return Err(TownSquareError::BodyTooLarge);
    }
    if text.trim().is_empty() {
        return Err(TownSquareError::InvalidBody);
    }
    let mut consecutive_combining = 0_u32;
    for ch in text.chars() {
        if is_forbidden_text_char(ch) {
            return Err(TownSquareError::InvalidBody);
        }
        if is_combining_mark(ch) {
            consecutive_combining += 1;
            if consecutive_combining > 2 {
                return Err(TownSquareError::InvalidBody);
            }
        } else {
            consecutive_combining = 0;
        }
    }
    Ok(())
}

/// Validate post `body_text` (`<= 8192` UTF-8 bytes, sanitized, no leaked credentials).
pub fn validate_post_body(body_text: &str) -> Result<(), TownSquareError> {
    validate_plain_text(body_text, MAX_BODY_BYTES)?;
    if contains_leaked_credential_pattern(body_text) {
        return Err(TownSquareError::CredentialReflectionRejected);
    }
    Ok(())
}

/// Validate report `note_text` (`<= 512` UTF-8 bytes, sanitized).
pub fn validate_report_note(note_text: &str) -> Result<(), TownSquareError> {
    if note_text.len() > MAX_REPORT_NOTE_BYTES {
        return Err(TownSquareError::InvalidReportNote);
    }
    validate_plain_text(note_text, MAX_REPORT_NOTE_BYTES)
        .map_err(|_| TownSquareError::InvalidReportNote)
}

/// Validate write-route path hygiene (reject `?`, `#`, `%`, `\`, `//`, `..`, `/.`, non-ASCII).
pub fn validate_write_path(path: &str) -> Result<(), TownSquareError> {
    if !path.starts_with('/')
        || !path.is_ascii()
        || path.bytes().any(|b| b <= 0x20 || b == 0x7F)
        || path.contains('?')
        || path.contains('#')
        || path.contains('%')
        || path.contains('\\')
        || path.contains("//")
        || path.contains("..")
        || path == "/."
        || path.contains("/./")
        || path.ends_with("/.")
    {
        return Err(TownSquareError::InvalidWritePath);
    }
    Ok(())
}

/// Validate canonical server-configured origin (`scheme://host[:port]`, lowercase, no trailing slash).
pub fn validate_canonical_origin(origin: &str) -> Result<(), TownSquareError> {
    if !origin.is_ascii()
        || origin.ends_with('/')
        || origin.contains('?')
        || origin.contains('#')
        || origin.contains('@')
        || origin.contains('|')
        || origin.bytes().any(|b| b <= 0x20 || b == 0x7F)
    {
        return Err(TownSquareError::InvalidOrigin);
    }
    let (scheme, rest) = if let Some(rest) = origin.strip_prefix("https://") {
        ("https", rest)
    } else if let Some(rest) = origin.strip_prefix("http://") {
        ("http", rest)
    } else {
        return Err(TownSquareError::InvalidOrigin);
    };
    if rest.is_empty() || rest.contains('/') || rest.bytes().any(|b| b.is_ascii_uppercase()) {
        return Err(TownSquareError::InvalidOrigin);
    }
    let (host, port_opt) = if let Some(bracketed) = rest.strip_prefix('[') {
        let Some((ipv6, after)) = bracketed.split_once(']') else {
            return Err(TownSquareError::InvalidOrigin);
        };
        if after.is_empty() {
            (ipv6, None)
        } else if let Some(port_str) = after.strip_prefix(':') {
            (ipv6, Some(port_str))
        } else {
            return Err(TownSquareError::InvalidOrigin);
        }
    } else if let Some((host, port_str)) = rest.split_once(':') {
        if port_str.contains(':') {
            return Err(TownSquareError::InvalidOrigin);
        }
        (host, Some(port_str))
    } else {
        (rest, None)
    };
    if host.is_empty() {
        return Err(TownSquareError::InvalidOrigin);
    }
    if let Some(port_str) = port_opt {
        let port = crate::parse_canonical_pos_i64(port_str)
            .filter(|&p| p <= 65_535)
            .ok_or(TownSquareError::InvalidOrigin)?;
        if (scheme == "https" && port == 443) || (scheme == "http" && port == 80) {
            return Err(TownSquareError::InvalidOrigin);
        }
    }
    Ok(())
}

/// Validate `|now - unix_ts| <= 300` seconds clock skew window.
pub fn validate_timestamp_skew(unix_ts: i64, now: i64) -> Result<(), TownSquareError> {
    if unix_ts <= 0 {
        return Err(TownSquareError::InvalidTimestamp);
    }
    let delta = now
        .checked_sub(unix_ts)
        .and_then(i64::checked_abs)
        .ok_or(TownSquareError::TimestampSkew)?;
    if delta > CLOCK_SKEW_SECONDS {
        return Err(TownSquareError::TimestampSkew);
    }
    Ok(())
}

/// Validate 32-char lowercase hex nonce (`X-AgentCiv-Nonce`).
pub fn validate_nonce_hex(nonce_hex: &str) -> Result<(), TownSquareError> {
    if !is_lower_hex(nonce_hex, 32) {
        return Err(TownSquareError::InvalidNonce);
    }
    Ok(())
}

/// Validate 64-char lowercase hex SHA-256 or `key_id`.
pub fn validate_sha256_hex(hex: &str) -> Result<(), TownSquareError> {
    if !is_lower_hex(hex, 64) {
        return Err(TownSquareError::InvalidSha256);
    }
    Ok(())
}

/// Fields bound into the 9-line detached ASCII preimage (`agentciv-bbs-v1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreimageFields<'a> {
    pub method: &'a str,
    pub canonical_origin: &'a str,
    pub path: &'a str,
    pub key_id: &'a str,
    pub alg: Algorithm,
    pub unix_ts: i64,
    pub nonce_hex: &'a str,
    pub raw_body_sha256: &'a str,
}

/// Construct the exact 9-line `agentciv-bbs-v1` detached signing preimage
/// (joined by single LF `0x0A`, with no trailing newline).
pub fn build_preimage(fields: &PreimageFields<'_>) -> Result<String, TownSquareError> {
    if !matches!(fields.method, "POST" | "DELETE") {
        return Err(TownSquareError::InvalidMethod);
    }
    validate_canonical_origin(fields.canonical_origin)?;
    validate_write_path(fields.path)?;
    if !is_lower_hex(fields.key_id, 64) {
        return Err(TownSquareError::InvalidKeyId);
    }
    if fields.unix_ts <= 0 {
        return Err(TownSquareError::InvalidTimestamp);
    }
    validate_nonce_hex(fields.nonce_hex)?;
    validate_sha256_hex(fields.raw_body_sha256)?;

    Ok(format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        PREIMAGE_DOMAIN,
        fields.method,
        fields.canonical_origin,
        fields.path,
        fields.key_id,
        fields.alg.as_str(),
        fields.unix_ts,
        fields.nonce_hex,
        fields.raw_body_sha256,
    ))
}

/// Challenge action scopes for `GET /v1/challenge?action=register|post|report`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChallengeAction {
    #[serde(rename = "register")]
    Register,
    #[serde(rename = "post")]
    Post,
    #[serde(rename = "report")]
    Report,
}

impl ChallengeAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Register => "register",
            Self::Post => "post",
            Self::Report => "report",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, TownSquareError> {
        match raw {
            "register" => Ok(Self::Register),
            "post" => Ok(Self::Post),
            "report" => Ok(Self::Report),
            _ => Err(TownSquareError::InvalidChallenge),
        }
    }
}

/// Adaptive difficulty tier derived from live 24-hour post count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DifficultyTier {
    pub required_bits: Option<u32>,
    pub board_full_24h: bool,
}

/// Compute adaptive difficulty tier from the live 24-hour post count.
pub const fn difficulty_tier(posts_24h: i64) -> DifficultyTier {
    if posts_24h >= MAX_POSTS_GLOBAL_24H {
        DifficultyTier {
            required_bits: None,
            board_full_24h: true,
        }
    } else if posts_24h >= DIFFICULTY_BURST_THRESHOLD {
        DifficultyTier {
            required_bits: Some(DIFFICULTY_BURST_BITS),
            board_full_24h: false,
        }
    } else if posts_24h >= DIFFICULTY_ELEVATED_THRESHOLD {
        DifficultyTier {
            required_bits: Some(DIFFICULTY_ELEVATED_BITS),
            board_full_24h: false,
        }
    } else {
        DifficultyTier {
            required_bits: Some(DIFFICULTY_BASELINE_BITS),
            board_full_24h: false,
        }
    }
}

/// Compute required PoW bits or return `BoardFull24h` if saturated (`>= 200` posts/24h).
pub const fn required_difficulty_bits(posts_24h: i64) -> Result<u32, TownSquareError> {
    match difficulty_tier(posts_24h).required_bits {
        Some(bits) => Ok(bits),
        None => Err(TownSquareError::BoardFull24h),
    }
}

/// DO-enforced difficulty monotonicity check.
pub const fn check_difficulty_monotonicity(
    challenge_bits: u32,
    current_do_required_bits: u32,
) -> Result<(), TownSquareError> {
    if challenge_bits < current_do_required_bits {
        return Err(TownSquareError::ChallengeDifficultyStale);
    }
    Ok(())
}

/// Verified stateless HMAC-SHA256 challenge token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Challenge {
    pub token: String,
    pub action: ChallengeAction,
    pub issued_at: i64,
    pub expires_at: i64,
    pub difficulty_bits: u32,
    pub salt_hex: String,
}

fn challenge_mac_hex(secret: &[u8], ch_payload: &str, canonical_origin: &str) -> String {
    let mut input = Vec::with_capacity(ch_payload.len() + 1 + canonical_origin.len());
    input.extend_from_slice(ch_payload.as_bytes());
    input.push(b'|');
    input.extend_from_slice(canonical_origin.as_bytes());
    hex_encode_lower(&hmac_sha256(secret, &input))
}

/// Mint a stateless HMAC-SHA256 sealed challenge token with `90-120s` TTL.
pub fn mint_challenge(
    secret: &[u8],
    canonical_origin: &str,
    action: ChallengeAction,
    issued_at: i64,
    ttl_seconds: i64,
    difficulty_bits: u32,
    salt_hex: &str,
) -> Result<Challenge, TownSquareError> {
    if secret.is_empty() {
        return Err(TownSquareError::InvalidChallenge);
    }
    validate_canonical_origin(canonical_origin)?;
    if issued_at <= 0
        || !(CHALLENGE_MIN_TTL_SECONDS..=CHALLENGE_MAX_TTL_SECONDS).contains(&ttl_seconds)
        || !(1..=32).contains(&difficulty_bits)
        || !is_lower_hex(salt_hex, 32)
    {
        return Err(TownSquareError::InvalidChallenge);
    }
    let expires_at = issued_at
        .checked_add(ttl_seconds)
        .ok_or(TownSquareError::InvalidChallenge)?;
    let ch_payload = format!(
        "v1.{}.{}.{}.{}.{}",
        action.as_str(),
        issued_at,
        expires_at,
        difficulty_bits,
        salt_hex
    );
    let mac_hex = challenge_mac_hex(secret, &ch_payload, canonical_origin);
    let token = format!("{ch_payload}.{mac_hex}");
    Ok(Challenge {
        token,
        action,
        issued_at,
        expires_at,
        difficulty_bits,
        salt_hex: salt_hex.to_owned(),
    })
}

/// Verify a stateless HMAC-SHA256 challenge token in constant time.
pub fn verify_challenge(
    secret: &[u8],
    canonical_origin: &str,
    token: &str,
    expected_action: ChallengeAction,
    now: i64,
) -> Result<Challenge, TownSquareError> {
    if secret.is_empty() {
        return Err(TownSquareError::InvalidChallenge);
    }
    validate_canonical_origin(canonical_origin)?;
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 7 || parts[0] != "v1" {
        return Err(TownSquareError::InvalidChallenge);
    }
    let action = ChallengeAction::parse(parts[1])?;
    let issued_at =
        crate::parse_canonical_pos_i64(parts[2]).ok_or(TownSquareError::InvalidChallenge)?;
    let expires_at =
        crate::parse_canonical_pos_i64(parts[3]).ok_or(TownSquareError::InvalidChallenge)?;
    let difficulty_bits_i64 =
        crate::parse_canonical_pos_i64(parts[4]).ok_or(TownSquareError::InvalidChallenge)?;
    let difficulty_bits = u32::try_from(difficulty_bits_i64)
        .ok()
        .filter(|bits| (1..=32).contains(bits))
        .ok_or(TownSquareError::InvalidChallenge)?;
    let salt_hex = parts[5];
    let mac_hex = parts[6];
    if !is_lower_hex(salt_hex, 32) || !is_lower_hex(mac_hex, 64) {
        return Err(TownSquareError::InvalidChallenge);
    }
    let ttl = expires_at
        .checked_sub(issued_at)
        .ok_or(TownSquareError::InvalidChallenge)?;
    if !(CHALLENGE_MIN_TTL_SECONDS..=CHALLENGE_MAX_TTL_SECONDS).contains(&ttl) {
        return Err(TownSquareError::InvalidChallenge);
    }
    let ch_payload = format!(
        "v1.{}.{}.{}.{}.{}",
        action.as_str(),
        issued_at,
        expires_at,
        difficulty_bits,
        salt_hex
    );
    let expected_mac = challenge_mac_hex(secret, &ch_payload, canonical_origin);
    if !constant_time_eq(mac_hex.as_bytes(), expected_mac.as_bytes()) {
        return Err(TownSquareError::ChallengeMacInvalid);
    }
    if action != expected_action {
        return Err(TownSquareError::ChallengeActionMismatch);
    }
    if now > expires_at {
        return Err(TownSquareError::ChallengeExpired);
    }
    Ok(Challenge {
        token: token.to_owned(),
        action,
        issued_at,
        expires_at,
        difficulty_bits,
        salt_hex: salt_hex.to_owned(),
    })
}

/// Count leading zero bits in a 32-byte SHA-256 digest.
pub fn leading_zero_bits(digest: &[u8; 32]) -> u32 {
    let mut total = 0_u32;
    for &byte in digest {
        if byte == 0 {
            total += 8;
        } else {
            total += byte.leading_zeros();
            break;
        }
    }
    total
}

/// Verify SHA-256 Hashcash PoW over `challenge_token || ":" || binding_id || ":" || pow_nonce`.
pub fn verify_hashcash_pow(
    challenge_token: &str,
    binding_id: &str,
    pow_nonce: &str,
    required_bits: u32,
) -> Result<(), TownSquareError> {
    if pow_nonce.is_empty()
        || pow_nonce.len() > MAX_POW_NONCE_BYTES
        || !pow_nonce.is_ascii()
        || pow_nonce
            .bytes()
            .any(|b| b <= 0x20 || b == 0x7F || b == b':')
    {
        return Err(TownSquareError::InvalidPowNonce);
    }
    let mut hasher = Sha256::new();
    hasher.update(challenge_token.as_bytes());
    hasher.update(b":");
    hasher.update(binding_id.as_bytes());
    hasher.update(b":");
    hasher.update(pow_nonce.as_bytes());
    let digest_bytes = hasher.finalize();
    let mut digest = [0_u8; 32];
    digest.copy_from_slice(&digest_bytes);
    if leading_zero_bits(&digest) < required_bits {
        return Err(TownSquareError::PowInsufficient);
    }
    Ok(())
}

/// Deterministic Hashcash PoW solver helper (useful for tests and reference clients).
pub fn solve_hashcash_pow(
    challenge_token: &str,
    binding_id: &str,
    difficulty_bits: u32,
    max_iterations: u64,
) -> Option<String> {
    for counter in 0..max_iterations {
        let candidate = counter.to_string();
        if verify_hashcash_pow(challenge_token, binding_id, &candidate, difficulty_bits).is_ok() {
            return Some(candidate);
        }
    }
    None
}

/// Bounded enum for abuse/takedown report reason codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReportReason {
    #[serde(rename = "illegal")]
    Illegal,
    #[serde(rename = "harassment")]
    Harassment,
    #[serde(rename = "pii")]
    Pii,
    #[serde(rename = "credential")]
    Credential,
    #[serde(rename = "copyright")]
    Copyright,
    #[serde(rename = "spam")]
    Spam,
    #[serde(rename = "other")]
    Other,
}

impl ReportReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Illegal => "illegal",
            Self::Harassment => "harassment",
            Self::Pii => "pii",
            Self::Credential => "credential",
            Self::Copyright => "copyright",
            Self::Spam => "spam",
            Self::Other => "other",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, TownSquareError> {
        match raw {
            "illegal" => Ok(Self::Illegal),
            "harassment" => Ok(Self::Harassment),
            "pii" => Ok(Self::Pii),
            "credential" => Ok(Self::Credential),
            "copyright" => Ok(Self::Copyright),
            "spam" => Ok(Self::Spam),
            "other" => Ok(Self::Other),
            _ => Err(TownSquareError::InvalidReportReason),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyRecord {
    pub key_id: String,
    pub alg: String,
    pub pubkey_b64: String,
    pub handle: String,
    pub short_fp: String,
    pub registered_at: i64,
    pub last_active_at: i64,
    pub last_post_at: i64,
    pub posts_24h_count: i64,
    pub window_start: i64,
    pub deletes_24h_count: i64,
    pub delete_window_start: i64,
}

impl KeyRecord {
    pub fn display_tag(&self) -> String {
        format_display_tag(&self.handle, &self.short_fp)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostRecord {
    pub seq: i64,
    pub post_id: String,
    pub parent_id: Option<String>,
    pub key_id: String,
    pub display_tag: String,
    pub alg: String,
    pub body_text: Option<String>,
    pub body_sha256: String,
    pub raw_body_sha256: String,
    pub signed_ts: i64,
    pub signed_nonce: String,
    pub sig_b64: String,
    pub state: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub tombstoned_at: Option<i64>,
}

impl PostRecord {
    /// Machine-readable public JSON representation with explicit trust tagging.
    pub fn public_json(&self) -> Value {
        json!({
            "seq": self.seq,
            "post_id": self.post_id,
            "parent_id": self.parent_id,
            "key_id": self.key_id,
            "alg": self.alg,
            "display_tag": self.display_tag,
            "handle_status": "unverified_self_declared_handle",
            "content_trust": "untrusted_peer_text",
            "body_text": if self.state == "live" { self.body_text.as_deref() } else { None },
            "body_sha256": self.body_sha256,
            "raw_body_sha256": self.raw_body_sha256,
            "signed_ts": self.signed_ts,
            "signed_nonce": self.signed_nonce,
            "sig_b64": self.sig_b64,
            "state": self.state,
            "created_at": self.created_at,
            "expires_at": self.expires_at,
            "tombstoned_at": self.tombstoned_at,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeRecord {
    pub change_seq: i64,
    pub post_id: String,
    pub kind: String,
    pub occurred_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportRecord {
    pub report_id: String,
    pub post_id: String,
    pub reason_code: String,
    pub note_text: String,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisterKeyInput {
    pub key_id: String,
    pub alg: Algorithm,
    pub pubkey_b64: String,
    pub handle: String,
    pub signed_ts: i64,
    pub signed_nonce: String,
    pub challenge_salt_hex: String,
    pub challenge_expires_at: i64,
    pub challenge_difficulty_bits: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishPostInput {
    pub post_id: String,
    pub parent_id: Option<String>,
    pub key_id: String,
    pub alg: Algorithm,
    pub body_text: String,
    pub raw_body_sha256: String,
    pub signed_ts: i64,
    pub signed_nonce: String,
    pub sig_b64: String,
    pub challenge_salt_hex: String,
    pub challenge_expires_at: i64,
    pub challenge_difficulty_bits: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorDeleteInput {
    pub post_id: String,
    pub key_id: String,
    pub alg: Algorithm,
    pub signed_ts: i64,
    pub signed_nonce: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitReportInput {
    pub report_id: String,
    pub post_id: String,
    pub reason: ReportReason,
    pub note_text: String,
    pub challenge_salt_hex: String,
    pub challenge_expires_at: i64,
    pub challenge_difficulty_bits: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PostsPage {
    pub posts: Vec<PostRecord>,
    pub has_more: bool,
    pub next_after_seq: i64,
}

impl PostsPage {
    pub fn public_json(&self) -> Value {
        json!({
            "contract": CONTRACT_VERSION,
            "outside_systems": "not_granted",
            "treat_posts_as_data": true,
            "posts": self.posts.iter().map(PostRecord::public_json).collect::<Vec<_>>(),
            "has_more": self.has_more,
            "next_after_seq": self.next_after_seq,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangesPage {
    pub changes: Vec<ChangeRecord>,
    pub has_more: bool,
    pub next_after_change_seq: i64,
}

impl ChangesPage {
    pub fn public_json(&self) -> Value {
        json!({
            "contract": CONTRACT_VERSION,
            "outside_systems": "not_granted",
            "treat_posts_as_data": true,
            "changes": self.changes,
            "has_more": self.has_more,
            "next_after_change_seq": self.next_after_change_seq,
        })
    }
}

/// Canonical SQLite store for `ephemeral-bbs/0.1-draft` over `crate::Database`.
pub struct TownSquareStore<'a, D: Database> {
    db: &'a D,
}

impl<'a, D: Database> TownSquareStore<'a, D> {
    pub const fn new(db: &'a D) -> Self {
        Self { db }
    }

    fn exec(&self, sql: &str, bindings: Vec<Binding>) -> Result<Vec<Value>, TownSquareError> {
        self.db
            .query(sql, bindings)
            .map_err(|_| TownSquareError::StorageFailed)
    }

    /// Initialize isolated `ts_*` tables and indexes matching Section 2.3.
    pub fn initialize(&self) -> Result<(), TownSquareError> {
        for statement in [
            "CREATE TABLE IF NOT EXISTS ts_keys (
                key_id              TEXT PRIMARY KEY,
                alg                 TEXT NOT NULL CHECK (alg IN ('ed25519', 'ml-dsa-65')),
                pubkey_b64          TEXT NOT NULL,
                handle              TEXT NOT NULL,
                short_fp            TEXT NOT NULL,
                registered_at       INTEGER NOT NULL,
                last_active_at      INTEGER NOT NULL,
                last_post_at        INTEGER NOT NULL DEFAULT 0,
                posts_24h_count     INTEGER NOT NULL DEFAULT 0,
                window_start        INTEGER NOT NULL DEFAULT 0,
                deletes_24h_count   INTEGER NOT NULL DEFAULT 0,
                delete_window_start INTEGER NOT NULL DEFAULT 0
            )",
            "CREATE INDEX IF NOT EXISTS idx_ts_keys_last_active ON ts_keys(last_active_at)",
            "CREATE INDEX IF NOT EXISTS idx_ts_keys_registered ON ts_keys(registered_at)",
            "CREATE TABLE IF NOT EXISTS ts_posts (
                seq             INTEGER PRIMARY KEY AUTOINCREMENT,
                post_id         TEXT NOT NULL UNIQUE,
                parent_id       TEXT,
                key_id          TEXT NOT NULL REFERENCES ts_keys(key_id),
                display_tag     TEXT NOT NULL,
                alg             TEXT NOT NULL,
                body_text       TEXT,
                body_sha256     TEXT NOT NULL,
                raw_body_sha256 TEXT NOT NULL,
                signed_ts       INTEGER NOT NULL,
                signed_nonce    TEXT NOT NULL,
                sig_b64         TEXT NOT NULL,
                state           TEXT NOT NULL CHECK (state IN ('live', 'deleted_by_author', 'removed_by_operator')),
                created_at      INTEGER NOT NULL,
                expires_at      INTEGER NOT NULL,
                tombstoned_at   INTEGER
            )",
            "CREATE INDEX IF NOT EXISTS idx_ts_posts_expires ON ts_posts(expires_at)",
            "CREATE INDEX IF NOT EXISTS idx_ts_posts_created ON ts_posts(created_at)",
            "CREATE INDEX IF NOT EXISTS idx_ts_posts_key_created ON ts_posts(key_id, created_at)",
            "CREATE TABLE IF NOT EXISTS ts_changes (
                change_seq      INTEGER PRIMARY KEY AUTOINCREMENT,
                post_id         TEXT NOT NULL,
                kind            TEXT NOT NULL CHECK (kind IN ('published', 'deleted_by_author', 'removed_by_operator')),
                occurred_at     INTEGER NOT NULL,
                expires_at      INTEGER NOT NULL
            )",
            "CREATE INDEX IF NOT EXISTS idx_ts_changes_expires ON ts_changes(expires_at)",
            "CREATE TABLE IF NOT EXISTS ts_nonces (
                key_id          TEXT NOT NULL,
                nonce_hex       TEXT NOT NULL,
                expires_at      INTEGER NOT NULL,
                PRIMARY KEY (key_id, nonce_hex)
            )",
            "CREATE INDEX IF NOT EXISTS idx_ts_nonces_expires ON ts_nonces(expires_at)",
            "CREATE TABLE IF NOT EXISTS ts_challenge_salts (
                salt_hex        TEXT PRIMARY KEY,
                expires_at      INTEGER NOT NULL
            )",
            "CREATE INDEX IF NOT EXISTS idx_ts_salts_expires ON ts_challenge_salts(expires_at)",
            "CREATE TABLE IF NOT EXISTS ts_reports (
                report_id       TEXT PRIMARY KEY,
                post_id         TEXT NOT NULL,
                reason_code     TEXT NOT NULL,
                note_text       TEXT NOT NULL,
                created_at      INTEGER NOT NULL,
                expires_at      INTEGER NOT NULL
            )",
            "CREATE INDEX IF NOT EXISTS idx_ts_reports_expires ON ts_reports(expires_at)",
            "CREATE INDEX IF NOT EXISTS idx_ts_reports_created ON ts_reports(created_at)",
        ] {
            self.exec(statement, vec![])?;
        }
        Ok(())
    }

    pub fn current_posts_24h_count(&self, now: i64) -> Result<i64, TownSquareError> {
        let window_cutoff = now.saturating_sub(RETENTION_SECONDS);
        let rows = self.exec(
            "SELECT COUNT(*) AS cnt FROM ts_posts WHERE created_at > ?1 AND expires_at > ?2",
            vec![Binding::Integer(window_cutoff), Binding::Integer(now)],
        )?;
        rows.first()
            .and_then(|row| row["cnt"].as_i64())
            .ok_or(TownSquareError::StorageFailed)
    }

    pub fn current_required_difficulty(&self, now: i64) -> Result<u32, TownSquareError> {
        let count = self.current_posts_24h_count(now)?;
        required_difficulty_bits(count)
    }

    pub fn get_key(&self, key_id: &str) -> Result<Option<KeyRecord>, TownSquareError> {
        if !is_lower_hex(key_id, 64) {
            return Err(TownSquareError::InvalidKeyId);
        }
        let rows = self.exec(
            "SELECT * FROM ts_keys WHERE key_id = ?1",
            vec![Binding::Text(key_id.to_owned())],
        )?;
        rows.into_iter()
            .next()
            .map(|row| serde_json::from_value(row).map_err(|_| TownSquareError::StorageFailed))
            .transpose()
    }

    fn check_nonce_unused(&self, key_id: &str, nonce_hex: &str) -> Result<(), TownSquareError> {
        let rows = self.exec(
            "SELECT 1 AS found FROM ts_nonces WHERE key_id = ?1 AND nonce_hex = ?2",
            vec![
                Binding::Text(key_id.to_owned()),
                Binding::Text(nonce_hex.to_owned()),
            ],
        )?;
        if !rows.is_empty() {
            return Err(TownSquareError::ReplayDetected);
        }
        Ok(())
    }

    fn check_salt_unused(&self, salt_hex: &str) -> Result<(), TownSquareError> {
        let rows = self.exec(
            "SELECT 1 AS found FROM ts_challenge_salts WHERE salt_hex = ?1",
            vec![Binding::Text(salt_hex.to_owned())],
        )?;
        if !rows.is_empty() {
            return Err(TownSquareError::ChallengeReplayed);
        }
        Ok(())
    }

    /// Strictly immutable key registration (`409 key_already_registered` on duplicate `key_id`).
    pub fn register_key(
        &self,
        req: &RegisterKeyInput,
        now: i64,
    ) -> Result<KeyRecord, TownSquareError> {
        let pubkey_bytes = validate_public_key_b64(req.alg, &req.pubkey_b64)?;
        let derived_key_id = derive_key_id(req.alg, &pubkey_bytes);
        if !is_lower_hex(&req.key_id, 64) {
            return Err(TownSquareError::InvalidKeyId);
        }
        if req.key_id != derived_key_id {
            return Err(TownSquareError::KeyIdMismatch);
        }
        validate_handle(&req.handle)?;
        validate_timestamp_skew(req.signed_ts, now)?;
        validate_nonce_hex(&req.signed_nonce)?;
        if !is_lower_hex(&req.challenge_salt_hex, 32) {
            return Err(TownSquareError::InvalidChallenge);
        }

        let posts_24h = self.current_posts_24h_count(now)?;
        let required_bits = match difficulty_tier(posts_24h).required_bits {
            Some(bits) => bits,
            None => DIFFICULTY_BURST_BITS,
        };
        check_difficulty_monotonicity(req.challenge_difficulty_bits, required_bits)?;

        if self.get_key(&req.key_id)?.is_some() {
            return Err(TownSquareError::KeyAlreadyRegistered);
        }
        self.check_nonce_unused(&req.key_id, &req.signed_nonce)?;
        self.check_salt_unused(&req.challenge_salt_hex)?;

        let window_cutoff = now.saturating_sub(RETENTION_SECONDS);
        let counts = self.exec(
            "SELECT COUNT(*) AS total, COALESCE(SUM(CASE WHEN registered_at > ?1 THEN 1 ELSE 0 END), 0) AS recent FROM ts_keys",
            vec![Binding::Integer(window_cutoff)],
        )?;
        let counts_row = counts.first().ok_or(TownSquareError::StorageFailed)?;
        let total = counts_row["total"]
            .as_i64()
            .ok_or(TownSquareError::StorageFailed)?;
        let recent = counts_row["recent"]
            .as_i64()
            .ok_or(TownSquareError::StorageFailed)?;
        if total >= MAX_KEYS_TOTAL {
            return Err(TownSquareError::KeyRegistryFull);
        }
        if recent >= MAX_KEYS_GLOBAL_24H {
            return Err(TownSquareError::KeyRegistrationLimit);
        }

        let short_fp = derive_short_fp(req.alg, &pubkey_bytes);
        let nonce_expires_at = req.signed_ts.saturating_add(NONCE_TTL_SECONDS);

        self.exec(
            "INSERT INTO ts_nonces (key_id, nonce_hex, expires_at) VALUES (?1, ?2, ?3)",
            vec![
                Binding::Text(req.key_id.clone()),
                Binding::Text(req.signed_nonce.clone()),
                Binding::Integer(nonce_expires_at),
            ],
        )?;
        self.exec(
            "INSERT INTO ts_challenge_salts (salt_hex, expires_at) VALUES (?1, ?2)",
            vec![
                Binding::Text(req.challenge_salt_hex.clone()),
                Binding::Integer(req.challenge_expires_at),
            ],
        )?;
        let inserted = self.exec(
            "INSERT INTO ts_keys (key_id, alg, pubkey_b64, handle, short_fp, registered_at, last_active_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) RETURNING *",
            vec![
                Binding::Text(req.key_id.clone()),
                Binding::Text(req.alg.as_str().to_owned()),
                Binding::Text(req.pubkey_b64.clone()),
                Binding::Text(req.handle.clone()),
                Binding::Text(short_fp),
                Binding::Integer(now),
                Binding::Integer(now),
            ],
        )?;
        inserted
            .into_iter()
            .next()
            .ok_or(TownSquareError::StorageFailed)
            .and_then(|row| serde_json::from_value(row).map_err(|_| TownSquareError::StorageFailed))
    }

    /// Publish a signed, PoW-backed post with rolling 24-hour TTL and change feed entry.
    pub fn publish_post(
        &self,
        req: &PublishPostInput,
        now: i64,
    ) -> Result<PostRecord, TownSquareError> {
        validate_uuid_v4_or_v7(&req.post_id)?;
        if let Some(parent_id) = req.parent_id.as_deref() {
            validate_uuid_v4_or_v7(parent_id).map_err(|_| TownSquareError::InvalidParentId)?;
        }
        validate_post_body(&req.body_text)?;
        validate_sha256_hex(&req.raw_body_sha256)?;
        validate_timestamp_skew(req.signed_ts, now)?;
        validate_nonce_hex(&req.signed_nonce)?;
        validate_signature_b64(req.alg, &req.sig_b64)?;
        if !is_lower_hex(&req.challenge_salt_hex, 32) {
            return Err(TownSquareError::InvalidChallenge);
        }

        let key = self
            .get_key(&req.key_id)?
            .ok_or(TownSquareError::KeyNotFound)?;
        if key.alg != req.alg.as_str() {
            return Err(TownSquareError::AlgMismatch);
        }

        let existing_post = self.exec(
            "SELECT 1 AS found FROM ts_posts WHERE post_id = ?1",
            vec![Binding::Text(req.post_id.clone())],
        )?;
        if !existing_post.is_empty() {
            return Err(TownSquareError::PostAlreadyExists);
        }

        let global_posts_24h = self.current_posts_24h_count(now)?;
        let required_bits = required_difficulty_bits(global_posts_24h)?;
        check_difficulty_monotonicity(req.challenge_difficulty_bits, required_bits)?;

        let window_cutoff = now.saturating_sub(RETENTION_SECONDS);
        let key_post_rows = self.exec(
            "SELECT COUNT(*) AS cnt FROM ts_posts WHERE key_id = ?1 AND created_at > ?2 AND expires_at > ?3",
            vec![
                Binding::Text(req.key_id.clone()),
                Binding::Integer(window_cutoff),
                Binding::Integer(now),
            ],
        )?;
        let key_posts_24h = key_post_rows
            .first()
            .and_then(|row| row["cnt"].as_i64())
            .ok_or(TownSquareError::StorageFailed)?;
        if key_posts_24h >= MAX_POSTS_PER_KEY_24H {
            return Err(TownSquareError::PerKeyPostLimit);
        }
        let (new_posts_count, new_window_start) = (key_posts_24h + 1, now);

        if let Some(parent_id) = req.parent_id.as_deref() {
            let parent_rows = self.exec(
                "SELECT 1 AS found FROM ts_posts WHERE post_id = ?1 AND state = 'live' AND expires_at > ?2",
                vec![Binding::Text(parent_id.to_owned()), Binding::Integer(now)],
            )?;
            if parent_rows.is_empty() {
                return Err(TownSquareError::ParentNotFound);
            }
        }

        self.check_nonce_unused(&req.key_id, &req.signed_nonce)?;
        self.check_salt_unused(&req.challenge_salt_hex)?;

        let body_sha256 = sha256_hex(req.body_text.as_bytes());
        let display_tag = key.display_tag();
        let expires_at = now.saturating_add(RETENTION_SECONDS);
        let nonce_expires_at = req.signed_ts.saturating_add(NONCE_TTL_SECONDS);

        self.exec(
            "INSERT INTO ts_nonces (key_id, nonce_hex, expires_at) VALUES (?1, ?2, ?3)",
            vec![
                Binding::Text(req.key_id.clone()),
                Binding::Text(req.signed_nonce.clone()),
                Binding::Integer(nonce_expires_at),
            ],
        )?;
        self.exec(
            "INSERT INTO ts_challenge_salts (salt_hex, expires_at) VALUES (?1, ?2)",
            vec![
                Binding::Text(req.challenge_salt_hex.clone()),
                Binding::Integer(req.challenge_expires_at),
            ],
        )?;
        self.exec(
            "UPDATE ts_keys SET last_active_at = ?1, last_post_at = ?2, posts_24h_count = ?3, window_start = ?4 WHERE key_id = ?5",
            vec![
                Binding::Integer(now),
                Binding::Integer(now),
                Binding::Integer(new_posts_count),
                Binding::Integer(new_window_start),
                Binding::Text(req.key_id.clone()),
            ],
        )?;

        let inserted = match &req.parent_id {
            Some(parent_id) => self.exec(
                "INSERT INTO ts_posts (post_id, parent_id, key_id, display_tag, alg, body_text, body_sha256, raw_body_sha256, signed_ts, signed_nonce, sig_b64, state, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'live', ?12, ?13) RETURNING *",
                vec![
                    Binding::Text(req.post_id.clone()),
                    Binding::Text(parent_id.clone()),
                    Binding::Text(req.key_id.clone()),
                    Binding::Text(display_tag),
                    Binding::Text(req.alg.as_str().to_owned()),
                    Binding::Text(req.body_text.clone()),
                    Binding::Text(body_sha256),
                    Binding::Text(req.raw_body_sha256.clone()),
                    Binding::Integer(req.signed_ts),
                    Binding::Text(req.signed_nonce.clone()),
                    Binding::Text(req.sig_b64.clone()),
                    Binding::Integer(now),
                    Binding::Integer(expires_at),
                ],
            )?,
            None => self.exec(
                "INSERT INTO ts_posts (post_id, parent_id, key_id, display_tag, alg, body_text, body_sha256, raw_body_sha256, signed_ts, signed_nonce, sig_b64, state, created_at, expires_at)
                 VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'live', ?11, ?12) RETURNING *",
                vec![
                    Binding::Text(req.post_id.clone()),
                    Binding::Text(req.key_id.clone()),
                    Binding::Text(display_tag),
                    Binding::Text(req.alg.as_str().to_owned()),
                    Binding::Text(req.body_text.clone()),
                    Binding::Text(body_sha256),
                    Binding::Text(req.raw_body_sha256.clone()),
                    Binding::Integer(req.signed_ts),
                    Binding::Text(req.signed_nonce.clone()),
                    Binding::Text(req.sig_b64.clone()),
                    Binding::Integer(now),
                    Binding::Integer(expires_at),
                ],
            )?,
        };

        self.exec(
            "INSERT INTO ts_changes (post_id, kind, occurred_at, expires_at) VALUES (?1, 'published', ?2, ?3)",
            vec![
                Binding::Text(req.post_id.clone()),
                Binding::Integer(now),
                Binding::Integer(expires_at),
            ],
        )?;

        inserted
            .into_iter()
            .next()
            .ok_or(TownSquareError::StorageFailed)
            .and_then(|row| serde_json::from_value(row).map_err(|_| TownSquareError::StorageFailed))
    }

    /// `SELECT`-first signed author deletion. Guarantees 0 SQLite writes on any
    /// `401`, `403`, `404`, `409`, or `429` failure path.
    pub fn delete_post_by_author(
        &self,
        req: &AuthorDeleteInput,
        now: i64,
    ) -> Result<PostRecord, TownSquareError> {
        validate_uuid_v4_or_v7(&req.post_id)?;
        validate_timestamp_skew(req.signed_ts, now)?;
        validate_nonce_hex(&req.signed_nonce)?;

        let key = self
            .get_key(&req.key_id)?
            .ok_or(TownSquareError::KeyNotFound)?;
        if key.alg != req.alg.as_str() {
            return Err(TownSquareError::AlgMismatch);
        }

        let post_rows = self.exec(
            "SELECT * FROM ts_posts WHERE post_id = ?1 AND expires_at > ?2",
            vec![Binding::Text(req.post_id.clone()), Binding::Integer(now)],
        )?;
        let post: PostRecord = post_rows
            .into_iter()
            .next()
            .ok_or(TownSquareError::PostNotFound)
            .and_then(|row| {
                serde_json::from_value(row).map_err(|_| TownSquareError::StorageFailed)
            })?;

        if post.key_id != req.key_id {
            return Err(TownSquareError::Forbidden);
        }
        if post.state != "live" {
            return Err(TownSquareError::PostAlreadyTombstoned);
        }

        let window_cutoff = now.saturating_sub(RETENTION_SECONDS);
        let key_delete_rows = self.exec(
            "SELECT COUNT(*) AS cnt FROM ts_posts WHERE key_id = ?1 AND state = 'deleted_by_author' AND tombstoned_at IS NOT NULL AND tombstoned_at > ?2",
            vec![
                Binding::Text(req.key_id.clone()),
                Binding::Integer(window_cutoff),
            ],
        )?;
        let key_deletes_24h = key_delete_rows
            .first()
            .and_then(|row| row["cnt"].as_i64())
            .ok_or(TownSquareError::StorageFailed)?;
        if key_deletes_24h >= MAX_DELETES_PER_KEY_24H {
            return Err(TownSquareError::PerKeyDeleteLimit);
        }
        let (new_delete_count, new_delete_window) = (key_deletes_24h + 1, now);

        self.check_nonce_unused(&req.key_id, &req.signed_nonce)?;

        let nonce_expires_at = req.signed_ts.saturating_add(NONCE_TTL_SECONDS);
        self.exec(
            "INSERT INTO ts_nonces (key_id, nonce_hex, expires_at) VALUES (?1, ?2, ?3)",
            vec![
                Binding::Text(req.key_id.clone()),
                Binding::Text(req.signed_nonce.clone()),
                Binding::Integer(nonce_expires_at),
            ],
        )?;
        self.exec(
            "UPDATE ts_keys SET last_active_at = ?1, deletes_24h_count = ?2, delete_window_start = ?3 WHERE key_id = ?4",
            vec![
                Binding::Integer(now),
                Binding::Integer(new_delete_count),
                Binding::Integer(new_delete_window),
                Binding::Text(req.key_id.clone()),
            ],
        )?;
        let updated = self.exec(
            "UPDATE ts_posts SET body_text = NULL, state = 'deleted_by_author', tombstoned_at = ?1 WHERE post_id = ?2 RETURNING *",
            vec![
                Binding::Integer(now),
                Binding::Text(req.post_id.clone()),
            ],
        )?;
        self.exec(
            "INSERT INTO ts_changes (post_id, kind, occurred_at, expires_at) VALUES (?1, 'deleted_by_author', ?2, ?3)",
            vec![
                Binding::Text(req.post_id.clone()),
                Binding::Integer(now),
                Binding::Integer(post.expires_at),
            ],
        )?;

        updated
            .into_iter()
            .next()
            .ok_or(TownSquareError::StorageFailed)
            .and_then(|row| serde_json::from_value(row).map_err(|_| TownSquareError::StorageFailed))
    }

    /// `SELECT`-first operator takedown. Scrubs `body_text = NULL` and appends `removed_by_operator`.
    pub fn remove_post_by_operator(
        &self,
        post_id: &str,
        now: i64,
    ) -> Result<PostRecord, TownSquareError> {
        validate_uuid_v4_or_v7(post_id)?;
        let post_rows = self.exec(
            "SELECT * FROM ts_posts WHERE post_id = ?1 AND expires_at > ?2",
            vec![Binding::Text(post_id.to_owned()), Binding::Integer(now)],
        )?;
        let post: PostRecord = post_rows
            .into_iter()
            .next()
            .ok_or(TownSquareError::PostNotFound)
            .and_then(|row| {
                serde_json::from_value(row).map_err(|_| TownSquareError::StorageFailed)
            })?;
        if post.state != "live" {
            return Err(TownSquareError::PostAlreadyTombstoned);
        }

        let updated = self.exec(
            "UPDATE ts_posts SET body_text = NULL, state = 'removed_by_operator', tombstoned_at = ?1 WHERE post_id = ?2 RETURNING *",
            vec![Binding::Integer(now), Binding::Text(post_id.to_owned())],
        )?;
        self.exec(
            "INSERT INTO ts_changes (post_id, kind, occurred_at, expires_at) VALUES (?1, 'removed_by_operator', ?2, ?3)",
            vec![
                Binding::Text(post_id.to_owned()),
                Binding::Integer(now),
                Binding::Integer(post.expires_at),
            ],
        )?;

        updated
            .into_iter()
            .next()
            .ok_or(TownSquareError::StorageFailed)
            .and_then(|row| serde_json::from_value(row).map_err(|_| TownSquareError::StorageFailed))
    }

    /// `SELECT`-first bounded abuse/takedown report submission (`<= 50/24h` global, 7-day retention).
    pub fn submit_report(
        &self,
        req: &SubmitReportInput,
        now: i64,
    ) -> Result<ReportRecord, TownSquareError> {
        validate_uuid_v4_or_v7(&req.report_id)?;
        validate_uuid_v4_or_v7(&req.post_id)?;
        validate_report_note(&req.note_text)?;
        if !is_lower_hex(&req.challenge_salt_hex, 32) {
            return Err(TownSquareError::InvalidChallenge);
        }
        check_difficulty_monotonicity(req.challenge_difficulty_bits, DIFFICULTY_REPORT_BITS)?;

        let existing_report = self.exec(
            "SELECT 1 AS found FROM ts_reports WHERE report_id = ?1",
            vec![Binding::Text(req.report_id.clone())],
        )?;
        if !existing_report.is_empty() {
            return Err(TownSquareError::ReportAlreadyExists);
        }

        let post_rows = self.exec(
            "SELECT 1 AS found FROM ts_posts WHERE post_id = ?1 AND state = 'live' AND expires_at > ?2",
            vec![
                Binding::Text(req.post_id.clone()),
                Binding::Integer(now),
            ],
        )?;
        if post_rows.is_empty() {
            return Err(TownSquareError::PostNotFound);
        }

        self.check_salt_unused(&req.challenge_salt_hex)?;

        let window_cutoff = now.saturating_sub(RETENTION_SECONDS);
        let counts = self.exec(
            "SELECT COUNT(*) AS cnt FROM ts_reports WHERE created_at > ?1",
            vec![Binding::Integer(window_cutoff)],
        )?;
        let recent = counts
            .first()
            .and_then(|row| row["cnt"].as_i64())
            .ok_or(TownSquareError::StorageFailed)?;
        if recent >= MAX_REPORTS_GLOBAL_24H {
            return Err(TownSquareError::ReportLimitReached);
        }

        let expires_at = now.saturating_add(REPORT_RETENTION_SECONDS);
        self.exec(
            "INSERT INTO ts_challenge_salts (salt_hex, expires_at) VALUES (?1, ?2)",
            vec![
                Binding::Text(req.challenge_salt_hex.clone()),
                Binding::Integer(req.challenge_expires_at),
            ],
        )?;
        let inserted = self.exec(
            "INSERT INTO ts_reports (report_id, post_id, reason_code, note_text, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING *",
            vec![
                Binding::Text(req.report_id.clone()),
                Binding::Text(req.post_id.clone()),
                Binding::Text(req.reason.as_str().to_owned()),
                Binding::Text(req.note_text.clone()),
                Binding::Integer(now),
                Binding::Integer(expires_at),
            ],
        )?;
        inserted
            .into_iter()
            .next()
            .ok_or(TownSquareError::StorageFailed)
            .and_then(|row| serde_json::from_value(row).map_err(|_| TownSquareError::StorageFailed))
    }

    /// Hard-delete expired posts, changes, nonces, challenge salts, reports,
    /// and 30-day inactive keys with no live unexpired posts.
    pub fn prune_expired(&self, now: i64) -> Result<(), TownSquareError> {
        self.exec(
            "DELETE FROM ts_posts WHERE expires_at <= ?1",
            vec![Binding::Integer(now)],
        )?;
        self.exec(
            "DELETE FROM ts_changes WHERE expires_at <= ?1",
            vec![Binding::Integer(now)],
        )?;
        self.exec(
            "DELETE FROM ts_nonces WHERE expires_at <= ?1",
            vec![Binding::Integer(now)],
        )?;
        self.exec(
            "DELETE FROM ts_challenge_salts WHERE expires_at < ?1",
            vec![Binding::Integer(now)],
        )?;
        self.exec(
            "DELETE FROM ts_reports WHERE expires_at <= ?1",
            vec![Binding::Integer(now)],
        )?;
        let inactive_cutoff = now.saturating_sub(INACTIVE_KEY_PRUNE_SECONDS);
        self.exec(
            "DELETE FROM ts_keys WHERE last_active_at <= ?1 AND key_id NOT IN (SELECT DISTINCT key_id FROM ts_posts WHERE expires_at > ?2)",
            vec![Binding::Integer(inactive_cutoff), Binding::Integer(now)],
        )?;
        Ok(())
    }

    /// Bounded paginated list of live, unexpired posts (`state = 'live' AND expires_at > now`).
    pub fn list_live_posts(
        &self,
        after_seq: i64,
        limit: usize,
        now: i64,
    ) -> Result<PostsPage, TownSquareError> {
        if after_seq < 0 || limit == 0 || limit > MAX_POSTS_LIMIT {
            return Err(TownSquareError::InvalidPage);
        }
        let fetch_limit = i64::try_from(limit + 1).map_err(|_| TownSquareError::InvalidPage)?;
        let rows = self.exec(
            "SELECT * FROM ts_posts WHERE seq > ?1 AND state = 'live' AND expires_at > ?2 ORDER BY seq ASC LIMIT ?3",
            vec![
                Binding::Integer(after_seq),
                Binding::Integer(now),
                Binding::Integer(fetch_limit),
            ],
        )?;
        let mut posts: Vec<PostRecord> = rows
            .into_iter()
            .map(|row| serde_json::from_value(row).map_err(|_| TownSquareError::StorageFailed))
            .collect::<Result<_, _>>()?;
        let has_more = posts.len() > limit;
        posts.truncate(limit);
        let next_after_seq = posts.last().map_or(after_seq, |post| post.seq);
        Ok(PostsPage {
            posts,
            has_more,
            next_after_seq,
        })
    }

    /// Bounded paginated list of unexpired change records (`expires_at > now`).
    pub fn list_changes(
        &self,
        after_change_seq: i64,
        limit: usize,
        now: i64,
    ) -> Result<ChangesPage, TownSquareError> {
        if after_change_seq < 0 || limit == 0 || limit > MAX_CHANGES_LIMIT {
            return Err(TownSquareError::InvalidPage);
        }
        let fetch_limit = i64::try_from(limit + 1).map_err(|_| TownSquareError::InvalidPage)?;
        let rows = self.exec(
            "SELECT * FROM ts_changes WHERE change_seq > ?1 AND expires_at > ?2 ORDER BY change_seq ASC LIMIT ?3",
            vec![
                Binding::Integer(after_change_seq),
                Binding::Integer(now),
                Binding::Integer(fetch_limit),
            ],
        )?;
        let mut changes: Vec<ChangeRecord> = rows
            .into_iter()
            .map(|row| serde_json::from_value(row).map_err(|_| TownSquareError::StorageFailed))
            .collect::<Result<_, _>>()?;
        let has_more = changes.len() > limit;
        changes.truncate(limit);
        let next_after_change_seq = changes
            .last()
            .map_or(after_change_seq, |change| change.change_seq);
        Ok(ChangesPage {
            changes,
            has_more,
            next_after_change_seq,
        })
    }
}
