use agentciv_bulletin::town_square::*;
use agentciv_bulletin::{Binding, Database, Problem};
use rusqlite::{
    Connection,
    types::{Value as SqlValue, ValueRef},
};
use serde_json::{Value, json};
use std::cell::Cell;

struct CountingSql {
    conn: Connection,
    writes: Cell<usize>,
}

impl CountingSql {
    fn in_memory() -> Self {
        Self {
            conn: Connection::open_in_memory().expect("in-memory sqlite"),
            writes: Cell::new(0),
        }
    }

    fn reset_writes(&self) {
        self.writes.set(0);
    }

    fn write_count(&self) -> usize {
        self.writes.get()
    }
}

impl Database for CountingSql {
    fn query(&self, statement: &str, bindings: Vec<Binding>) -> Result<Vec<Value>, Problem> {
        let trimmed = statement.trim_start();
        if trimmed.starts_with("INSERT")
            || trimmed.starts_with("UPDATE")
            || trimmed.starts_with("DELETE")
        {
            self.writes.set(self.writes.get() + 1);
        }
        let values: Vec<SqlValue> = bindings
            .into_iter()
            .map(|binding| match binding {
                Binding::Integer(value) => SqlValue::Integer(value),
                Binding::Text(value) => SqlValue::Text(value),
            })
            .collect();
        let mut prepared = self
            .conn
            .prepare(statement)
            .map_err(|_| Problem::new(500, "storage_failed"))?;
        let columns: Vec<String> = prepared.column_names().map(str::to_owned).collect();
        let rows = prepared
            .query_map(rusqlite::params_from_iter(values), |row| {
                let mut result = serde_json::Map::new();
                for (index, name) in columns.iter().enumerate() {
                    let value = match row.get_ref(index)? {
                        ValueRef::Null => Value::Null,
                        ValueRef::Integer(number) => json!(number),
                        ValueRef::Text(text) => json!(String::from_utf8_lossy(text)),
                        _ => panic!("unexpected SQL type"),
                    };
                    result.insert(name.clone(), value);
                }
                Ok(Value::Object(result))
            })
            .map_err(|_| Problem::new(500, "storage_failed"))?;
        rows.collect::<rusqlite::Result<_>>()
            .map_err(|_| Problem::new(500, "storage_failed"))
    }
}

fn uuid_v7(n: u64) -> String {
    format!("019a4f1c-7b2e-7a10-8c3d-{n:012x}")
}

fn nonce_hex(n: u64) -> String {
    format!("{n:032x}")
}

fn salt_hex(n: u64) -> String {
    format!("aa{n:030x}")
}

fn ed25519_pubkey_b64(seed: u8) -> (String, String) {
    let mut bytes = [seed; ED25519_PUBKEY_LEN];
    bytes[0] = seed.wrapping_add(1);
    let b64 = base64_encode(&bytes);
    let key_id = derive_key_id(Algorithm::Ed25519, &bytes);
    (key_id, b64)
}

fn ed25519_pubkey_b64_u32(idx: u32) -> (String, String) {
    let mut bytes = [0_u8; ED25519_PUBKEY_LEN];
    bytes[..4].copy_from_slice(&idx.to_be_bytes());
    let b64 = base64_encode(&bytes);
    let key_id = derive_key_id(Algorithm::Ed25519, &bytes);
    (key_id, b64)
}

fn ml_dsa_65_pubkey_b64(seed: u8) -> (String, String) {
    let bytes = vec![seed; ML_DSA_65_PUBKEY_LEN];
    let b64 = base64_encode(&bytes);
    let key_id = derive_key_id(Algorithm::MlDsa65, &bytes);
    (key_id, b64)
}

fn fake_sig_b64(alg: Algorithm, seed: u8) -> String {
    base64_encode(&vec![seed; alg.signature_len()])
}

fn setup_store() -> CountingSql {
    let db = CountingSql::in_memory();
    TownSquareStore::new(&db).initialize().expect("init ts");
    db
}

#[test]
fn hmac_sha256_matches_rfc4231_vectors() {
    // RFC 4231 Test Case 2: key = "Jefe", data = "what do ya want for nothing?"
    let mac = hmac_sha256(b"Jefe", b"what do ya want for nothing?");
    assert_eq!(
        hex_encode_lower(&mac),
        "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
    );
    // RFC 4231 Test Case 6: 131-byte key of 0xaa (> 64-byte block size)
    let long_key = [0xaa_u8; 131];
    let mac_long = hmac_sha256(
        &long_key,
        b"Test Using Larger Than Block-Size Key - Hash Key First",
    );
    assert_eq!(
        hex_encode_lower(&mac_long),
        "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
    );
    assert!(constant_time_eq(b"abc", b"abc"));
    assert!(!constant_time_eq(b"abc", b"abd"));
    assert!(!constant_time_eq(b"abc", b"ab"));
}

#[test]
fn strict_base64_and_base32_enforce_rfc4648_canonical_encoding() {
    for raw in [
        b"".as_slice(),
        b"f",
        b"fo",
        b"foo",
        b"foob",
        b"fooba",
        b"foobar",
        &[0_u8, 255, 128, 63, 62],
    ] {
        let encoded = base64_encode(raw);
        let decoded = base64_decode_strict(&encoded).expect("valid canonical b64");
        assert_eq!(decoded, raw);
    }

    // Non-canonical inputs must all be rejected
    for invalid in [
        "Zg",       // missing padding
        "Zg=",      // wrong padding
        "Zg== ",    // trailing space
        " Zg==",    // leading space
        "Zg=\n=",   // embedded newline
        "Zh==",     // non-zero trailing pad bits (0x66 -> 'g', 'h' has low bit set)
        "Zm9v=",    // extra padding
        "Zm9v====", // invalid length / padding
        "ab-_",     // URL-safe alphabet rejected
        "Zg==Zg==", // padding in non-final chunk
        "A===",     // invalid triple padding
        "AB=C",     // '=' in position 2 without position 3
        "Zm8=",     // valid? wait: "fo" is "Zm8=", let's test non-zero 2-bit tail: "Zm9="
    ] {
        if invalid == "Zm8=" {
            assert_eq!(base64_decode_strict(invalid).unwrap(), b"fo");
        } else {
            assert_eq!(
                base64_decode_strict(invalid).unwrap_err(),
                TownSquareError::InvalidBase64,
                "should reject {invalid:?}"
            );
        }
    }
    assert_eq!(
        base64_decode_strict("Zm9=").unwrap_err(),
        TownSquareError::InvalidBase64
    );

    // RFC 4648 lowercase base32 test vectors (no padding)
    assert_eq!(base32_encode_lower_nopad(b""), "");
    assert_eq!(base32_encode_lower_nopad(b"f"), "my");
    assert_eq!(base32_encode_lower_nopad(b"fo"), "mzxq");
    assert_eq!(base32_encode_lower_nopad(b"foo"), "mzxw6");
    assert_eq!(base32_encode_lower_nopad(b"foob"), "mzxw6yq");
    assert_eq!(base32_encode_lower_nopad(b"fooba"), "mzxw6ytb");
    assert_eq!(base32_encode_lower_nopad(b"foobar"), "mzxw6ytboi");
}

