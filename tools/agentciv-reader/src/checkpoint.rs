//! Reader-side `c2sp.org/tlog-checkpoint` parsing, RFC 9162 Merkle consistency and
//! inclusion proof verification, and host equivocation / rollback detection.
//!
//! Unsigned checkpoints prove append-only log growth relative to a reader's own cached
//! checkpoint. Full split-view detection across independent readers requires independent
//! witness cosignatures or post-quantum signed checkpoints (tracked separately in #189).

use sha2::{Digest, Sha256};
use std::fmt;

const LEAF_DOMAIN: &[u8] = b"agentciv-merkle-leaf/0.1\0";
const RFC9162_LEAF_PREFIX: u8 = 0x00;
const RFC9162_NODE_PREFIX: u8 = 0x01;
const B64_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Parsed `c2sp.org/tlog-checkpoint` header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    pub origin: String,
    pub tree_size: u64,
    pub root_hash: [u8; 32],
}

/// Structured errors for checkpoint parsing, consistency proofs, and equivocation detection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckpointError {
    /// Note syntax, origin, decimal tree size, or base64 root hash is malformed.
    InvalidFormat,
    /// Cached and fresh checkpoints belong to different origins.
    OriginMismatch,
    /// Observed tree size is smaller than the cached tree size (`N < M`).
    Rollback {
        cached_size: u64,
        observed_size: u64,
    },
    /// Conflicting root hashes were observed for the same tree size (`N == M`, `root_N != root_M`).
    Equivocation { tree_size: u64 },
    /// RFC 9162 consistency or inclusion proof is malformed or fails verification.
    InvalidProof,
    /// Leaf index is out of bounds for the checkpoint tree size.
    InvalidIndex,
}

impl fmt::Display for CheckpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat => write!(f, "malformed c2sp.org/tlog-checkpoint note"),
            Self::OriginMismatch => write!(f, "checkpoint origin does not match cached origin"),
            Self::Rollback {
                cached_size,
                observed_size,
            } => write!(
                f,
                "host rollback detected: observed tree size {observed_size} < cached {cached_size}"
            ),
            Self::Equivocation { tree_size } => write!(
                f,
                "host equivocation detected: conflicting root hash at tree size {tree_size}"
            ),
            Self::InvalidProof => write!(f, "invalid RFC 9162 Merkle proof"),
            Self::InvalidIndex => write!(f, "leaf index out of range for checkpoint tree size"),
        }
    }
}

impl std::error::Error for CheckpointError {}

/// Compute SHA-256 of raw bytes.
pub fn sha256_bytes(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().into()
}

/// RFC 9162 empty tree root hash: `SHA-256("")`.
pub fn empty_root_hash() -> [u8; 32] {
    sha256_bytes(b"")
}

/// Compute tombstone-compatible RFC 9162 leaf hash:
/// `SHA-256(0x00 || LEAF_DOMAIN || len(event_id) || event_id || sequence || len(timestamp) || timestamp || original_record_sha256)`.
pub fn leaf_hash(
    event_id: &str,
    sequence: u64,
    timestamp: &str,
    original_record_sha256: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update([RFC9162_LEAF_PREFIX]);
    hasher.update(LEAF_DOMAIN);
    hasher.update((event_id.len() as u64).to_be_bytes());
    hasher.update(event_id.as_bytes());
    hasher.update(sequence.to_be_bytes());
    hasher.update((timestamp.len() as u64).to_be_bytes());
    hasher.update(timestamp.as_bytes());
    hasher.update(original_record_sha256);
    hasher.finalize().into()
}

/// Compute RFC 9162 interior node hash: `SHA-256(0x01 || left || right)`.
pub fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update([RFC9162_NODE_PREFIX]);
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

