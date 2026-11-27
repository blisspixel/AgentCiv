use agentciv_archive::{
    ArchiveSeal, MAX_SEAL_BYTES, PREIMAGE_VERSION, SEAL_VERSION, SealAlgorithm, SealError,
    SealVerifier, build_seal_preimage, compute_key_id, compute_root_digest,
    decode_base64_canonical, encode_base64, inspect_seal_envelope, verify_seal_envelope,
};
use serde_json::{Value, json};
use std::fs;
use std::process::{Command, Output};

const FIXTURE_BUNDLE: &str =
    include_str!("../../../conformance/fixtures/valid/archive-bundle.json");
const FIXTURE_SEAL: &str = include_str!("../../../conformance/fixtures/valid/archive-seal.json");
const SEALED_AT: &str = "2026-11-26T00:00:00Z";

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_agentciv-archive"))
        .args(args)
        .output()
        .unwrap()
}

fn synthetic_seal(bundle_input: &str, alg: SealAlgorithm, note: Option<&str>) -> ArchiveSeal {
    let summary = compute_root_digest(bundle_input).unwrap();
    let pubkey = vec![0x42u8; alg.pubkey_len()];
    let signature = vec![0x24u8; alg.signature_len()];
    let key_id = compute_key_id(alg, &pubkey);
    ArchiveSeal {
        seal_version: SEAL_VERSION.to_owned(),
        bundle_sha256: summary.bundle_sha256,
        root_digest_sha256: summary.root_digest_sha256,
        preimage_version: PREIMAGE_VERSION.to_owned(),
        alg,
        key_id,
        pubkey_b64: encode_base64(&pubkey),
        signature_b64: encode_base64(&signature),
        sealed_at: SEALED_AT.to_owned(),
        signer_note: note.map(str::to_owned),
    }
}

struct MockVerifier {
    expected_alg: SealAlgorithm,
    accept: bool,
}

impl SealVerifier for MockVerifier {
    fn verify(
        &self,
        alg: SealAlgorithm,
        pubkey: &[u8],
        preimage: &[u8],
        signature: &[u8],
    ) -> Result<(), SealError> {
        if alg != self.expected_alg {
            return Err(SealError::UnsupportedAlgorithm);
        }
        if pubkey.len() != alg.pubkey_len() || signature.len() != alg.signature_len() {
            return Err(SealError::CryptographicVerificationFailed);
        }
        let text = std::str::from_utf8(preimage)
            .map_err(|_| SealError::CryptographicVerificationFailed)?;
        if !text.starts_with("agentciv-archive-seal-v1\n") || !text.ends_with('\n') {
            return Err(SealError::CryptographicVerificationFailed);
        }
        if self.accept {
            Ok(())
        } else {
            Err(SealError::CryptographicVerificationFailed)
        }
    }
}

#[test]
fn canonical_base64_round_trip_and_strict_rejections() {
    for len in 1..=10 {
        let bytes: Vec<u8> = (0..len).map(|i| (i as u8).wrapping_mul(37)).collect();
        let encoded = encode_base64(&bytes);
        assert_eq!(decode_base64_canonical(&encoded).unwrap(), bytes);
    }

    // Non-canonical trailing bits: 0x00 encodes as "AA==", whereas "AB==" sets non-zero pad bits.
    assert_eq!(decode_base64_canonical("AA==").unwrap(), vec![0u8]);
    assert_eq!(
        decode_base64_canonical("AB=="),
        Err(SealError::InvalidBase64)
    );

    // 2 bytes: [0x00, 0x00] -> "AAA=", whereas "AAB=" sets non-zero trailing bits.
    assert_eq!(decode_base64_canonical("AAA=").unwrap(), vec![0u8, 0u8]);
    assert_eq!(
        decode_base64_canonical("AAB="),
        Err(SealError::InvalidBase64)
    );

    // Missing padding, whitespace, URL-safe chars, internal padding, empty string.
    for invalid in [
        "", "AA", "AA= ", "AA==", "A-==", "A_==", "A=AA", "AA==AAAA", "AAA=AAAA",
    ] {
        if invalid == "AA==" {
            continue;
        }
        assert_eq!(
            decode_base64_canonical(invalid),
            Err(SealError::InvalidBase64),
            "expected rejection for {invalid:?}"
        );
    }
}