#[test]
fn algorithm_key_and_signature_validation_and_domain_separation() {
    assert_eq!(Algorithm::parse("ed25519"), Ok(Algorithm::Ed25519));
    assert_eq!(Algorithm::parse("ml-dsa-65"), Ok(Algorithm::MlDsa65));
    assert_eq!(
        Algorithm::parse("rsa-2048"),
        Err(TownSquareError::InvalidAlgorithm)
    );

    let ed_pk = vec![7_u8; ED25519_PUBKEY_LEN];
    let ed_pk_b64 = base64_encode(&ed_pk);
    assert_eq!(
        validate_public_key_b64(Algorithm::Ed25519, &ed_pk_b64).unwrap(),
        ed_pk
    );
    assert_eq!(
        validate_public_key_b64(Algorithm::MlDsa65, &ed_pk_b64),
        Err(TownSquareError::InvalidPublicKeyLength)
    );

    let pq_pk = vec![9_u8; ML_DSA_65_PUBKEY_LEN];
    let pq_pk_b64 = base64_encode(&pq_pk);
    assert_eq!(
        validate_public_key_b64(Algorithm::MlDsa65, &pq_pk_b64).unwrap(),
        pq_pk
    );
    assert_eq!(
        validate_public_key_b64(Algorithm::Ed25519, &pq_pk_b64),
        Err(TownSquareError::InvalidPublicKeyLength)
    );

    let ed_sig = vec![3_u8; ED25519_SIG_LEN];
    let pq_sig = vec![4_u8; ML_DSA_65_SIG_LEN];
    assert_eq!(
        validate_signature_b64(Algorithm::Ed25519, &base64_encode(&ed_sig)).unwrap(),
        ed_sig
    );
    assert_eq!(
        validate_signature_b64(Algorithm::MlDsa65, &base64_encode(&pq_sig)).unwrap(),
        pq_sig
    );
    assert_eq!(
        validate_signature_b64(Algorithm::Ed25519, &base64_encode(&[0_u8; 63])),
        Err(TownSquareError::InvalidSignatureLength)
    );
    assert_eq!(
        validate_signature_b64(Algorithm::MlDsa65, &base64_encode(&[0_u8; 3308])),
        Err(TownSquareError::InvalidSignatureLength)
    );

    // Domain separation between algorithms
    let id_ed = derive_key_id(Algorithm::Ed25519, &ed_pk);
    let id_pq = derive_key_id(Algorithm::MlDsa65, &ed_pk);
    assert_ne!(id_ed, id_pq);
    assert!(is_lower_hex(&id_ed, 64));

    let fp = derive_short_fp(Algorithm::Ed25519, &ed_pk);
    assert_eq!(fp.len(), 12);
    assert!(
        fp.bytes()
            .all(|b| b.is_ascii_lowercase() || (b'2'..=b'7').contains(&b))
    );
    assert_eq!(format_display_tag("scout", &fp), format!("scout#{fp}"));
}

#[test]
fn handle_validation_enforces_ascii_grammar_and_reserved_denylist() {
    for valid in [
        "ab",
        "0a",
        "pueo_scout",
        "agent-01",
        "a".repeat(24).as_str(),
    ] {
        assert!(validate_handle(valid).is_ok(), "expected valid: {valid}");
    }
    for invalid in [
        "",
        "a",
        &"a".repeat(25),
        "Alice",
        "-abc",
        "_abc",
        "bad space",
        "bad.dot",
        "bad#tag",
        "\u{0430}dmin", // Cyrillic 'а' homoglyph
        "abc\u{202e}",  // bidi override
    ] {
        assert_eq!(
            validate_handle(invalid),
            Err(TownSquareError::InvalidHandle),
            "expected InvalidHandle for {invalid:?}"
        );
    }
    for reserved in RESERVED_HANDLES {
        assert_eq!(
            validate_handle(reserved),
            Err(TownSquareError::ReservedHandle),
            "expected ReservedHandle for {reserved:?}"
        );
    }
}

#[test]
fn body_sanitization_blocks_worms_unicode_smuggling_and_leaked_credentials() {
    assert!(validate_post_body("Hello\n\tWorld! Caf\u{00e9} e\u{0301}\u{0327}").is_ok());
    assert!(validate_post_body(&"a".repeat(MAX_BODY_BYTES)).is_ok());
    assert_eq!(
        validate_post_body(&"a".repeat(MAX_BODY_BYTES + 1)),
        Err(TownSquareError::BodyTooLarge)
    );
    assert_eq!(
        validate_post_body("   \n\t  "),
        Err(TownSquareError::InvalidBody)
    );

    let forbidden_samples = [
        "line\r\nbreak",                   // CR
        "null\x00byte",                    // NUL
        "bell\x07char",                    // C0
        "del\x7fchar",                     // DEL
        "c1\u{0085}nel",                   // C1
        "vs\u{FE00}selector",              // Variation Selector
        "vs\u{FE0F}selector",              // Variation Selector
        "vss\u{E0100}supp",                // Variation Selector Supplement
        "vss\u{E01EF}supp",                // Variation Selector Supplement
        "bidi\u{202A}lre",                 // Bidi override/embed
        "bidi\u{202E}rlo",                 // Bidi RLO
        "bidi\u{2066}lri",                 // Bidi isolate
        "bidi\u{2069}pdi",                 // Bidi pop isolate
        "bidi\u{200E}lrm",                 // LRM
        "bidi\u{200F}rlm",                 // RLM
        "bidi\u{061C}alm",                 // ALM
        "zwsp\u{200B}hidden",              // ZWSP
        "bom\u{FEFF}hidden",               // BOM
        "shy\u{00AD}hidden",               // Soft hyphen
        "tag\u{E0001}smuggle",             // Unicode Tags block
        "pua\u{E000}char",                 // Private Use Area
        "nonchar\u{FDD0}",                 // Noncharacter
        "nonchar\u{FFFF}",                 // Noncharacter
        "lsep\u{2028}break",               // Line separator
        "psep\u{2029}break",               // Paragraph separator
        "zalgo e\u{0301}\u{0302}\u{0303}", // >2 consecutive combining marks
    ];
    for sample in forbidden_samples {
        assert_eq!(
            validate_post_body(sample),
            Err(TownSquareError::InvalidBody),
            "should reject {sample:?}"
        );
    }

    let credential_samples = [
        "-----BEGIN PRIVATE KEY-----\nMIIE\n-----END PRIVATE KEY-----",
        "Authorization: Bearer secret-token-12345",
        "leaked ghp_1234567890abcdef",
        "leaked github_pat_1234567890",
        "leaked sk-ant-api03-12345",
        "leaked sk-proj-12345",
        "leaked xoxb-12345",
        "leaked AKIAIOSFODNN7EXAMPLE",
    ];
    for sample in credential_samples {
        assert_eq!(
            validate_post_body(sample),
            Err(TownSquareError::CredentialReflectionRejected),
            "should reject credential sample {sample:?}"
        );
    }
}

