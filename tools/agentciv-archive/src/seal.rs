//! Detached offline archive root seal contract, canonical root-digest builder,
//! ASCII preimage generator, and structural/digest verifier.
//!
//! Structural inspection verifies schema, duplicate-key rejection, canonical RFC 4648 base64,
//! algorithm key/signature byte lengths, `key_id` binding, and bundle/root digests.
//! It never claims cryptographic signature verification unless a `SealVerifier` backend is supplied.

use crate::json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt;
use std::sync::OnceLock;

pub const SEAL_VERSION: &str = "agentciv-archive-seal/0.1-draft";
pub const PREIMAGE_VERSION: &str = "agentciv-archive-seal-v1";
pub const ROOT_DIGEST_PREFIX: &str = "agentciv-archive-root-v1";
pub const MAX_SEAL_BYTES: usize = 64 * 1024;
pub const MAX_SIGNER_NOTE_BYTES: usize = 256;

const SEAL_SCHEMA: &str = include_str!("../../../schemas/archive-seal.schema.json");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SealAlgorithm {
    #[serde(rename = "ed25519")]
    Ed25519,
    #[serde(rename = "ml-dsa-65")]
    MlDsa65,
    #[serde(rename = "ed25519+ml-dsa-65")]
    Ed25519MlDsa65,
    #[serde(rename = "slh-dsa-sha2-128s")]
    SlhDsaSha2_128s,
}

impl SealAlgorithm {
    pub const ALL: [Self; 4] = [
        Self::Ed25519,
        Self::MlDsa65,
        Self::Ed25519MlDsa65,
        Self::SlhDsaSha2_128s,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ed25519 => "ed25519",
            Self::MlDsa65 => "ml-dsa-65",
            Self::Ed25519MlDsa65 => "ed25519+ml-dsa-65",
            Self::SlhDsaSha2_128s => "slh-dsa-sha2-128s",
        }
    }

    pub const fn pubkey_len(self) -> usize {
        match self {
            Self::Ed25519 => 32,
            Self::MlDsa65 => 1952,
            Self::Ed25519MlDsa65 => 1984,
            Self::SlhDsaSha2_128s => 32,
        }
    }

    pub const fn signature_len(self) -> usize {
        match self {
            Self::Ed25519 => 64,
            Self::MlDsa65 => 3309,
            Self::Ed25519MlDsa65 => 3373,
            Self::SlhDsaSha2_128s => 7856,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SealError {
    InvalidBundle(&'static str),
    SealTooLarge,
    InvalidJson,
    InvalidSealSchema,
    SignerNoteTooLarge,
    BundleDigestMismatch,
    RootDigestMismatch,
    InvalidBase64,
    InvalidPubkeyLength,
    InvalidSignatureLength,
    KeyIdMismatch,
    UnsupportedAlgorithm,
    CryptographicVerificationFailed,
}

impl SealError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidBundle(code) => code,
            Self::SealTooLarge => "seal_too_large",
            Self::InvalidJson => "invalid_json",
            Self::InvalidSealSchema => "invalid_seal_schema",
            Self::SignerNoteTooLarge => "signer_note_too_large",
            Self::BundleDigestMismatch => "bundle_digest_mismatch",
            Self::RootDigestMismatch => "root_digest_mismatch",
            Self::InvalidBase64 => "invalid_base64",
            Self::InvalidPubkeyLength => "invalid_pubkey_length",
            Self::InvalidSignatureLength => "invalid_signature_length",
            Self::KeyIdMismatch => "key_id_mismatch",
            Self::UnsupportedAlgorithm => "unsupported_algorithm",
            Self::CryptographicVerificationFailed => "cryptographic_verification_failed",
        }
    }
}

impl fmt::Display for SealError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for SealError {}