/// Encode 32-byte hash as 64-char lowercase hex.
pub fn encode_sha256_hex(hash: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in hash {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Parse 64-char hexadecimal SHA-256 hash.
pub fn parse_sha256_hex(hex: &str) -> Result<[u8; 32], CheckpointError> {
    if hex.len() != 64 {
        return Err(CheckpointError::InvalidProof);
    }
    let bytes = hex.as_bytes();
    let mut out = [0u8; 32];
    for (index, slot) in out.iter_mut().enumerate() {
        let hi = hex_nibble(bytes[index * 2]).ok_or(CheckpointError::InvalidProof)?;
        let lo = hex_nibble(bytes[index * 2 + 1]).ok_or(CheckpointError::InvalidProof)?;
        *slot = (hi << 4) | lo;
    }
    Ok(out)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Format a 3-line `c2sp.org/tlog-checkpoint` note body.
pub fn format_checkpoint(
    origin: &str,
    tree_size: u64,
    root_hash: &[u8; 32],
) -> Result<String, CheckpointError> {
    if !valid_origin(origin) {
        return Err(CheckpointError::InvalidFormat);
    }
    let b64 = encode_base64(root_hash);
    Ok(format!("{origin}\n{tree_size}\n{b64}\n"))
}

/// Parse a `c2sp.org/tlog-checkpoint` note body (or signed note whose body precedes `\n\n`).
pub fn parse_checkpoint(note: &str) -> Result<Checkpoint, CheckpointError> {
    let body = match note.split_once("\n\n") {
        Some((head, _sigs)) => head,
        None => note.strip_suffix('\n').unwrap_or(note),
    };
    let mut lines = body.split('\n');
    let origin = lines.next().ok_or(CheckpointError::InvalidFormat)?;
    let size_line = lines.next().ok_or(CheckpointError::InvalidFormat)?;
    let hash_line = lines.next().ok_or(CheckpointError::InvalidFormat)?;

    if !valid_origin(origin) {
        return Err(CheckpointError::InvalidFormat);
    }
    let tree_size = parse_canonical_u64(size_line).ok_or(CheckpointError::InvalidFormat)?;
    let decoded = decode_base64_32(hash_line).ok_or(CheckpointError::InvalidFormat)?;

    // Any remaining lines in the note body must be non-empty extension lines.
    for ext in lines {
        if ext.is_empty() {
            return Err(CheckpointError::InvalidFormat);
        }
    }

    Ok(Checkpoint {
        origin: origin.to_owned(),
        tree_size,
        root_hash: decoded,
    })
}

fn valid_origin(origin: &str) -> bool {
    !origin.is_empty()
        && !origin.bytes().any(|b| b.is_ascii_control() || b == b' ')
        && origin.len() <= 1024
}

fn parse_canonical_u64(raw: &str) -> Option<u64> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if raw.len() > 1 && raw.starts_with('0') {
        return None;
    }
    raw.parse::<u64>().ok()
}

fn encode_base64(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64_TABLE[((triple >> 18) & 0x3f) as usize] as char);
        out.push(B64_TABLE[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_TABLE[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64_TABLE[(triple & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Strictly decode a 44-char standard RFC 4648 base64 string (with `=` padding) into 32 bytes.
fn decode_base64_32(input: &str) -> Option<[u8; 32]> {
    let bytes = input.as_bytes();
    if bytes.len() != 44 || bytes[43] != b'=' {
        return None;
    }
    let mut out = [0u8; 32];
    // First 10 full 4-char blocks -> 30 bytes.
    for block in 0..10 {
        let base = block * 4;
        let v0 = b64_val(bytes[base])?;
        let v1 = b64_val(bytes[base + 1])?;
        let v2 = b64_val(bytes[base + 2])?;
        let v3 = b64_val(bytes[base + 3])?;
        let triple = ((v0 as u32) << 18) | ((v1 as u32) << 12) | ((v2 as u32) << 6) | (v3 as u32);
        let dst = block * 3;
        out[dst] = ((triple >> 16) & 0xff) as u8;
        out[dst + 1] = ((triple >> 8) & 0xff) as u8;
        out[dst + 2] = (triple & 0xff) as u8;
    }
    // Final 4-char block with one `=` pad -> 2 bytes; trailing 2 bits of v2 must be zero.
    let v0 = b64_val(bytes[40])?;
    let v1 = b64_val(bytes[41])?;
    let v2 = b64_val(bytes[42])?;
    if (v2 & 0x03) != 0 {
        return None;
    }
    let triple = ((v0 as u32) << 18) | ((v1 as u32) << 12) | ((v2 as u32) << 6);
    out[30] = ((triple >> 16) & 0xff) as u8;
    out[31] = ((triple >> 8) & 0xff) as u8;
    Some(out)
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

/// Verify that `fresh` is an append-only extension of `cached` using an RFC 9162
/// consistency proof, detecting rollback (`N < M`), same-size split-view equivocation
/// (`N == M` with `root_N != root_M`), and tampered consistency proofs.
pub fn verify_consistency(
    cached: &Checkpoint,
    fresh: &Checkpoint,
    proof: &[[u8; 32]],
) -> Result<(), CheckpointError> {
    if cached.origin != fresh.origin {
        return Err(CheckpointError::OriginMismatch);
    }
    let first_size = cached.tree_size;
    let second_size = fresh.tree_size;
    if second_size < first_size {
        return Err(CheckpointError::Rollback {
            cached_size: first_size,
            observed_size: second_size,
        });
    }
    if second_size == first_size {
        if cached.root_hash != fresh.root_hash {
            return Err(CheckpointError::Equivocation {
                tree_size: second_size,
            });
        }
        if !proof.is_empty() {
            return Err(CheckpointError::InvalidProof);
        }
        return Ok(());
    }
    if first_size == 0 {
        if cached.root_hash != empty_root_hash() {
            return Err(CheckpointError::Equivocation { tree_size: 0 });
        }
        if !proof.is_empty() {
            return Err(CheckpointError::InvalidProof);
        }
        return Ok(());
    }
    if proof.is_empty() {
        return Err(CheckpointError::InvalidProof);
    }

    let mut fn_idx = first_size - 1;
    let mut sn = second_size - 1;
    while (fn_idx & 1) == 1 {
        fn_idx >>= 1;
        sn >>= 1;
    }
    let (mut fr, mut sr, rest) = if first_size.is_power_of_two() {
        (cached.root_hash, cached.root_hash, proof)
    } else {
        (proof[0], proof[0], &proof[1..])
    };
    for node in rest {
        if sn == 0 {
            return Err(CheckpointError::InvalidProof);
        }
        if (fn_idx & 1) == 1 || fn_idx == sn {
            fr = node_hash(node, &fr);
            sr = node_hash(node, &sr);
            while fn_idx != 0 && (fn_idx & 1) == 0 {
                fn_idx >>= 1;
                sn >>= 1;
            }
        } else {
            sr = node_hash(&sr, node);
        }
        fn_idx >>= 1;
        sn >>= 1;
    }
    if sn == 0 && fr == cached.root_hash && sr == fresh.root_hash {
        Ok(())
    } else {
        Err(CheckpointError::InvalidProof)
    }
}

/// Convenience wrapper for `verify_consistency` accepting hex-encoded proof nodes.
pub fn verify_consistency_hex(
    cached: &Checkpoint,
    fresh: &Checkpoint,
    proof_hex: &[String],
) -> Result<(), CheckpointError> {
    let mut proof = Vec::with_capacity(proof_hex.len());
    for item in proof_hex {
        proof.push(parse_sha256_hex(item)?);
    }
    verify_consistency(cached, fresh, &proof)
}

/// Verify an RFC 9162 inclusion proof for `leaf` at `leaf_index` against `checkpoint`.
pub fn verify_inclusion(
    checkpoint: &Checkpoint,
    leaf_index: u64,
    leaf: &[u8; 32],
    proof: &[[u8; 32]],
) -> Result<(), CheckpointError> {
    if leaf_index >= checkpoint.tree_size {
        return Err(CheckpointError::InvalidIndex);
    }
    let mut fn_idx = leaf_index;
    let mut sn = checkpoint.tree_size - 1;
    let mut current = *leaf;
    for sibling in proof {
        if sn == 0 {
            return Err(CheckpointError::InvalidProof);
        }
        if (fn_idx & 1) == 1 || fn_idx == sn {
            current = node_hash(sibling, &current);
            while fn_idx != 0 && (fn_idx & 1) == 0 {
                fn_idx >>= 1;
                sn >>= 1;
            }
        } else {
            current = node_hash(&current, sibling);
        }
        fn_idx >>= 1;
        sn >>= 1;
    }
    if sn == 0 && current == checkpoint.root_hash {
        Ok(())
    } else {
        Err(CheckpointError::InvalidProof)
    }
}

/// Convenience wrapper for `verify_inclusion` accepting hex-encoded proof nodes.
pub fn verify_inclusion_hex(
    checkpoint: &Checkpoint,
    leaf_index: u64,
    leaf: &[u8; 32],
    proof_hex: &[String],
) -> Result<(), CheckpointError> {
    let mut proof = Vec::with_capacity(proof_hex.len());
    for item in proof_hex {
        proof.push(parse_sha256_hex(item)?);
    }
    verify_inclusion(checkpoint, leaf_index, leaf, &proof)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn largest_p2_lt(n: usize) -> usize {
        1usize << (n - 1).ilog2()
    }

    fn mth(leaves: &[[u8; 32]]) -> [u8; 32] {
        match leaves.len() {
            0 => empty_root_hash(),
            1 => leaves[0],
            n => {
                let k = largest_p2_lt(n);
                node_hash(&mth(&leaves[..k]), &mth(&leaves[k..]))
            }
        }
    }

    fn subproof(m: usize, leaves: &[[u8; 32]], complete: bool, out: &mut Vec<[u8; 32]>) {
        let n = leaves.len();
        if m == n {
            if !complete {
                out.push(mth(leaves));
            }
            return;
        }
        let k = largest_p2_lt(n);
        if m <= k {
            subproof(m, &leaves[..k], complete, out);
            out.push(mth(&leaves[k..]));
        } else {
            subproof(m - k, &leaves[k..], false, out);
            out.push(mth(&leaves[..k]));
        }
    }

    fn build_consistency(leaves: &[[u8; 32]], m: usize) -> Vec<[u8; 32]> {
        let mut out = Vec::new();
        subproof(m, leaves, true, &mut out);
        out
    }

    fn build_inclusion(leaves: &[[u8; 32]], index: usize, out: &mut Vec<[u8; 32]>) {
        let n = leaves.len();
        if n <= 1 {
            return;
        }
        let k = largest_p2_lt(n);
        if index < k {
            build_inclusion(&leaves[..k], index, out);
            out.push(mth(&leaves[k..]));
        } else {
            build_inclusion(&leaves[k..], index - k, out);
            out.push(mth(&leaves[..k]));
        }
    }

    fn make_leaves(count: usize) -> Vec<[u8; 32]> {
        (1..=count)
            .map(|seq| {
                let raw = format!(r#"{{"id":"m:{seq}"}}"#);
                leaf_hash(
                    &format!("event:{seq}"),
                    seq as u64,
                    "2026-11-26T00:00:00Z",
                    &sha256_bytes(raw.as_bytes()),
                )
            })
            .collect()
    }

    #[test]
    fn parse_and_format_c2sp_checkpoint_valid_and_malformed() {
        let root = empty_root_hash();
        let note = format_checkpoint("agentciv/civ:test", 0, &root).expect("format");
        let parsed = parse_checkpoint(&note).expect("parse");
        assert_eq!(parsed.origin, "agentciv/civ:test");
        assert_eq!(parsed.tree_size, 0);
        assert_eq!(parsed.root_hash, root);

        // Signed note with blank line + signature block parses header cleanly.
        let signed = format!("{note}\n-- witness AAAA\n");
        assert_eq!(parse_checkpoint(&signed).expect("signed"), parsed);

        // Reject malformed cases.
        assert_eq!(
            format_checkpoint("bad origin", 0, &root),
            Err(CheckpointError::InvalidFormat)
        );
        assert_eq!(
            parse_checkpoint(
                "agentciv/civ:test\n01\n47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=\n"
            ),
            Err(CheckpointError::InvalidFormat)
        );
        assert_eq!(
            parse_checkpoint("agentciv/civ:test\n1\nnot-base64\n"),
            Err(CheckpointError::InvalidFormat)
        );
        assert_eq!(
            parse_checkpoint(
                "agentciv/civ:test\n1\n47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFV=\n"
            ),
            Err(CheckpointError::InvalidFormat)
        );
    }

    #[test]
    fn detect_rollback_equivocation_and_tampered_proofs() {
        let leaves = make_leaves(10);
        let cp5 = Checkpoint {
            origin: "agentciv/civ:test".to_owned(),
            tree_size: 5,
            root_hash: mth(&leaves[..5]),
        };
        let cp10 = Checkpoint {
            origin: "agentciv/civ:test".to_owned(),
            tree_size: 10,
            root_hash: mth(&leaves[..10]),
        };
        let proof = build_consistency(&leaves[..10], 5);
        assert!(verify_consistency(&cp5, &cp10, &proof).is_ok());

        let hex_proof: Vec<String> = proof.iter().map(encode_sha256_hex).collect();
        assert!(verify_consistency_hex(&cp5, &cp10, &hex_proof).is_ok());

        // Rollback: N < M.
        assert_eq!(
            verify_consistency(&cp10, &cp5, &[]),
            Err(CheckpointError::Rollback {
                cached_size: 10,
                observed_size: 5
            })
        );

        // Split-view equivocation: N == M, different root.
        let mut forked = cp5.clone();
        forked.root_hash[0] ^= 0xff;
        assert_eq!(
            verify_consistency(&cp5, &forked, &[]),
            Err(CheckpointError::Equivocation { tree_size: 5 })
        );

        // Origin mismatch.
        let mut other_origin = cp10.clone();
        other_origin.origin = "agentciv/civ:other".to_owned();
        assert_eq!(
            verify_consistency(&cp5, &other_origin, &proof),
            Err(CheckpointError::OriginMismatch)
        );

        // Tampered proof.
        let mut bad_proof = proof.clone();
        bad_proof[0][0] ^= 0x01;
        assert_eq!(
            verify_consistency(&cp5, &cp10, &bad_proof),
            Err(CheckpointError::InvalidProof)
        );

        // Missing proof when N > M.
        assert_eq!(
            verify_consistency(&cp5, &cp10, &[]),
            Err(CheckpointError::InvalidProof)
        );

        // Inclusion verification and failure paths.
        let mut inc = Vec::new();
        build_inclusion(&leaves[..10], 3, &mut inc);
        let inc_hex: Vec<String> = inc.iter().map(encode_sha256_hex).collect();
        assert!(verify_inclusion_hex(&cp10, 3, &leaves[3], &inc_hex).is_ok());
        assert_eq!(
            verify_inclusion(&cp10, 10, &leaves[3], &inc),
            Err(CheckpointError::InvalidIndex)
        );
        assert_eq!(
            verify_inclusion(&cp10, 3, &leaves[4], &inc),
            Err(CheckpointError::InvalidProof)
        );
    }
}
