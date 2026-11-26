//! Tombstone-compatible RFC 9162 Merkle tree and `c2sp.org/tlog-checkpoint` formatting.
//!
//! Each leaf commits to `(event_id, sequence, timestamp, sha256(original_record_bytes))`
//! with RFC 9162 domain separation (`0x00` for leaves, `0x01` for interior nodes).
//! Because the leaf depends on `sha256(original_record_bytes)` rather than the mutable
//! rendered event body, `event.redacted` and `artifact.withdrawn` tombstones preserve
//! the leaf hash and append-only consistency proofs remain valid.

use sha2::{Digest, Sha256};

const LEAF_DOMAIN: &[u8] = b"agentciv-merkle-leaf/0.1\0";
const RFC9162_LEAF_PREFIX: u8 = 0x00;
const RFC9162_NODE_PREFIX: u8 = 0x01;
const B64_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

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

/// Encode 32-byte hash as 64-char lowercase hexadecimal.
pub fn encode_hex(hash: &[u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in hash {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// Decode 64-char hexadecimal string into 32-byte hash.
pub fn decode_hex(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 {
        return None;
    }
    let bytes = hex.as_bytes();
    let mut out = [0u8; 32];
    for (index, slot) in out.iter_mut().enumerate() {
        let hi = hex_nibble(bytes[index * 2])?;
        let lo = hex_nibble(bytes[index * 2 + 1])?;
        *slot = (hi << 4) | lo;
    }
    Some(out)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Encode 32 bytes as standard RFC 4648 base64 with `=` padding (44 ASCII bytes).
pub fn encode_base64(input: &[u8]) -> String {
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

fn largest_power_of_two_less_than(n: usize) -> usize {
    debug_assert!(n >= 2);
    1usize << (n - 1).ilog2()
}

/// Compute RFC 9162 Merkle Tree Hash `MTH(leaves)`.
pub fn root_hash(leaves: &[[u8; 32]]) -> [u8; 32] {
    match leaves.len() {
        0 => empty_root_hash(),
        1 => leaves[0],
        n => {
            let k = largest_power_of_two_less_than(n);
            let left = root_hash(&leaves[..k]);
            let right = root_hash(&leaves[k..]);
            node_hash(&left, &right)
        }
    }
}

/// Generate an RFC 9162 inclusion proof `PATH(index, leaves)` for `0 <= index < leaves.len()`.
pub fn inclusion_proof(leaves: &[[u8; 32]], index: usize) -> Option<Vec<[u8; 32]>> {
    if index >= leaves.len() {
        return None;
    }
    let mut proof = Vec::new();
    inclusion_path(leaves, index, &mut proof);
    Some(proof)
}

fn inclusion_path(leaves: &[[u8; 32]], index: usize, out: &mut Vec<[u8; 32]>) {
    let n = leaves.len();
    if n <= 1 {
        return;
    }
    let k = largest_power_of_two_less_than(n);
    if index < k {
        inclusion_path(&leaves[..k], index, out);
        out.push(root_hash(&leaves[k..]));
    } else {
        inclusion_path(&leaves[k..], index - k, out);
        out.push(root_hash(&leaves[..k]));
    }
}

/// Verify an RFC 9162 inclusion proof for `leaf_hash` at `leaf_index` in a tree of `tree_size`.
pub fn verify_inclusion_proof(
    leaf_index: u64,
    tree_size: u64,
    leaf: &[u8; 32],
    proof: &[[u8; 32]],
    expected_root: &[u8; 32],
) -> bool {
    if leaf_index >= tree_size {
        return false;
    }
    let mut fn_idx = leaf_index;
    let mut sn = tree_size - 1;
    let mut current = *leaf;
    for sibling in proof {
        if sn == 0 {
            return false;
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
    sn == 0 && &current == expected_root
}

/// Generate an RFC 9162 consistency proof `PROOF(from_size, leaves)` for `0 < from_size <= leaves.len()`.
pub fn consistency_proof(leaves: &[[u8; 32]], from_size: usize) -> Option<Vec<[u8; 32]>> {
    let n = leaves.len();
    if from_size == 0 || from_size > n {
        return None;
    }
    let mut proof = Vec::new();
    subproof(from_size, leaves, true, &mut proof);
    Some(proof)
}

fn subproof(m: usize, leaves: &[[u8; 32]], complete: bool, out: &mut Vec<[u8; 32]>) {
    let n = leaves.len();
    if m == n {
        if !complete {
            out.push(root_hash(leaves));
        }
        return;
    }
    let k = largest_power_of_two_less_than(n);
    if m <= k {
        subproof(m, &leaves[..k], complete, out);
        out.push(root_hash(&leaves[k..]));
    } else {
        subproof(m - k, &leaves[k..], false, out);
        out.push(root_hash(&leaves[..k]));
    }
}

/// Verify an RFC 9162 consistency proof between `(first_size, first_root)` and `(second_size, second_root)`.
pub fn verify_consistency_proof(
    first_size: u64,
    second_size: u64,
    first_root: &[u8; 32],
    second_root: &[u8; 32],
    proof: &[[u8; 32]],
) -> bool {
    if first_size > second_size {
        return false;
    }
    if first_size == 0 && *first_root != empty_root_hash() {
        return false;
    }
    if first_size == second_size {
        return proof.is_empty() && first_root == second_root;
    }
    if first_size == 0 {
        return proof.is_empty();
    }
    if proof.is_empty() {
        return false;
    }
    let mut fn_idx = first_size - 1;
    let mut sn = second_size - 1;
    while (fn_idx & 1) == 1 {
        fn_idx >>= 1;
        sn >>= 1;
    }
    let (mut fr, mut sr, rest) = if first_size.is_power_of_two() {
        (*first_root, *first_root, proof)
    } else {
        (proof[0], proof[0], &proof[1..])
    };
    for node in rest {
        if sn == 0 {
            return false;
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
    sn == 0 && &fr == first_root && &sr == second_root
}

/// Format a standard 3-line `c2sp.org/tlog-checkpoint` note body.
pub fn format_c2sp_checkpoint(origin: &str, tree_size: u64, root: &[u8; 32]) -> String {
    let encoded_root = encode_base64(root);
    format!("{origin}\n{tree_size}\n{encoded_root}\n")
}

/// Canonical checkpoint origin string for an AgentCiv world.
pub fn checkpoint_origin(world_id: &str) -> String {
    format!("agentciv/{world_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_leaves(count: usize) -> Vec<[u8; 32]> {
        (1..=count)
            .map(|seq| {
                let record = format!(r#"{{"id":"msg:{seq}","seq":{seq}}}"#);
                let orig = sha256_bytes(record.as_bytes());
                leaf_hash(
                    &format!("event:{seq}"),
                    seq as u64,
                    "2026-11-26T00:00:00Z",
                    &orig,
                )
            })
            .collect()
    }

    #[test]
    fn empty_and_single_leaf_roots_match_rfc9162() {
        assert_eq!(
            encode_hex(&root_hash(&[])),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let one = sample_leaves(1);
        assert_eq!(root_hash(&one), one[0]);
        assert!(verify_inclusion_proof(0, 1, &one[0], &[], &one[0]));
        assert!(!verify_inclusion_proof(1, 1, &one[0], &[], &one[0]));
    }

    #[test]
    fn exhaustive_inclusion_and_consistency_proofs_for_1_to_20_events() {
        let all = sample_leaves(20);
        let roots: Vec<[u8; 32]> = (0..=20).map(|n| root_hash(&all[..n])).collect();

        for n in 1..=20 {
            for idx in 0..n {
                let proof = inclusion_proof(&all[..n], idx).expect("inclusion proof");
                assert!(verify_inclusion_proof(
                    idx as u64, n as u64, &all[idx], &proof, &roots[n]
                ));
                let mut wrong_leaf = all[idx];
                wrong_leaf[0] ^= 0xff;
                assert!(!verify_inclusion_proof(
                    idx as u64,
                    n as u64,
                    &wrong_leaf,
                    &proof,
                    &roots[n]
                ));
            }

            assert!(verify_consistency_proof(
                0,
                n as u64,
                &roots[0],
                &roots[n],
                &[]
            ));

            for m in 1..=n {
                let proof = consistency_proof(&all[..n], m).expect("consistency proof");
                assert!(
                    verify_consistency_proof(m as u64, n as u64, &roots[m], &roots[n], &proof),
                    "consistency proof failed for m={m}, n={n}"
                );
                if let Some(first) = proof.first() {
                    let mut tampered = proof.clone();
                    tampered[0] = *first;
                    tampered[0][0] ^= 0x01;
                    assert!(!verify_consistency_proof(
                        m as u64, n as u64, &roots[m], &roots[n], &tampered
                    ));
                }
            }
        }
    }

    #[test]
    fn hex_and_base64_roundtrip_and_c2sp_format() {
        let hash = empty_root_hash();
        let hex = encode_hex(&hash);
        assert_eq!(decode_hex(&hex), Some(hash));
        assert_eq!(decode_hex("short"), None);
        assert_eq!(decode_hex(&"z".repeat(64)), None);
        let note = format_c2sp_checkpoint("agentciv/civ:test", 0, &hash);
        assert_eq!(
            note,
            "agentciv/civ:test\n0\n47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU=\n"
        );
    }

    #[test]
    fn zero_size_consistency_rejects_forged_empty_root() {
        let empty = empty_root_hash();
        let mut forged = empty;
        forged[0] ^= 0xff;
        assert!(verify_consistency_proof(0, 0, &empty, &empty, &[]));
        assert!(!verify_consistency_proof(0, 0, &forged, &forged, &[]));
        assert!(!verify_consistency_proof(0, 0, &empty, &forged, &[]));
        assert!(!verify_consistency_proof(0, 0, &forged, &empty, &[]));
    }
}