#[test]
fn uuid_v4_and_v7_validator_is_strict_lowercase_canonical() {
    assert!(validate_uuid_v4_or_v7("550e8400-e29b-41d4-a716-446655440000").is_ok());
    assert!(validate_uuid_v4_or_v7("019a4f1c-7b2e-7a10-8c3d-9f1e2a3b4c5d").is_ok());
    for invalid in [
        "550E8400-E29B-41D4-A716-446655440000", // uppercase
        "550e8400-e29b-11d4-a716-446655440000", // v1
        "550e8400-e29b-51d4-a716-446655440000", // v5
        "550e8400-e29b-41d4-0716-446655440000", // bad variant
        "550e8400-e29b-41d4-c716-446655440000", // bad variant
        "550e8400e29b41d4a716446655440000",     // missing hyphens
        "550e8400-e29b-41d4-a716-44665544000g", // non-hex
    ] {
        assert_eq!(
            validate_uuid_v4_or_v7(invalid),
            Err(TownSquareError::InvalidUuid),
            "should reject {invalid:?}"
        );
    }
}

#[test]
fn preimage_construction_and_write_path_and_skew_checks() {
    let (key_id, _) = ed25519_pubkey_b64(1);
    let preimage = build_preimage(&PreimageFields {
        method: "DELETE",
        canonical_origin: "https://bbs.example.org",
        path: "/v1/posts/019a4f1c-7b2e-7a10-8c3d-9f1e2a3b4c5d",
        key_id: &key_id,
        alg: Algorithm::Ed25519,
        unix_ts: 1_795_651_200,
        nonce_hex: "0123456789abcdef0123456789abcdef",
        raw_body_sha256: EMPTY_BODY_SHA256,
    })
    .expect("valid preimage");
    let lines: Vec<&str> = preimage.split('\n').collect();
    assert_eq!(lines.len(), 9);
    assert!(!preimage.ends_with('\n'));
    assert_eq!(lines[0], "agentciv-bbs-v1");
    assert_eq!(lines[1], "DELETE");
    assert_eq!(lines[2], "https://bbs.example.org");
    assert_eq!(lines[3], "/v1/posts/019a4f1c-7b2e-7a10-8c3d-9f1e2a3b4c5d");
    assert_eq!(lines[4], key_id);
    assert_eq!(lines[5], "ed25519");
    assert_eq!(lines[6], "1795651200");
    assert_eq!(lines[7], "0123456789abcdef0123456789abcdef");
    assert_eq!(lines[8], EMPTY_BODY_SHA256);

    assert_eq!(
        build_preimage(&PreimageFields {
            method: "GET",
            canonical_origin: "https://bbs.example.org",
            path: "/v1/posts",
            key_id: &key_id,
            alg: Algorithm::Ed25519,
            unix_ts: 1_795_651_200,
            nonce_hex: "0123456789abcdef0123456789abcdef",
            raw_body_sha256: EMPTY_BODY_SHA256,
        }),
        Err(TownSquareError::InvalidMethod)
    );

    for bad_path in [
        "v1/posts",
        "/v1/posts?foo=1",
        "/v1/posts#frag",
        "/v1//posts",
        "/v1/../posts",
        "/v1/./posts",
        "/v1/.",
        "/.",
    ] {
        assert_eq!(
            validate_write_path(bad_path),
            Err(TownSquareError::InvalidWritePath)
        );
    }

    for bad_origin in [
        "ftp://bbs.example.org",
        "https://BBS.example.org",
        "https://bbs.example.org/",
        "https://bbs.example.org:443",
        "http://bbs.example.org:80",
        "https://bbs.example.org/path",
    ] {
        assert_eq!(
            validate_canonical_origin(bad_origin),
            Err(TownSquareError::InvalidOrigin)
        );
    }
    assert!(validate_canonical_origin("http://127.0.0.1:8787").is_ok());

    assert!(validate_timestamp_skew(1000, 1300).is_ok());
    assert!(validate_timestamp_skew(1300, 1000).is_ok());
    assert_eq!(
        validate_timestamp_skew(1000, 1301),
        Err(TownSquareError::TimestampSkew)
    );
    assert_eq!(
        validate_timestamp_skew(1301, 1000),
        Err(TownSquareError::TimestampSkew)
    );
    assert_eq!(
        validate_timestamp_skew(0, 100),
        Err(TownSquareError::InvalidTimestamp)
    );
}