/// Pluggable cryptographic signature verifier for offline archive root seals.
///
/// Implementations verify `signature` over the exact ASCII `preimage` bytes using `pubkey`
/// under `alg`. Returning `Ok(())` attests only to cryptographic key binding for the preimage;
/// it does not grant authority, trust, or copying permission.
pub trait SealVerifier {
    fn verify(
        &self,
        alg: SealAlgorithm,
        pubkey: &[u8],
        preimage: &[u8],
        signature: &[u8],
    ) -> std::result::Result<(), SealError>;
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootDigestSummary {
    pub bundle_sha256: String,
    pub canonical_records_sha256: String,
    pub record_count: usize,
    pub root_digest_sha256: String,
    pub preimage_version: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveSeal {
    pub seal_version: String,
    pub bundle_sha256: String,
    pub root_digest_sha256: String,
    pub preimage_version: String,
    pub alg: SealAlgorithm,
    pub key_id: String,
    pub pubkey_b64: String,
    pub signature_b64: String,
    pub sealed_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signer_note: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SealInspectionReport {
    pub valid: bool,
    pub seal_version: String,
    pub preimage_version: String,
    pub alg: SealAlgorithm,
    pub key_id: String,
    pub sealed_at: String,
    pub bundle_sha256: String,
    pub canonical_records_sha256: String,
    pub record_count: usize,
    pub root_digest_sha256: String,
    pub verification_scope: String,
    pub cryptographic_signature_verified: bool,
    pub source_authenticity: String,
    pub authority_transferred: bool,
}

fn seal_validator() -> &'static jsonschema::Validator {
    static VALIDATOR: OnceLock<jsonschema::Validator> = OnceLock::new();
    VALIDATOR.get_or_init(|| {
        let schema: Value = serde_json::from_str(SEAL_SCHEMA).expect("bundled seal schema parses");
        jsonschema::options()
            .should_validate_formats(true)
            .build(&schema)
            .expect("bundled seal schema compiles")
    })
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex_lower(&Sha256::digest(bytes))
}

const B64_ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Encode raw bytes into canonical RFC 4648 standard base64 with `=` padding.
pub fn encode_base64(input: &[u8]) -> String {
    let mut output = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = if chunk.len() > 1 {
            u32::from(chunk[1])
        } else {
            0
        };
        let b2 = if chunk.len() > 2 {
            u32::from(chunk[2])
        } else {
            0
        };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        output.push(B64_ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
        output.push(B64_ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            output.push(B64_ALPHABET[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(B64_ALPHABET[(triple & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}

fn decode_b64_char(byte: u8) -> std::result::Result<u8, SealError> {
    match byte {
        b'A'..=b'Z' => Ok(byte - b'A'),
        b'a'..=b'z' => Ok(byte - b'a' + 26),
        b'0'..=b'9' => Ok(byte - b'0' + 52),
        b'+' => Ok(62),
        b'/' => Ok(63),
        _ => Err(SealError::InvalidBase64),
    }
}

/// Strictly decode canonical RFC 4648 standard base64 (with mandatory `=` padding,
/// zero non-canonical trailing bits, and no whitespace or URL-safe characters).
pub fn decode_base64_canonical(input: &str) -> std::result::Result<Vec<u8>, SealError> {
    let bytes = input.as_bytes();
    if bytes.is_empty() || !bytes.len().is_multiple_of(4) {
        return Err(SealError::InvalidBase64);
    }
    let mut output = Vec::with_capacity((bytes.len() / 4) * 3);
    let chunk_count = bytes.len() / 4;
    for (index, chunk) in bytes.as_chunks::<4>().0.iter().enumerate() {
        let is_last = index + 1 == chunk_count;
        let v0 = decode_b64_char(chunk[0])?;
        let v1 = decode_b64_char(chunk[1])?;
        match (chunk[2], chunk[3]) {
            (b'=', b'=') => {
                if !is_last || (v1 & 0x0f) != 0 {
                    return Err(SealError::InvalidBase64);
                }
                output.push((v0 << 2) | (v1 >> 4));
            }
            (c2, b'=') => {
                if !is_last {
                    return Err(SealError::InvalidBase64);
                }
                let v2 = decode_b64_char(c2)?;
                if (v2 & 0x03) != 0 {
                    return Err(SealError::InvalidBase64);
                }
                output.push((v0 << 2) | (v1 >> 4));
                output.push((v1 << 4) | (v2 >> 2));
            }
            (b'=', _) => return Err(SealError::InvalidBase64),
            (c2, c3) => {
                let v2 = decode_b64_char(c2)?;
                let v3 = decode_b64_char(c3)?;
                output.push((v0 << 2) | (v1 >> 4));
                output.push((v1 << 4) | (v2 >> 2));
                output.push((v2 << 6) | v3);
            }
        }
    }
    if encode_base64(&output) != input {
        return Err(SealError::InvalidBase64);
    }
    Ok(output)
}

/// Compute `key_id = hex(SHA-256(alg || ":" || pubkey_bytes))`.
///
/// For composite `ed25519+ml-dsa-65`, `pubkey_bytes` is the exact concatenation of
/// the 32-byte Ed25519 public key followed by the 1952-byte ML-DSA-65 public key.
pub fn compute_key_id(alg: SealAlgorithm, pubkey_bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(alg.as_str().as_bytes());
    hasher.update(b":");
    hasher.update(pubkey_bytes);
    hex_lower(&hasher.finalize())
}

/// Validate an archive bundle and compute its deterministic canonical root digest summary.
pub fn compute_root_digest(
    bundle_input: &str,
) -> std::result::Result<RootDigestSummary, SealError> {
    let bundle = crate::validate(bundle_input).map_err(SealError::InvalidBundle)?;
    let bundle_sha256 = sha256_hex(bundle_input.as_bytes());

    let mut pairs: Vec<(&str, &str)> = bundle
        .entries
        .iter()
        .map(|entry| (entry.event_id.as_str(), entry.sha256.as_str()))
        .collect();
    pairs.sort_unstable();

    let mut records_hasher = Sha256::new();
    for (event_id, record_sha256) in &pairs {
        records_hasher.update(event_id.len().to_string().as_bytes());
        records_hasher.update(b":");
        records_hasher.update(event_id.as_bytes());
        records_hasher.update(b":");
        records_hasher.update(record_sha256.as_bytes());
        records_hasher.update(b"\n");
    }
    let canonical_records_sha256 = hex_lower(&records_hasher.finalize());

    let record_count = bundle.entries.len();
    let root_preimage = format!(
        "{ROOT_DIGEST_PREFIX}\n{bundle_sha256}\n{canonical_records_sha256}\n{record_count}\n"
    );
    let root_digest_sha256 = sha256_hex(root_preimage.as_bytes());

    Ok(RootDigestSummary {
        bundle_sha256,
        canonical_records_sha256,
        record_count,
        root_digest_sha256,
        preimage_version: PREIMAGE_VERSION.to_owned(),
    })
}

/// Build the fixed-order ASCII seal preimage signed by a detached archive root seal.
///
/// Format:
/// `agentciv-archive-seal-v1\n<bundle_sha256>\n<root_digest_sha256>\n<alg>\n<key_id>\n<sealed_at>\n`
///
/// Callers must pass validated single-line fields (no embedded CR or LF), as enforced by
/// `inspect_seal_envelope` and `verify_seal_envelope`.
pub fn build_seal_preimage(
    bundle_sha256: &str,
    root_digest_sha256: &str,
    alg: SealAlgorithm,
    key_id: &str,
    sealed_at: &str,
) -> String {
    format!(
        "{PREIMAGE_VERSION}\n{bundle_sha256}\n{root_digest_sha256}\n{}\n{key_id}\n{sealed_at}\n",
        alg.as_str()
    )
}

struct VerifiedEnvelope {
    seal: ArchiveSeal,
    summary: RootDigestSummary,
    pubkey: Vec<u8>,
    signature: Vec<u8>,
    preimage: String,
}

fn check_envelope(
    bundle_input: &str,
    seal_input: &str,
) -> std::result::Result<VerifiedEnvelope, SealError> {
    if seal_input.len() > MAX_SEAL_BYTES {
        return Err(SealError::SealTooLarge);
    }
    let summary = compute_root_digest(bundle_input)?;
    let value = json::parse(seal_input).map_err(|_| SealError::InvalidJson)?;
    if !seal_validator().is_valid(&value) {
        return Err(SealError::InvalidSealSchema);
    }
    let seal: ArchiveSeal =
        serde_json::from_value(value).map_err(|_| SealError::InvalidSealSchema)?;
    if let Some(note) = &seal.signer_note
        && note.len() > MAX_SIGNER_NOTE_BYTES
    {
        return Err(SealError::SignerNoteTooLarge);
    }
    if seal.bundle_sha256 != summary.bundle_sha256 {
        return Err(SealError::BundleDigestMismatch);
    }
    if seal.root_digest_sha256 != summary.root_digest_sha256 {
        return Err(SealError::RootDigestMismatch);
    }
    let pubkey = decode_base64_canonical(&seal.pubkey_b64)?;
    if pubkey.len() != seal.alg.pubkey_len() {
        return Err(SealError::InvalidPubkeyLength);
    }
    let signature = decode_base64_canonical(&seal.signature_b64)?;
    if signature.len() != seal.alg.signature_len() {
        return Err(SealError::InvalidSignatureLength);
    }
    let expected_key_id = compute_key_id(seal.alg, &pubkey);
    if seal.key_id != expected_key_id {
        return Err(SealError::KeyIdMismatch);
    }
    let preimage = build_seal_preimage(
        &seal.bundle_sha256,
        &seal.root_digest_sha256,
        seal.alg,
        &seal.key_id,
        &seal.sealed_at,
    );
    Ok(VerifiedEnvelope {
        seal,
        summary,
        pubkey,
        signature,
        preimage,
    })
}

/// Perform structural, schema, digest, key-length, and `key_id` binding checks on a detached seal.
///
/// This function does NOT verify the cryptographic signature and explicitly reports
/// `verification_scope = "structural_and_digest_only"` and
/// `cryptographic_signature_verified = false`.
pub fn inspect_seal_envelope(
    bundle_input: &str,
    seal_input: &str,
) -> std::result::Result<SealInspectionReport, SealError> {
    let checked = check_envelope(bundle_input, seal_input)?;
    Ok(SealInspectionReport {
        valid: true,
        seal_version: checked.seal.seal_version,
        preimage_version: checked.seal.preimage_version,
        alg: checked.seal.alg,
        key_id: checked.seal.key_id,
        sealed_at: checked.seal.sealed_at,
        bundle_sha256: checked.summary.bundle_sha256,
        canonical_records_sha256: checked.summary.canonical_records_sha256,
        record_count: checked.summary.record_count,
        root_digest_sha256: checked.summary.root_digest_sha256,
        verification_scope: "structural_and_digest_only".to_owned(),
        cryptographic_signature_verified: false,
        source_authenticity: "unverified".to_owned(),
        authority_transferred: false,
    })
}

/// Perform structural/digest checks AND cryptographic signature verification via `verifier`.
///
/// A passing cryptographic check proves only that the signature matches the supplied public key
/// over the canonical preimage; it does not establish real-world identity or transfer authority.
pub fn verify_seal_envelope<V: SealVerifier + ?Sized>(
    bundle_input: &str,
    seal_input: &str,
    verifier: &V,
) -> std::result::Result<SealInspectionReport, SealError> {
    let checked = check_envelope(bundle_input, seal_input)?;
    verifier.verify(
        checked.seal.alg,
        &checked.pubkey,
        checked.preimage.as_bytes(),
        &checked.signature,
    )?;
    Ok(SealInspectionReport {
        valid: true,
        seal_version: checked.seal.seal_version,
        preimage_version: checked.seal.preimage_version,
        alg: checked.seal.alg,
        key_id: checked.seal.key_id,
        sealed_at: checked.seal.sealed_at,
        bundle_sha256: checked.summary.bundle_sha256,
        canonical_records_sha256: checked.summary.canonical_records_sha256,
        record_count: checked.summary.record_count,
        root_digest_sha256: checked.summary.root_digest_sha256,
        verification_scope: "structural_digest_and_signature".to_owned(),
        cryptographic_signature_verified: true,
        source_authenticity: "key_possession_verified_only".to_owned(),
        authority_transferred: false,
    })
}