#[test]
fn root_digest_and_preimage_are_deterministic_and_validate_bundle() {
    let summary = compute_root_digest(FIXTURE_BUNDLE).unwrap();
    assert_eq!(summary.record_count, 1);
    assert_eq!(summary.preimage_version, PREIMAGE_VERSION);
    assert_eq!(summary.bundle_sha256.len(), 64);
    assert_eq!(summary.canonical_records_sha256.len(), 64);
    assert_eq!(summary.root_digest_sha256.len(), 64);

    // Deterministic across calls
    assert_eq!(summary, compute_root_digest(FIXTURE_BUNDLE).unwrap());

    // Preimage has exact 6 LF-terminated lines (7 newline-split segments)
    let key_id = compute_key_id(SealAlgorithm::Ed25519, &[0x11u8; 32]);
    let preimage = build_seal_preimage(
        &summary.bundle_sha256,
        &summary.root_digest_sha256,
        SealAlgorithm::Ed25519,
        &key_id,
        SEALED_AT,
    );
    let segments: Vec<&str> = preimage.split('\n').collect();
    assert_eq!(segments.len(), 7);
    assert_eq!(segments[0], PREIMAGE_VERSION);
    assert_eq!(segments[1], summary.bundle_sha256);
    assert_eq!(segments[2], summary.root_digest_sha256);
    assert_eq!(segments[3], "ed25519");
    assert_eq!(segments[4], key_id);
    assert_eq!(segments[5], SEALED_AT);
    assert_eq!(segments[6], "");

    // Invalid bundle rejected before digesting
    assert!(matches!(
        compute_root_digest("{}"),
        Err(SealError::InvalidBundle("invalid_bundle"))
    ));
}

#[test]
fn valid_seals_for_all_four_algorithms_pass_structural_inspection() {
    for alg in SealAlgorithm::ALL {
        let seal = synthetic_seal(FIXTURE_BUNDLE, alg, Some("offline test seal"));
        let seal_json = serde_json::to_string_pretty(&seal).unwrap();
        let report = inspect_seal_envelope(FIXTURE_BUNDLE, &seal_json).unwrap();
        assert!(report.valid);
        assert_eq!(report.alg, alg);
        assert_eq!(report.seal_version, SEAL_VERSION);
        assert_eq!(report.preimage_version, PREIMAGE_VERSION);
        assert_eq!(report.key_id, seal.key_id);
        assert_eq!(report.sealed_at, SEALED_AT);
        assert_eq!(report.record_count, 1);
        assert_eq!(report.verification_scope, "structural_and_digest_only");
        assert!(!report.cryptographic_signature_verified);
        assert_eq!(report.source_authenticity, "unverified");
        assert!(!report.authority_transferred);
    }
}

#[test]
fn tampered_bundle_and_mismatched_digests_are_rejected() {
    let seal = synthetic_seal(FIXTURE_BUNDLE, SealAlgorithm::Ed25519, None);
    let seal_json = serde_json::to_string(&seal).unwrap();

    // Tampered bundle whitespace changes bundle_sha256 even though bundle stays valid
    let tampered_bundle = format!("{FIXTURE_BUNDLE}\n");
    assert_eq!(
        inspect_seal_envelope(&tampered_bundle, &seal_json),
        Err(SealError::BundleDigestMismatch)
    );

    // Mismatched root_digest_sha256
    let mut bad_root = seal.clone();
    bad_root.root_digest_sha256 = "0".repeat(64);
    assert_eq!(
        inspect_seal_envelope(FIXTURE_BUNDLE, &serde_json::to_string(&bad_root).unwrap()),
        Err(SealError::RootDigestMismatch)
    );

    // Mismatched key_id
    let mut bad_key_id = seal;
    bad_key_id.key_id = "f".repeat(64);
    assert_eq!(
        inspect_seal_envelope(FIXTURE_BUNDLE, &serde_json::to_string(&bad_key_id).unwrap()),
        Err(SealError::KeyIdMismatch)
    );
}