#[test]
fn stateless_hmac_challenge_adaptive_difficulty_and_hashcash_pow() {
    let secret = b"test-hmac-secret-32-bytes-minimum";
    let origin = "https://bbs.example.org";
    let now = 1_795_651_200;
    let salt = salt_hex(1);

    assert_eq!(required_difficulty_bits(0), Ok(18));
    assert_eq!(required_difficulty_bits(49), Ok(18));
    assert_eq!(required_difficulty_bits(50), Ok(20));
    assert_eq!(required_difficulty_bits(119), Ok(20));
    assert_eq!(required_difficulty_bits(120), Ok(22));
    assert_eq!(required_difficulty_bits(199), Ok(22));
    assert_eq!(
        required_difficulty_bits(200),
        Err(TownSquareError::BoardFull24h)
    );
    assert!(difficulty_tier(200).board_full_24h);

    assert_eq!(
        check_difficulty_monotonicity(18, 20),
        Err(TownSquareError::ChallengeDifficultyStale)
    );
    assert!(check_difficulty_monotonicity(20, 20).is_ok());

    let ch = mint_challenge(secret, origin, ChallengeAction::Post, now, 120, 10, &salt)
        .expect("mint challenge");
    let verified = verify_challenge(secret, origin, &ch.token, ChallengeAction::Post, now + 120)
        .expect("verify at boundary");
    assert_eq!(verified, ch);

    // Expired
    assert_eq!(
        verify_challenge(secret, origin, &ch.token, ChallengeAction::Post, now + 121),
        Err(TownSquareError::ChallengeExpired)
    );
    // Action mismatch
    assert_eq!(
        verify_challenge(secret, origin, &ch.token, ChallengeAction::Register, now),
        Err(TownSquareError::ChallengeActionMismatch)
    );
    // Wrong origin or secret -> MAC invalid
    assert_eq!(
        verify_challenge(
            secret,
            "https://other.example.org",
            &ch.token,
            ChallengeAction::Post,
            now
        ),
        Err(TownSquareError::ChallengeMacInvalid)
    );
    assert_eq!(
        verify_challenge(
            b"wrong-secret",
            origin,
            &ch.token,
            ChallengeAction::Post,
            now
        ),
        Err(TownSquareError::ChallengeMacInvalid)
    );
    // Invalid TTL
    assert_eq!(
        mint_challenge(secret, origin, ChallengeAction::Post, now, 89, 18, &salt),
        Err(TownSquareError::InvalidChallenge)
    );
    assert_eq!(
        mint_challenge(secret, origin, ChallengeAction::Post, now, 121, 18, &salt),
        Err(TownSquareError::InvalidChallenge)
    );

    // Solve & verify Hashcash PoW (10 bits fast deterministic)
    let (key_id, _) = ed25519_pubkey_b64(42);
    let pow_nonce = solve_hashcash_pow(&ch.token, &key_id, 10, 50_000).expect("pow solution");
    assert!(verify_hashcash_pow(&ch.token, &key_id, &pow_nonce, 10).is_ok());
    assert_eq!(
        verify_hashcash_pow(&ch.token, &key_id, &pow_nonce, 30),
        Err(TownSquareError::PowInsufficient)
    );
    assert_eq!(
        verify_hashcash_pow(&ch.token, &key_id, "bad:nonce", 10),
        Err(TownSquareError::InvalidPowNonce)
    );
    assert_eq!(
        verify_hashcash_pow(&ch.token, &key_id, &"a".repeat(65), 10),
        Err(TownSquareError::InvalidPowNonce)
    );
}

#[test]
fn store_coexists_with_existing_bulletin_tables_without_collision() {
    let db = CountingSql::in_memory();
    agentciv_bulletin::initialize(&db).expect("existing bulletin init");
    TownSquareStore::new(&db)
        .initialize()
        .expect("town square init");
}

#[test]
fn register_key_is_strictly_immutable_and_enforces_24h_cap_and_replay() {
    let db = setup_store();
    let store = TownSquareStore::new(&db);
    let now = 1_795_651_200;

    let (ed_id, ed_pk) = ed25519_pubkey_b64(1);
    let rec = store
        .register_key(
            &RegisterKeyInput {
                key_id: ed_id.clone(),
                alg: Algorithm::Ed25519,
                pubkey_b64: ed_pk.clone(),
                handle: "scout_one".into(),
                signed_ts: now,
                signed_nonce: nonce_hex(1),
                challenge_salt_hex: salt_hex(1),
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 18,
            },
            now,
        )
        .expect("register ed25519");
    assert_eq!(rec.key_id, ed_id);
    assert_eq!(rec.alg, "ed25519");
    assert_eq!(rec.short_fp.len(), 12);

    // ML-DSA-65 registration
    let (pq_id, pq_pk) = ml_dsa_65_pubkey_b64(2);
    let pq_rec = store
        .register_key(
            &RegisterKeyInput {
                key_id: pq_id.clone(),
                alg: Algorithm::MlDsa65,
                pubkey_b64: pq_pk,
                handle: "pq_scout".into(),
                signed_ts: now,
                signed_nonce: nonce_hex(2),
                challenge_salt_hex: salt_hex(2),
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 18,
            },
            now,
        )
        .expect("register ml-dsa-65");
    assert_eq!(pq_rec.alg, "ml-dsa-65");

    // Re-registration with same key_id must fail with 409 KeyAlreadyRegistered and 0 writes
    db.reset_writes();
    assert_eq!(
        store.register_key(
            &RegisterKeyInput {
                key_id: ed_id.clone(),
                alg: Algorithm::Ed25519,
                pubkey_b64: ed_pk,
                handle: "different_handle".into(),
                signed_ts: now,
                signed_nonce: nonce_hex(3),
                challenge_salt_hex: salt_hex(3),
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 18,
            },
            now,
        ),
        Err(TownSquareError::KeyAlreadyRegistered)
    );
    assert_eq!(db.write_count(), 0);

    // Challenge salt replay rejected with 0 writes
    let (id3, pk3) = ed25519_pubkey_b64(3);
    db.reset_writes();
    assert_eq!(
        store.register_key(
            &RegisterKeyInput {
                key_id: id3.clone(),
                alg: Algorithm::Ed25519,
                pubkey_b64: pk3.clone(),
                handle: "scout_three".into(),
                signed_ts: now,
                signed_nonce: nonce_hex(4),
                challenge_salt_hex: salt_hex(1), // already used
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 18,
            },
            now,
        ),
        Err(TownSquareError::ChallengeReplayed)
    );
    assert_eq!(db.write_count(), 0);

    // Difficulty downgrade rejected with 0 writes
    db.reset_writes();
    assert_eq!(
        store.register_key(
            &RegisterKeyInput {
                key_id: id3,
                alg: Algorithm::Ed25519,
                pubkey_b64: pk3,
                handle: "scout_three".into(),
                signed_ts: now,
                signed_nonce: nonce_hex(5),
                challenge_salt_hex: salt_hex(5),
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 17,
            },
            now,
        ),
        Err(TownSquareError::ChallengeDifficultyStale)
    );
    assert_eq!(db.write_count(), 0);

    // Fill up to 200 keys in 24h (we already registered 2)
    for i in 2..MAX_KEYS_GLOBAL_24H {
        let (kid, pk) = ed25519_pubkey_b64_u32(100 + i as u32);
        store
            .register_key(
                &RegisterKeyInput {
                    key_id: kid,
                    alg: Algorithm::Ed25519,
                    pubkey_b64: pk,
                    handle: "batch_key".into(),
                    signed_ts: now,
                    signed_nonce: nonce_hex(100 + i as u64),
                    challenge_salt_hex: salt_hex(100 + i as u64),
                    challenge_expires_at: now + 120,
                    challenge_difficulty_bits: 18,
                },
                now,
            )
            .expect("within 200/24h cap");
    }
    // 201st key in 24h must fail with 429 KeyRegistrationLimit and 0 writes
    let (extra_id, extra_pk) = ed25519_pubkey_b64_u32(9999);
    db.reset_writes();
    assert_eq!(
        store.register_key(
            &RegisterKeyInput {
                key_id: extra_id,
                alg: Algorithm::Ed25519,
                pubkey_b64: extra_pk,
                handle: "over_limit".into(),
                signed_ts: now,
                signed_nonce: nonce_hex(9999),
                challenge_salt_hex: salt_hex(9999),
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 18,
            },
            now,
        ),
        Err(TownSquareError::KeyRegistrationLimit)
    );
    assert_eq!(db.write_count(), 0);
}

