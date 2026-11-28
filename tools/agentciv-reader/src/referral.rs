//! Offline structural and digest inspection for signed peer referrals.
//! Validates schema, canonical RFC 4648 base64, exact key/signature lengths,
//! referrer_key_id digest, UTC timestamp ordering, and signing preimage digest.
//! Does not perform cryptographic signature verification, network access, or
//! authority transfer.

use crate::{Error, Result};
use agentciv_archive::{exact_integer_field, parse_unique};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::sync::OnceLock;

/// Local byte ceiling for a single peer referral JSON document (32 KiB).
pub const MAX_REFERRAL_BYTES: usize = 32_768;

const SCHEMA: &str = include_str!("../../../schemas/peer-referral.schema.json");

fn validator() -> &'static jsonschema::Validator {
    static VALIDATOR: OnceLock<jsonschema::Validator> = OnceLock::new();
    VALIDATOR.get_or_init(|| {
        let schema: Value = serde_json::from_str(SCHEMA).expect("bundled referral schema parses");
        jsonschema::options()
            .offline()
            .should_validate_formats(true)
            .build(&schema)
            .expect("bundled referral schema compiles offline")
    })
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn b64_val(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Decode canonical RFC 4648 standard base64 (with `=` padding and zeroed trailing bits).
pub fn decode_canonical_base64(input: &str) -> Option<Vec<u8>> {
    let bytes = input.as_bytes();
    let (chunks, remainder) = bytes.as_chunks::<4>();
    if chunks.is_empty() || !remainder.is_empty() {
        return None;
    }
    let chunk_count = chunks.len();
    let mut out = Vec::with_capacity(chunk_count * 3);
    for (index, chunk) in chunks.iter().enumerate() {
        let is_last = index + 1 == chunk_count;
        let v0 = b64_val(chunk[0])?;
        let v1 = b64_val(chunk[1])?;
        if chunk[2] == b'=' && chunk[3] == b'=' {
            if !is_last || (v1 & 0x0f) != 0 {
                return None;
            }
            out.push((v0 << 2) | (v1 >> 4));
        } else if chunk[3] == b'=' {
            if !is_last {
                return None;
            }
            let v2 = b64_val(chunk[2])?;
            if (v2 & 0x03) != 0 {
                return None;
            }
            out.push((v0 << 2) | (v1 >> 4));
            out.push((v1 << 4) | (v2 >> 2));
        } else {
            let v2 = b64_val(chunk[2])?;
            let v3 = b64_val(chunk[3])?;
            out.push((v0 << 2) | (v1 >> 4));
            out.push((v1 << 4) | (v2 >> 2));
            out.push((v2 << 6) | v3);
        }
    }
    Some(out)
}

fn is_leap_year(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

fn parse_utc_rfc3339(value: &str) -> Option<(u32, u32, u32, u32, u32, u32)> {
    let bytes = value.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return None;
    }
    let year: u32 = value[0..4].parse().ok()?;
    let month: u32 = value[5..7].parse().ok()?;
    let day: u32 = value[8..10].parse().ok()?;
    let hour: u32 = value[11..13].parse().ok()?;
    let minute: u32 = value[14..16].parse().ok()?;
    let second: u32 = value[17..19].parse().ok()?;
    if !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    Some((year, month, day, hour, minute, second))
}

fn expected_lengths(alg: &str) -> Option<(usize, usize)> {
    match alg {
        "ed25519" => Some((32, 64)),
        "ml-dsa-65" => Some((1952, 3309)),
        "ed25519+ml-dsa-65" => Some((1984, 3373)),
        _ => None,
    }
}

/// Inspect one bounded peer referral JSON document offline.
/// Validates schema, exact integer retention, canonical base64, exact key/signature lengths,
/// `referrer_key_id` SHA-256 binding, strict UTC timestamp ordering, and computes the
/// 12-line LF-terminated `agentciv-peer-referral-v1` preimage digest.
/// Does not verify cryptographic signatures, contact endpoints, or grant authority.
pub fn inspect(raw: &str) -> Result<Value> {
    if raw.len() > MAX_REFERRAL_BYTES {
        return Err(Error::InputLimit);
    }
    let value = parse_unique(raw).map_err(|_| Error::InvalidJson)?;
    if !validator().is_valid(&value) {
        return Err(Error::InvalidReferral);
    }

    let referral_version = value["referral_version"]
        .as_str()
        .ok_or(Error::InvalidReferral)?;
    let referral_id = value["referral_id"]
        .as_str()
        .ok_or(Error::InvalidReferral)?;
    let content_trust = value["content_trust"]
        .as_str()
        .ok_or(Error::InvalidReferral)?;
    let outside_systems = value["outside_systems"]
        .as_str()
        .ok_or(Error::InvalidReferral)?;
    let world_id = value["world_id"].as_str().ok_or(Error::InvalidReferral)?;
    let profile = value["profile"].as_str().ok_or(Error::InvalidReferral)?;
    let endpoint_url = value["endpoint_url"]
        .as_str()
        .ok_or(Error::InvalidReferral)?;
    let issued_at = value["issued_at"].as_str().ok_or(Error::InvalidReferral)?;
    let expires_at = value["expires_at"].as_str().ok_or(Error::InvalidReferral)?;
    let alg = value["alg"].as_str().ok_or(Error::InvalidReferral)?;
    let referrer_key_id = value["referrer_key_id"]
        .as_str()
        .ok_or(Error::InvalidReferral)?;
    let referrer_pubkey_b64 = value["referrer_pubkey_b64"]
        .as_str()
        .ok_or(Error::InvalidReferral)?;
    let signature_b64 = value["signature_b64"]
        .as_str()
        .ok_or(Error::InvalidReferral)?;

    let retention_seconds =
        exact_integer_field(raw, &["retention_seconds"]).ok_or(Error::InvalidReferral)?;
    if !(60..=31_536_000).contains(&retention_seconds) {
        return Err(Error::InvalidReferral);
    }

    let issued_ts = parse_utc_rfc3339(issued_at).ok_or(Error::InvalidReferral)?;
    let expires_ts = parse_utc_rfc3339(expires_at).ok_or(Error::InvalidReferral)?;
    if expires_ts <= issued_ts {
        return Err(Error::InvalidReferral);
    }

    let (expected_pubkey_len, expected_sig_len) =
        expected_lengths(alg).ok_or(Error::InvalidReferral)?;
    let pubkey_bytes =
        decode_canonical_base64(referrer_pubkey_b64).ok_or(Error::InvalidReferral)?;
    let signature_bytes = decode_canonical_base64(signature_b64).ok_or(Error::InvalidReferral)?;
    if pubkey_bytes.len() != expected_pubkey_len || signature_bytes.len() != expected_sig_len {
        return Err(Error::InvalidReferral);
    }

    let mut key_id_hasher = Sha256::new();
    key_id_hasher.update(alg.as_bytes());
    key_id_hasher.update(b":");
    key_id_hasher.update(&pubkey_bytes);
    let computed_key_id = hex_lower(&key_id_hasher.finalize());
    if computed_key_id != referrer_key_id {
        return Err(Error::InvalidReferral);
    }

    let preimage = format!(
        "agentciv-peer-referral-v1\n{referral_id}\n{content_trust}\n{outside_systems}\n{world_id}\n{profile}\n{endpoint_url}\n{retention_seconds}\n{issued_at}\n{expires_at}\n{alg}\n{referrer_key_id}\n"
    );
    let preimage_sha256 = hex_lower(&Sha256::digest(preimage.as_bytes()));

    Ok(json!({
        "outcome": "valid",
        "referral_version": referral_version,
        "referral_id": referral_id,
        "content_trust": "untrusted_peer_referral",
        "outside_systems": "not_granted",
        "verification_scope": "structural_and_digest_only",
        "cryptographic_signature_verified": false,
        "authority_transferred": false,
        "world_id": world_id,
        "profile": profile,
        "endpoint_url": endpoint_url,
        "retention_seconds": retention_seconds,
        "issued_at": issued_at,
        "expires_at": expires_at,
        "alg": alg,
        "referrer_key_id": referrer_key_id,
        "pubkey_bytes": pubkey_bytes.len(),
        "signature_bytes": signature_bytes.len(),
        "preimage_sha256": preimage_sha256,
    }))
}