#[test]
fn wrong_key_and_signature_lengths_and_non_canonical_base64_are_rejected() {
    for alg in SealAlgorithm::ALL {
        let base = synthetic_seal(FIXTURE_BUNDLE, alg, None);

        // Wrong pubkey length
        let short_pk = vec![0x42u8; alg.pubkey_len() - 1];
        let mut bad_pk = base.clone();
        bad_pk.pubkey_b64 = encode_base64(&short_pk);
        bad_pk.key_id = compute_key_id(alg, &short_pk);
        assert_eq!(
            inspect_seal_envelope(FIXTURE_BUNDLE, &serde_json::to_string(&bad_pk).unwrap()),
            Err(SealError::InvalidPubkeyLength)
        );

        // Wrong signature length
        let short_sig = vec![0x24u8; alg.signature_len() - 1];
        let mut bad_sig = base.clone();
        bad_sig.signature_b64 = encode_base64(&short_sig);
        assert_eq!(
            inspect_seal_envelope(FIXTURE_BUNDLE, &serde_json::to_string(&bad_sig).unwrap()),
            Err(SealError::InvalidSignatureLength)
        );
    }

    // Non-canonical base64 in pubkey (32 bytes -> 44 chars ending with '=', flip trailing pad bit)
    let mut non_canon = synthetic_seal(FIXTURE_BUNDLE, SealAlgorithm::Ed25519, None);
    let mut chars: Vec<char> = non_canon.pubkey_b64.chars().collect();
    assert_eq!(chars[chars.len() - 1], '=');
    // For 32 bytes (32 % 3 == 2), the 43rd char has 2 low padding bits that must be 0.
    // Flipping 'A' (0) -> 'B' (1) or toggling bit 0 guarantees non-zero padding bits.
    let idx = chars.len() - 2;
    chars[idx] = if chars[idx] == 'B' { 'D' } else { 'B' };
    non_canon.pubkey_b64 = chars.into_iter().collect();
    assert_eq!(
        inspect_seal_envelope(FIXTURE_BUNDLE, &serde_json::to_string(&non_canon).unwrap()),
        Err(SealError::InvalidBase64)
    );
}

#[test]
fn duplicate_json_keys_oversized_seals_and_invalid_fields_are_rejected() {
    let seal = synthetic_seal(FIXTURE_BUNDLE, SealAlgorithm::Ed25519, None);
    let seal_json = serde_json::to_string(&seal).unwrap();

    // Duplicate JSON key
    let duplicate = format!(
        r#"{{"alg":"ed25519",{}}}"#,
        seal_json.trim_start_matches('{')
    );
    assert_eq!(
        inspect_seal_envelope(FIXTURE_BUNDLE, &duplicate),
        Err(SealError::InvalidJson)
    );

    // Oversized seal input
    let oversized = " ".repeat(MAX_SEAL_BYTES + 1);
    assert_eq!(
        inspect_seal_envelope(FIXTURE_BUNDLE, &oversized),
        Err(SealError::SealTooLarge)
    );

    // Unknown field rejected by schema
    let mut with_extra: Value = serde_json::from_str(&seal_json).unwrap();
    with_extra["unexpected"] = json!("nope");
    assert_eq!(
        inspect_seal_envelope(FIXTURE_BUNDLE, &with_extra.to_string()),
        Err(SealError::InvalidSealSchema)
    );

    // Invalid timestamp rejected by schema
    let mut bad_time: Value = serde_json::from_str(&seal_json).unwrap();
    bad_time["sealed_at"] = json!("2026-13-45T99:99:99Z");
    assert_eq!(
        inspect_seal_envelope(FIXTURE_BUNDLE, &bad_time.to_string()),
        Err(SealError::InvalidSealSchema)
    );

    // Multibyte signer_note <= 256 chars but > 256 UTF-8 bytes rejected by byte bound
    let multibyte_note = "\u{00e9}".repeat(129); // 129 chars, 258 UTF-8 bytes
    let over_bytes = synthetic_seal(
        FIXTURE_BUNDLE,
        SealAlgorithm::Ed25519,
        Some(&multibyte_note),
    );
    assert_eq!(
        inspect_seal_envelope(FIXTURE_BUNDLE, &serde_json::to_string(&over_bytes).unwrap()),
        Err(SealError::SignerNoteTooLarge)
    );
}