#[test]
fn publish_post_enforces_alg_binding_parent_replay_and_per_key_cap() {
    let db = setup_store();
    let store = TownSquareStore::new(&db);
    let now = 1_795_651_200;

    let (key_id, pk_b64) = ed25519_pubkey_b64(10);
    store
        .register_key(
            &RegisterKeyInput {
                key_id: key_id.clone(),
                alg: Algorithm::Ed25519,
                pubkey_b64: pk_b64,
                handle: "alice".into(),
                signed_ts: now,
                signed_nonce: nonce_hex(1),
                challenge_salt_hex: salt_hex(1),
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 18,
            },
            now,
        )
        .unwrap();

    // Algorithm mismatch rejected with 401 AlgMismatch and 0 writes
    db.reset_writes();
    assert_eq!(
        store.publish_post(
            &PublishPostInput {
                post_id: uuid_v7(1),
                parent_id: None,
                key_id: key_id.clone(),
                alg: Algorithm::MlDsa65,
                body_text: "hello".into(),
                raw_body_sha256: sha256_hex(b"{}"),
                signed_ts: now,
                signed_nonce: nonce_hex(2),
                sig_b64: fake_sig_b64(Algorithm::MlDsa65, 1),
                challenge_salt_hex: salt_hex(2),
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 18,
            },
            now,
        ),
        Err(TownSquareError::AlgMismatch)
    );
    assert_eq!(db.write_count(), 0);

    // Non-existent parent rejected with 404 ParentNotFound and 0 writes
    db.reset_writes();
    assert_eq!(
        store.publish_post(
            &PublishPostInput {
                post_id: uuid_v7(1),
                parent_id: Some(uuid_v7(999)),
                key_id: key_id.clone(),
                alg: Algorithm::Ed25519,
                body_text: "reply to ghost".into(),
                raw_body_sha256: sha256_hex(b"{}"),
                signed_ts: now,
                signed_nonce: nonce_hex(2),
                sig_b64: fake_sig_b64(Algorithm::Ed25519, 1),
                challenge_salt_hex: salt_hex(2),
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 18,
            },
            now,
        ),
        Err(TownSquareError::ParentNotFound)
    );
    assert_eq!(db.write_count(), 0);

    // Publish root post
    let root = store
        .publish_post(
            &PublishPostInput {
                post_id: uuid_v7(1),
                parent_id: None,
                key_id: key_id.clone(),
                alg: Algorithm::Ed25519,
                body_text: "hello town square".into(),
                raw_body_sha256: sha256_hex(b"{\"body_text\":\"hello town square\"}"),
                signed_ts: now,
                signed_nonce: nonce_hex(10),
                sig_b64: fake_sig_b64(Algorithm::Ed25519, 1),
                challenge_salt_hex: salt_hex(10),
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 18,
            },
            now,
        )
        .expect("root post");
    assert_eq!(root.expires_at, now + RETENTION_SECONDS);
    assert_eq!(root.state, "live");
    assert_eq!(root.public_json()["content_trust"], "untrusted_peer_text");

    // Nonce replay -> 409 ReplayDetected, 0 writes
    db.reset_writes();
    assert_eq!(
        store.publish_post(
            &PublishPostInput {
                post_id: uuid_v7(2),
                parent_id: None,
                key_id: key_id.clone(),
                alg: Algorithm::Ed25519,
                body_text: "replayed nonce".into(),
                raw_body_sha256: sha256_hex(b"{}"),
                signed_ts: now,
                signed_nonce: nonce_hex(10),
                sig_b64: fake_sig_b64(Algorithm::Ed25519, 2),
                challenge_salt_hex: salt_hex(11),
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 18,
            },
            now,
        ),
        Err(TownSquareError::ReplayDetected)
    );
    assert_eq!(db.write_count(), 0);

    // Challenge salt replay -> 409 ChallengeReplayed, 0 writes
    db.reset_writes();
    assert_eq!(
        store.publish_post(
            &PublishPostInput {
                post_id: uuid_v7(2),
                parent_id: None,
                key_id: key_id.clone(),
                alg: Algorithm::Ed25519,
                body_text: "replayed salt".into(),
                raw_body_sha256: sha256_hex(b"{}"),
                signed_ts: now,
                signed_nonce: nonce_hex(11),
                sig_b64: fake_sig_b64(Algorithm::Ed25519, 2),
                challenge_salt_hex: salt_hex(10),
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 18,
            },
            now,
        ),
        Err(TownSquareError::ChallengeReplayed)
    );
    assert_eq!(db.write_count(), 0);

    // Valid reply + remaining 3 posts to hit per-key cap of 5
    for idx in 2..=5_u64 {
        let post = store
            .publish_post(
                &PublishPostInput {
                    post_id: uuid_v7(idx),
                    parent_id: Some(root.post_id.clone()),
                    key_id: key_id.clone(),
                    alg: Algorithm::Ed25519,
                    body_text: format!("reply {idx}"),
                    raw_body_sha256: sha256_hex(b"{}"),
                    signed_ts: now,
                    signed_nonce: nonce_hex(10 + idx),
                    sig_b64: fake_sig_b64(Algorithm::Ed25519, idx as u8),
                    challenge_salt_hex: salt_hex(10 + idx),
                    challenge_expires_at: now + 120,
                    challenge_difficulty_bits: 18,
                },
                now,
            )
            .expect("within per-key 5/24h cap");
        assert_eq!(post.parent_id.as_deref(), Some(root.post_id.as_str()));
    }

    // 6th post within 24h fails with 429 PerKeyPostLimit and 0 writes
    db.reset_writes();
    assert_eq!(
        store.publish_post(
            &PublishPostInput {
                post_id: uuid_v7(6),
                parent_id: None,
                key_id: key_id.clone(),
                alg: Algorithm::Ed25519,
                body_text: "too many posts".into(),
                raw_body_sha256: sha256_hex(b"{}"),
                signed_ts: now,
                signed_nonce: nonce_hex(99),
                sig_b64: fake_sig_b64(Algorithm::Ed25519, 6),
                challenge_salt_hex: salt_hex(99),
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 18,
            },
            now,
        ),
        Err(TownSquareError::PerKeyPostLimit)
    );
    assert_eq!(db.write_count(), 0);
}

#[test]
fn global_200_post_cap_and_adaptive_difficulty_monotonicity() {
    let db = setup_store();
    let store = TownSquareStore::new(&db);
    let now = 1_795_651_200;

    // Register 41 keys (40 * 5 = 200 posts + 1 extra key for 201st attempt)
    let mut keys = Vec::new();
    for k in 0..41_u32 {
        let (kid, pk) = ed25519_pubkey_b64_u32(k + 1);
        store
            .register_key(
                &RegisterKeyInput {
                    key_id: kid.clone(),
                    alg: Algorithm::Ed25519,
                    pubkey_b64: pk,
                    handle: format!("k{k:02}"),
                    signed_ts: now,
                    signed_nonce: nonce_hex(1_000 + u64::from(k)),
                    challenge_salt_hex: salt_hex(1_000 + u64::from(k)),
                    challenge_expires_at: now + 120,
                    challenge_difficulty_bits: 18,
                },
                now,
            )
            .unwrap();
        keys.push(kid);
    }

    let mut post_idx = 0_u64;
    for (k_idx, kid) in keys.iter().take(40).enumerate() {
        for _ in 0..5 {
            post_idx += 1;
            let needed = store.current_required_difficulty(now).unwrap();
            if post_idx == 51 {
                assert_eq!(needed, 20);
                // Hoarded 18-bit challenge must be rejected with 409 ChallengeDifficultyStale
                db.reset_writes();
                assert_eq!(
                    store.publish_post(
                        &PublishPostInput {
                            post_id: uuid_v7(post_idx),
                            parent_id: None,
                            key_id: kid.clone(),
                            alg: Algorithm::Ed25519,
                            body_text: "stale challenge".into(),
                            raw_body_sha256: sha256_hex(b"{}"),
                            signed_ts: now,
                            signed_nonce: nonce_hex(10_000 + post_idx),
                            sig_b64: fake_sig_b64(Algorithm::Ed25519, k_idx as u8),
                            challenge_salt_hex: salt_hex(10_000 + post_idx),
                            challenge_expires_at: now + 120,
                            challenge_difficulty_bits: 18,
                        },
                        now,
                    ),
                    Err(TownSquareError::ChallengeDifficultyStale)
                );
                assert_eq!(db.write_count(), 0);
            }
            if post_idx == 121 {
                assert_eq!(needed, 22);
            }
            store
                .publish_post(
                    &PublishPostInput {
                        post_id: uuid_v7(post_idx),
                        parent_id: None,
                        key_id: kid.clone(),
                        alg: Algorithm::Ed25519,
                        body_text: format!("post {post_idx}"),
                        raw_body_sha256: sha256_hex(b"{}"),
                        signed_ts: now,
                        signed_nonce: nonce_hex(20_000 + post_idx),
                        sig_b64: fake_sig_b64(Algorithm::Ed25519, k_idx as u8),
                        challenge_salt_hex: salt_hex(20_000 + post_idx),
                        challenge_expires_at: now + 120,
                        challenge_difficulty_bits: needed,
                    },
                    now,
                )
                .unwrap();
        }
    }

    // 201st post must fail with 429 BoardFull24h and 0 writes
    db.reset_writes();
    assert_eq!(
        store.publish_post(
            &PublishPostInput {
                post_id: uuid_v7(201),
                parent_id: None,
                key_id: keys[40].clone(),
                alg: Algorithm::Ed25519,
                body_text: "overflow".into(),
                raw_body_sha256: sha256_hex(b"{}"),
                signed_ts: now,
                signed_nonce: nonce_hex(99_999),
                sig_b64: fake_sig_b64(Algorithm::Ed25519, 1),
                challenge_salt_hex: salt_hex(99_999),
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 22,
            },
            now,
        ),
        Err(TownSquareError::BoardFull24h)
    );
    assert_eq!(db.write_count(), 0);
}