#[test]
fn custom_seal_verifier_accepts_and_rejects_cleanly() {
    let seal = synthetic_seal(FIXTURE_BUNDLE, SealAlgorithm::Ed25519MlDsa65, None);
    let seal_json = serde_json::to_string(&seal).unwrap();

    let accepting = MockVerifier {
        expected_alg: SealAlgorithm::Ed25519MlDsa65,
        accept: true,
    };
    let report = verify_seal_envelope(FIXTURE_BUNDLE, &seal_json, &accepting).unwrap();
    assert!(report.valid);
    assert!(report.cryptographic_signature_verified);
    assert_eq!(report.verification_scope, "structural_digest_and_signature");
    assert_eq!(report.source_authenticity, "key_possession_verified_only");
    assert!(!report.authority_transferred);

    let rejecting = MockVerifier {
        expected_alg: SealAlgorithm::Ed25519MlDsa65,
        accept: false,
    };
    assert_eq!(
        verify_seal_envelope(FIXTURE_BUNDLE, &seal_json, &rejecting),
        Err(SealError::CryptographicVerificationFailed)
    );

    let wrong_alg = MockVerifier {
        expected_alg: SealAlgorithm::SlhDsaSha2_128s,
        accept: true,
    };
    assert_eq!(
        verify_seal_envelope(FIXTURE_BUNDLE, &seal_json, &wrong_alg),
        Err(SealError::UnsupportedAlgorithm)
    );

    // Display formatting for SealError matches fixed codes
    assert_eq!(
        SealError::CryptographicVerificationFailed.to_string(),
        "cryptographic_verification_failed"
    );
}

#[test]
fn checked_in_positive_seal_fixture_matches_positive_bundle_fixture() {
    // Normalize CRLF to LF so Windows git autocrlf checkouts match the canonical LF fixture hash.
    let normalized_bundle = FIXTURE_BUNDLE.replace("\r\n", "\n");
    let report = inspect_seal_envelope(&normalized_bundle, FIXTURE_SEAL).unwrap();
    assert!(report.valid);
    assert_eq!(report.alg, SealAlgorithm::Ed25519);
    assert_eq!(report.verification_scope, "structural_and_digest_only");
    assert!(!report.cryptographic_signature_verified);
    assert_eq!(report.source_authenticity, "unverified");
    assert!(!report.authority_transferred);
}

#[test]
fn cli_root_digest_and_inspect_seal_succeed_and_fail_with_expected_exit_codes() {
    let dir = tempfile::tempdir().unwrap();
    let bundle_path = dir.path().join("bundle.json");
    fs::write(&bundle_path, FIXTURE_BUNDLE).unwrap();

    // root-digest
    let result = cli(&["root-digest", bundle_path.to_str().unwrap()]);
    assert!(result.status.success(), "{:?}", result.stderr);
    assert!(result.stderr.is_empty());
    let digest_json: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(digest_json["record_count"], 1);
    assert_eq!(digest_json["preimage_version"], PREIMAGE_VERSION);

    // inspect-seal valid
    let seal = synthetic_seal(
        FIXTURE_BUNDLE,
        SealAlgorithm::SlhDsaSha2_128s,
        Some("cli test"),
    );
    let seal_path = dir.path().join("seal.json");
    fs::write(&seal_path, serde_json::to_vec_pretty(&seal).unwrap()).unwrap();

    let result = cli(&[
        "inspect-seal",
        bundle_path.to_str().unwrap(),
        seal_path.to_str().unwrap(),
    ]);
    assert!(result.status.success(), "{:?}", result.stderr);
    assert!(result.stderr.is_empty());
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["valid"], true);
    assert_eq!(report["alg"], "slh-dsa-sha2-128s");
    assert_eq!(report["verification_scope"], "structural_and_digest_only");
    assert_eq!(report["cryptographic_signature_verified"], false);
    assert_eq!(report["source_authenticity"], "unverified");
    assert_eq!(report["authority_transferred"], false);

    // inspect-seal tampered bundle
    let tampered_path = dir.path().join("tampered.json");
    fs::write(&tampered_path, format!("{FIXTURE_BUNDLE}\n")).unwrap();
    let result = cli(&[
        "inspect-seal",
        tampered_path.to_str().unwrap(),
        seal_path.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert_eq!(result.stderr, b"bundle_digest_mismatch\n");

    // inspect-seal oversized seal
    let big_seal_path = dir.path().join("big-seal.json");
    fs::write(&big_seal_path, vec![b' '; MAX_SEAL_BYTES + 1]).unwrap();
    let result = cli(&[
        "inspect-seal",
        bundle_path.to_str().unwrap(),
        big_seal_path.to_str().unwrap(),
    ]);
    assert!(!result.status.success());
    assert!(result.stdout.is_empty());
    assert_eq!(result.stderr, b"seal_too_large\n");
}