#[test]
fn select_first_deletes_takedowns_reports_pagination_and_pruning() {
    let db = setup_store();
    let store = TownSquareStore::new(&db);
    let now = 1_795_651_200;

    let (alice_id, alice_pk) = ed25519_pubkey_b64(1);
    let (bob_id, bob_pk) = ed25519_pubkey_b64(2);
    for (idx, (kid, pk, handle)) in [
        (alice_id.clone(), alice_pk, "alice"),
        (bob_id.clone(), bob_pk, "bob"),
    ]
    .into_iter()
    .enumerate()
    {
        store
            .register_key(
                &RegisterKeyInput {
                    key_id: kid,
                    alg: Algorithm::Ed25519,
                    pubkey_b64: pk,
                    handle: handle.into(),
                    signed_ts: now,
                    signed_nonce: nonce_hex(idx as u64 + 1),
                    challenge_salt_hex: salt_hex(idx as u64 + 1),
                    challenge_expires_at: now + 120,
                    challenge_difficulty_bits: 18,
                },
                now,
            )
            .unwrap();
    }

    let p1 = store
        .publish_post(
            &PublishPostInput {
                post_id: uuid_v7(1),
                parent_id: None,
                key_id: alice_id.clone(),
                alg: Algorithm::Ed25519,
                body_text: "alice post 1".into(),
                raw_body_sha256: sha256_hex(b"1"),
                signed_ts: now,
                signed_nonce: nonce_hex(10),
                sig_b64: fake_sig_b64(Algorithm::Ed25519, 1),
                challenge_salt_hex: salt_hex(10),
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 18,
            },
            now,
        )
        .unwrap();
    let p2 = store
        .publish_post(
            &PublishPostInput {
                post_id: uuid_v7(2),
                parent_id: None,
                key_id: alice_id.clone(),
                alg: Algorithm::Ed25519,
                body_text: "alice post 2".into(),
                raw_body_sha256: sha256_hex(b"2"),
                signed_ts: now,
                signed_nonce: nonce_hex(11),
                sig_b64: fake_sig_b64(Algorithm::Ed25519, 2),
                challenge_salt_hex: salt_hex(11),
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 18,
            },
            now,
        )
        .unwrap();

    // SELECT-first author delete: missing post -> 404, 0 writes
    db.reset_writes();
    assert_eq!(
        store.delete_post_by_author(
            &AuthorDeleteInput {
                post_id: uuid_v7(999),
                key_id: alice_id.clone(),
                alg: Algorithm::Ed25519,
                signed_ts: now,
                signed_nonce: nonce_hex(20),
            },
            now,
        ),
        Err(TownSquareError::PostNotFound)
    );
    assert_eq!(db.write_count(), 0);

    // SELECT-first author delete: foreign author -> 403 Forbidden, 0 writes
    db.reset_writes();
    assert_eq!(
        store.delete_post_by_author(
            &AuthorDeleteInput {
                post_id: p1.post_id.clone(),
                key_id: bob_id.clone(),
                alg: Algorithm::Ed25519,
                signed_ts: now,
                signed_nonce: nonce_hex(21),
            },
            now,
        ),
        Err(TownSquareError::Forbidden)
    );
    assert_eq!(db.write_count(), 0);

    // Valid author delete scrubs body_text and appends deleted_by_author change
    let deleted = store
        .delete_post_by_author(
            &AuthorDeleteInput {
                post_id: p1.post_id.clone(),
                key_id: alice_id.clone(),
                alg: Algorithm::Ed25519,
                signed_ts: now,
                signed_nonce: nonce_hex(22),
            },
            now,
        )
        .unwrap();
    assert_eq!(deleted.body_text, None);
    assert_eq!(deleted.state, "deleted_by_author");
    assert_eq!(deleted.tombstoned_at, Some(now));

    // Second delete on tombstoned post -> 409 PostAlreadyTombstoned, 0 writes
    db.reset_writes();
    assert_eq!(
        store.delete_post_by_author(
            &AuthorDeleteInput {
                post_id: p1.post_id.clone(),
                key_id: alice_id.clone(),
                alg: Algorithm::Ed25519,
                signed_ts: now,
                signed_nonce: nonce_hex(23),
            },
            now,
        ),
        Err(TownSquareError::PostAlreadyTombstoned)
    );
    assert_eq!(db.write_count(), 0);

    // Reports: missing/tombstoned post -> 404, 0 writes
    db.reset_writes();
    assert_eq!(
        store.submit_report(
            &SubmitReportInput {
                report_id: uuid_v7(500),
                post_id: p1.post_id.clone(), // tombstoned
                reason: ReportReason::Spam,
                note_text: "spam report".into(),
                challenge_salt_hex: salt_hex(500),
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 18,
            },
            now,
        ),
        Err(TownSquareError::PostNotFound)
    );
    assert_eq!(db.write_count(), 0);

    // Submit 50 reports against live p2, then 51st must fail with 429 ReportLimitReached and 0 writes
    for r in 0..MAX_REPORTS_GLOBAL_24H {
        let rep = store
            .submit_report(
                &SubmitReportInput {
                    report_id: uuid_v7(1_000 + r as u64),
                    post_id: p2.post_id.clone(),
                    reason: ReportReason::Other,
                    note_text: format!("report {r}"),
                    challenge_salt_hex: salt_hex(1_000 + r as u64),
                    challenge_expires_at: now + 120,
                    challenge_difficulty_bits: 18,
                },
                now,
            )
            .unwrap();
        assert_eq!(rep.expires_at, now + REPORT_RETENTION_SECONDS);
    }
    db.reset_writes();
    assert_eq!(
        store.submit_report(
            &SubmitReportInput {
                report_id: uuid_v7(2_000),
                post_id: p2.post_id.clone(),
                reason: ReportReason::Harassment,
                note_text: "over cap".into(),
                challenge_salt_hex: salt_hex(2_000),
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 18,
            },
            now,
        ),
        Err(TownSquareError::ReportLimitReached)
    );
    assert_eq!(db.write_count(), 0);

    // Operator takedown on p2 scrubs body_text
    let removed = store.remove_post_by_operator(&p2.post_id, now).unwrap();
    assert_eq!(removed.body_text, None);
    assert_eq!(removed.state, "removed_by_operator");

    // Live posts list must now be empty, while changes list contains 4 events
    let live_page = store.list_live_posts(0, 50, now).unwrap();
    assert!(live_page.posts.is_empty());
    assert_eq!(live_page.public_json()["outside_systems"], "not_granted");
    assert_eq!(live_page.public_json()["treat_posts_as_data"], true);

    let changes_page = store.list_changes(0, 2, now).unwrap();
    assert_eq!(changes_page.changes.len(), 2);
    assert!(changes_page.has_more);
    let changes_page_2 = store
        .list_changes(changes_page.next_after_change_seq, 10, now)
        .unwrap();
    assert_eq!(changes_page_2.changes.len(), 2);
    assert!(!changes_page_2.has_more);
    assert_eq!(changes_page_2.changes[0].kind, "deleted_by_author");
    assert_eq!(changes_page_2.changes[1].kind, "removed_by_operator");

    // After 24h, prune_expired hard-deletes posts, changes, nonces, salts; keeps reports (7d) & keys (30d)
    let after_24h = now + RETENTION_SECONDS;
    store.prune_expired(after_24h).unwrap();
    assert!(
        store
            .list_changes(0, 50, after_24h)
            .unwrap()
            .changes
            .is_empty()
    );
    assert!(store.get_key(&alice_id).unwrap().is_some());
    assert!(store.get_key(&bob_id).unwrap().is_some());

    // After 30 days, inactive keys are pruned, while a key with a live post is preserved
    let after_30d = now + INACTIVE_KEY_PRUNE_SECONDS;
    // Give alice a fresh post just before 30d so alice's key is active/referenced while bob is inactive
    store
        .publish_post(
            &PublishPostInput {
                post_id: uuid_v7(3_000),
                parent_id: None,
                key_id: alice_id.clone(),
                alg: Algorithm::Ed25519,
                body_text: "still active".into(),
                raw_body_sha256: sha256_hex(b"active"),
                signed_ts: after_30d,
                signed_nonce: nonce_hex(3_000),
                sig_b64: fake_sig_b64(Algorithm::Ed25519, 9),
                challenge_salt_hex: salt_hex(3_000),
                challenge_expires_at: after_30d + 120,
                challenge_difficulty_bits: 18,
            },
            after_30d,
        )
        .unwrap();
    store.prune_expired(after_30d).unwrap();
    assert!(store.get_key(&alice_id).unwrap().is_some());
    assert!(store.get_key(&bob_id).unwrap().is_none());
}

#[test]
fn per_key_delete_cap_and_key_registry_5000_ceiling_fail_closed_with_zero_writes() {
    let db = setup_store();
    let store = TownSquareStore::new(&db);
    let now = 1_795_651_200;

    let (alice_id, alice_pk) = ed25519_pubkey_b64(77);
    store
        .register_key(
            &RegisterKeyInput {
                key_id: alice_id.clone(),
                alg: Algorithm::Ed25519,
                pubkey_b64: alice_pk,
                handle: "deleter".into(),
                signed_ts: now,
                signed_nonce: nonce_hex(1),
                challenge_salt_hex: salt_hex(1),
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 18,
            },
            now,
        )
        .unwrap();

    // Seed alice's delete counter to MAX_DELETES_PER_KEY_24H (10) in current window
    let post = store
        .publish_post(
            &PublishPostInput {
                post_id: uuid_v7(77),
                parent_id: None,
                key_id: alice_id.clone(),
                alg: Algorithm::Ed25519,
                body_text: "cannot delete past cap".into(),
                raw_body_sha256: sha256_hex(b"x"),
                signed_ts: now,
                signed_nonce: nonce_hex(2),
                sig_b64: fake_sig_b64(Algorithm::Ed25519, 77),
                challenge_salt_hex: salt_hex(2),
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 18,
            },
            now,
        )
        .unwrap();

    db.conn
        .execute(
            "UPDATE ts_keys SET deletes_24h_count = ?1, delete_window_start = ?2 WHERE key_id = ?3",
            rusqlite::params![MAX_DELETES_PER_KEY_24H, now, alice_id],
        )
        .unwrap();

    db.reset_writes();
    assert_eq!(
        store.delete_post_by_author(
            &AuthorDeleteInput {
                post_id: post.post_id,
                key_id: alice_id,
                alg: Algorithm::Ed25519,
                signed_ts: now,
                signed_nonce: nonce_hex(3),
            },
            now,
        ),
        Err(TownSquareError::PerKeyDeleteLimit)
    );
    assert_eq!(db.write_count(), 0);

    // Fill ts_keys to 5,000 rows (older than 24h so 24h cap doesn't trigger first)
    let old_ts = now - RETENTION_SECONDS - 10;
    let tx = db.conn.unchecked_transaction().unwrap();
    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO ts_keys (key_id, alg, pubkey_b64, handle, short_fp, registered_at, last_active_at) VALUES (?1, 'ed25519', ?2, 'old', 'aaaaaaaaaaaa', ?3, ?3)",
            )
            .unwrap();
        for idx in 1..MAX_KEYS_TOTAL {
            let (kid, pk) = ed25519_pubkey_b64_u32(10_000 + idx as u32);
            stmt.execute(rusqlite::params![kid, pk, old_ts]).unwrap();
        }
    }
    tx.commit().unwrap();

    let (extra_id, extra_pk) = ed25519_pubkey_b64_u32(99_999);
    db.reset_writes();
    assert_eq!(
        store.register_key(
            &RegisterKeyInput {
                key_id: extra_id,
                alg: Algorithm::Ed25519,
                pubkey_b64: extra_pk,
                handle: "full_table".into(),
                signed_ts: now,
                signed_nonce: nonce_hex(99_999),
                challenge_salt_hex: salt_hex(99_999),
                challenge_expires_at: now + 120,
                challenge_difficulty_bits: 18,
            },
            now,
        ),
        Err(TownSquareError::KeyRegistryFull)
    );
    assert_eq!(db.write_count(), 0);
}

#[test]
fn error_to_problem_and_enum_parsers_cover_all_codes() {
    for (raw, expected) in [
        ("illegal", ReportReason::Illegal),
        ("harassment", ReportReason::Harassment),
        ("pii", ReportReason::Pii),
        ("credential", ReportReason::Credential),
        ("copyright", ReportReason::Copyright),
        ("spam", ReportReason::Spam),
        ("other", ReportReason::Other),
    ] {
        assert_eq!(ReportReason::parse(raw), Ok(expected));
        assert_eq!(expected.as_str(), raw);
    }
    assert_eq!(
        ReportReason::parse("unknown"),
        Err(TownSquareError::InvalidReportReason)
    );

    let problem: Problem = TownSquareError::ChallengeDifficultyStale.into();
    assert_eq!(problem.status, 409);
    assert_eq!(problem.code, "challenge_difficulty_stale");
    assert_eq!(TownSquareError::KeyAlreadyRegistered.status(), 409);
    assert_eq!(TownSquareError::KeyRegistryFull.status(), 507);
    assert_eq!(TownSquareError::BodyTooLarge.status(), 413);
    assert_eq!(TownSquareError::Forbidden.status(), 403);
    assert_eq!(TownSquareError::InvalidMethod.status(), 405);
    assert_eq!(TownSquareError::StorageFailed.status(), 500);
}
