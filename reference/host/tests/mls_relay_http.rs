//! Black-box loopback HTTP adversarial integration tests for `mls-relay/0.1-draft` (Stage 2B, #209).

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc;
use std::thread;

use agentciv_host::merkle;
use agentciv_host::mls_relay::{
    DEFAULT_RETENTION_SECONDS, MAX_ENVELOPES_PER_GROUP, MAX_KEY_PACKAGES_PER_PRINCIPAL,
    MAX_OPAQUE_BLOB_BYTES, MlsRelayStore, PROFILE_ID, RelayConfig, SUPPORTED_CIPHERSUITES,
};
use agentciv_host::mls_relay_http::{
    self, MAX_COMMIT_BODY_BYTES, MAX_JSON_BODY_BYTES, MlsRelayState,
};
use reqwest::StatusCode;
use reqwest::blocking::{Client, Response};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

const PQ_SUITE: &str = "MLS_256_X25519MLKEM768_CHACHA20POLY1305_SHA512_MLDSA65";
const T0: i64 = 1_764_300_000;

struct TestRelayServer {
    base_url: String,
    clock: Arc<AtomicI64>,
    shutdown: Option<oneshot::Sender<()>>,
    handle: Option<thread::JoinHandle<()>>,
}

impl TestRelayServer {
    fn start(config: RelayConfig) -> Self {
        let clock = Arc::new(AtomicI64::new(T0));
        let clock_for_state = Arc::clone(&clock);
        let store = MlsRelayStore::open_in_memory(config).expect("open in-memory relay store");
        let state =
            MlsRelayState::with_clock(store, move || clock_for_state.load(Ordering::SeqCst));
        let app = mls_relay_http::router(state);

        let (addr_tx, addr_rx) = mpsc::channel::<SocketAddr>();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        let handle = thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("build tokio runtime");
            rt.block_on(async move {
                let listener = TcpListener::bind("127.0.0.1:0")
                    .await
                    .expect("bind loopback listener");
                let addr = listener.local_addr().expect("local_addr");
                addr_tx.send(addr).expect("send bound address");
                axum::serve(listener, app)
                    .with_graceful_shutdown(async move {
                        let _ = shutdown_rx.await;
                    })
                    .await
                    .expect("serve mls relay router");
            });
        });

        let addr = addr_rx.recv().expect("receive bound address");
        Self {
            base_url: format!("http://{addr}"),
            clock,
            shutdown: Some(shutdown_tx),
            handle: Some(handle),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url)
    }

    fn advance_seconds(&self, delta: i64) {
        self.clock.fetch_add(delta, Ordering::SeqCst);
    }
}

impl Drop for TestRelayServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn client() -> Client {
    Client::builder().build().expect("build reqwest client")
}

fn b64(bytes: &[u8]) -> String {
    merkle::encode_base64(bytes)
}

fn assert_security_headers(resp: &Response) {
    assert_eq!(
        resp.headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(
        resp.headers()
            .get("x-content-type-options")
            .and_then(|v| v.to_str().ok()),
        Some("nosniff")
    );
    assert_eq!(
        resp.headers()
            .get("content-security-policy")
            .and_then(|v| v.to_str().ok()),
        Some("default-src 'none'; frame-ancestors 'none'")
    );
    assert_eq!(
        resp.headers()
            .get("referrer-policy")
            .and_then(|v| v.to_str().ok()),
        Some("no-referrer")
    );
}

fn assert_problem_json(resp: Response, expected_status: StatusCode, expected_code: &str) -> Value {
    assert_eq!(resp.status(), expected_status);
    assert_security_headers(&resp);
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        content_type.starts_with("application/problem+json"),
        "expected application/problem+json, got {content_type}"
    );
    let body: Value = serde_json::from_str(&resp.text().expect("read problem body"))
        .expect("problem body is valid JSON");
    assert_eq!(body["status"], expected_status.as_u16());
    assert_eq!(body["code"], expected_code);
    assert_eq!(
        body["type"],
        format!("https://agentciv.io/problems/{expected_code}")
    );
    body
}

#[test]
fn writes_disabled_default_fails_closed_with_503_on_all_write_routes() {
    let server = TestRelayServer::start(RelayConfig::default());
    let http = client();

    // GET /mls/v1/info reports writes_enabled: false and full relay metadata.
    let info_resp = http
        .get(server.url("/mls/v1/info"))
        .send()
        .expect("GET /mls/v1/info");
    assert_eq!(info_resp.status(), StatusCode::OK);
    assert_security_headers(&info_resp);
    let info: Value = serde_json::from_str(&info_resp.text().expect("info text")).expect("json");
    assert_eq!(info["profile"], PROFILE_ID);
    assert_eq!(info["writes_enabled"], false);
    assert_eq!(info["retention_seconds"], DEFAULT_RETENTION_SECONDS);
    assert_eq!(info["max_opaque_blob_bytes"], MAX_OPAQUE_BLOB_BYTES);
    assert_eq!(info["max_envelopes_per_group"], MAX_ENVELOPES_PER_GROUP);
    assert_eq!(info["content_trust"], "untrusted_opaque_ciphertext");
    assert_eq!(info["outside_systems"], "not_granted");
    assert_eq!(
        info["supported_ciphersuites"],
        json!(SUPPORTED_CIPHERSUITES)
    );

    // All POST write routes fail closed with 503 application/problem+json.
    let resp = http
        .post(server.url("/mls/v1/key-packages"))
        .header("content-type", "application/json")
        .body(
            json!({
                "kp_id": "kp:1",
                "principal_id": "alice#01",
                "ciphersuite": PQ_SUITE,
                "blob_b64": b64(b"kp-blob")
            })
            .to_string(),
        )
        .send()
        .expect("POST key-packages");
    assert_problem_json(resp, StatusCode::SERVICE_UNAVAILABLE, "writes_disabled");

    let resp = http
        .post(server.url("/mls/v1/key-packages/consume"))
        .header("content-type", "application/json")
        .body(json!({ "principal_id": "alice#01" }).to_string())
        .send()
        .expect("POST key-packages/consume");
    assert_problem_json(resp, StatusCode::SERVICE_UNAVAILABLE, "writes_disabled");

    let resp = http
        .post(server.url("/mls/v1/groups"))
        .header("content-type", "application/json")
        .body(json!({ "group_id": "grp:1", "ciphersuite": PQ_SUITE }).to_string())
        .send()
        .expect("POST groups");
    assert_problem_json(resp, StatusCode::SERVICE_UNAVAILABLE, "writes_disabled");

    let resp = http
        .post(server.url("/mls/v1/groups/grp:1/commits"))
        .header("content-type", "application/json")
        .body(
            json!({
                "expected_epoch": 0,
                "commit_blob_b64": b64(b"commit-0")
            })
            .to_string(),
        )
        .send()
        .expect("POST commits");
    assert_problem_json(resp, StatusCode::SERVICE_UNAVAILABLE, "writes_disabled");

    let resp = http
        .post(server.url("/mls/v1/groups/grp:1/messages"))
        .header("content-type", "application/json")
        .body(
            json!({
                "epoch": 0,
                "blob_b64": b64(b"msg-0")
            })
            .to_string(),
        )
        .send()
        .expect("POST messages");
    assert_problem_json(resp, StatusCode::SERVICE_UNAVAILABLE, "writes_disabled");

    // POST /mls/v1/prune remains callable when writes_enabled is false so TTL expiry is enforceable.
    let prune_resp = http
        .post(server.url("/mls/v1/prune"))
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .expect("POST prune");
    assert_eq!(prune_resp.status(), StatusCode::OK);
    assert_security_headers(&prune_resp);
    let prune: Value = serde_json::from_str(&prune_resp.text().expect("prune text")).expect("json");
    assert_eq!(prune["envelopes_deleted"], 0);
    assert_eq!(prune["key_packages_deleted"], 0);
    assert_eq!(prune["groups_deleted"], 0);
}

#[test]
fn full_two_participant_pq_mls_relay_lifecycle_and_merkle_consistency() {
    let server = TestRelayServer::start(RelayConfig {
        writes_enabled: true,
        retention_seconds: DEFAULT_RETENTION_SECONDS,
    });
    let http = client();

    // 1. Bob publishes a single-use PQ KeyPackage.
    let kp_resp = http
        .post(server.url("/mls/v1/key-packages"))
        .header("content-type", "application/json")
        .body(
            json!({
                "kp_id": "kp:bob-01",
                "principal_id": "bob#02",
                "ciphersuite": PQ_SUITE,
                "blob_b64": b64(b"pq-keypackage-bob-01")
            })
            .to_string(),
        )
        .send()
        .expect("publish kp");
    assert_eq!(kp_resp.status(), StatusCode::CREATED);
    assert_security_headers(&kp_resp);
    let kp: Value = serde_json::from_str(&kp_resp.text().expect("kp body")).expect("json");
    assert_eq!(kp["kp_id"], "kp:bob-01");
    assert_eq!(kp["consumed"], false);

    // Duplicate kp_id fails with 409 conflict.
    let dup_kp = http
        .post(server.url("/mls/v1/key-packages"))
        .header("content-type", "application/json")
        .body(
            json!({
                "kp_id": "kp:bob-01",
                "principal_id": "bob#02",
                "ciphersuite": PQ_SUITE,
                "blob_b64": b64(b"other")
            })
            .to_string(),
        )
        .send()
        .expect("dup kp");
    assert_problem_json(dup_kp, StatusCode::CONFLICT, "conflict");

    // 2. Alice consumes Bob's KeyPackage (200 OK).
    let consume1 = http
        .post(server.url("/mls/v1/key-packages/consume"))
        .header("content-type", "application/json")
        .body(json!({ "principal_id": "bob#02" }).to_string())
        .send()
        .expect("consume 1");
    assert_eq!(consume1.status(), StatusCode::OK);
    assert_security_headers(&consume1);
    let consumed: Value =
        serde_json::from_str(&consume1.text().expect("consume text")).expect("json");
    assert_eq!(consumed["kp_id"], "kp:bob-01");
    assert_eq!(consumed["consumed"], true);

    // 3. Second consume returns 404 not_found (single-use guarantee!).
    let consume2 = http
        .post(server.url("/mls/v1/key-packages/consume"))
        .header("content-type", "application/json")
        .body(json!({ "principal_id": "bob#02" }).to_string())
        .send()
        .expect("consume 2");
    assert_problem_json(consume2, StatusCode::NOT_FOUND, "not_found");

    // 4. Alice creates a PQ-MLS group at epoch 0.
    let grp_resp = http
        .post(server.url("/mls/v1/groups"))
        .header("content-type", "application/json")
        .body(
            json!({
                "group_id": "grp:pq-room-1",
                "ciphersuite": PQ_SUITE
            })
            .to_string(),
        )
        .send()
        .expect("create group");
    assert_eq!(grp_resp.status(), StatusCode::CREATED);
    assert_security_headers(&grp_resp);
    let grp: Value = serde_json::from_str(&grp_resp.text().expect("grp body")).expect("json");
    assert_eq!(grp["group_id"], "grp:pq-room-1");
    assert_eq!(grp["current_epoch"], 0);

    // GET /mls/v1/groups/{group_id} returns metadata (200 OK); unknown group returns 404.
    let get_grp = http
        .get(server.url("/mls/v1/groups/grp:pq-room-1"))
        .send()
        .expect("get group");
    assert_eq!(get_grp.status(), StatusCode::OK);
    assert_security_headers(&get_grp);
    let get_missing = http
        .get(server.url("/mls/v1/groups/grp:missing"))
        .send()
        .expect("get missing group");
    assert_problem_json(get_missing, StatusCode::NOT_FOUND, "not_found");

    // 5. Alice submits Commit epoch 0 -> 1 with a Welcome for Bob.
    let commit1_resp = http
        .post(server.url("/mls/v1/groups/grp:pq-room-1/commits"))
        .header("content-type", "application/json")
        .body(
            json!({
                "expected_epoch": 0,
                "commit_blob_b64": b64(b"commit-epoch-0-to-1"),
                "welcomes": [
                    {
                        "recipient_id": "bob#02",
                        "blob_b64": b64(b"welcome-for-bob-epoch-1")
                    }
                ]
            })
            .to_string(),
        )
        .send()
        .expect("submit commit 1");
    assert_eq!(commit1_resp.status(), StatusCode::CREATED);
    assert_security_headers(&commit1_resp);
    let receipt1: Value =
        serde_json::from_str(&commit1_resp.text().expect("receipt 1")).expect("json");
    assert_eq!(receipt1["previous_epoch"], 0);
    assert_eq!(receipt1["new_epoch"], 1);
    assert_eq!(receipt1["welcome_seqs"].as_array().map(Vec::len), Some(1));

    // Checkpoint at tree_size = 1 (after Commit 1).
    let cp1_resp = http
        .get(server.url("/mls/v1/groups/grp:pq-room-1/checkpoint"))
        .send()
        .expect("checkpoint 1");
    assert_eq!(cp1_resp.status(), StatusCode::OK);
    assert_security_headers(&cp1_resp);
    let cp1: Value = serde_json::from_str(&cp1_resp.text().expect("cp1")).expect("json");
    assert_eq!(cp1["origin"], "agentciv-mls/grp:pq-room-1");
    assert_eq!(cp1["tree_size"], 1);
    let root1_hex = cp1["root_hash_hex"].as_str().expect("root1_hex").to_owned();

    // 6. Competing Commit at stale epoch 0 fails with 409 epoch_conflict and current_epoch: 1.
    let stale_commit = http
        .post(server.url("/mls/v1/groups/grp:pq-room-1/commits"))
        .header("content-type", "application/json")
        .body(
            json!({
                "expected_epoch": 0,
                "commit_blob_b64": b64(b"competing-commit-at-0")
            })
            .to_string(),
        )
        .send()
        .expect("stale commit");
    let conflict = assert_problem_json(stale_commit, StatusCode::CONFLICT, "epoch_conflict");
    assert_eq!(conflict["current_epoch"], 1);

    // 7. Submit PrivateMessage at epoch 1 (and verify stale epoch 0 fails with 409).
    let stale_msg = http
        .post(server.url("/mls/v1/groups/grp:pq-room-1/messages"))
        .header("content-type", "application/json")
        .body(
            json!({
                "epoch": 0,
                "blob_b64": b64(b"stale-pm")
            })
            .to_string(),
        )
        .send()
        .expect("stale msg");
    let msg_conflict = assert_problem_json(stale_msg, StatusCode::CONFLICT, "epoch_conflict");
    assert_eq!(msg_conflict["current_epoch"], 1);

    let msg1_resp = http
        .post(server.url("/mls/v1/groups/grp:pq-room-1/messages"))
        .header("content-type", "application/json")
        .body(
            json!({
                "epoch": 1,
                "blob_b64": b64(b"encrypted-hello-epoch-1")
            })
            .to_string(),
        )
        .send()
        .expect("submit msg 1");
    assert_eq!(msg1_resp.status(), StatusCode::CREATED);
    assert_security_headers(&msg1_resp);
    let msg1: Value = serde_json::from_str(&msg1_resp.text().expect("msg1")).expect("json");
    assert_eq!(msg1["kind"], "private_message");
    assert_eq!(msg1["epoch"], 1);

    // Submit a second Commit (epoch 1 -> 2) to verify multi-epoch consistency proof.
    let commit2_resp = http
        .post(server.url("/mls/v1/groups/grp:pq-room-1/commits"))
        .header("content-type", "application/json")
        .body(
            json!({
                "expected_epoch": 1,
                "commit_blob_b64": b64(b"commit-epoch-1-to-2")
            })
            .to_string(),
        )
        .send()
        .expect("submit commit 2");
    assert_eq!(commit2_resp.status(), StatusCode::CREATED);

    // 8. Fetch paginated envelopes and welcomes.
    let env_page1 = http
        .get(server.url("/mls/v1/groups/grp:pq-room-1/envelopes?after_seq=0&limit=2"))
        .send()
        .expect("fetch envelopes page 1");
    assert_eq!(env_page1.status(), StatusCode::OK);
    assert_security_headers(&env_page1);
    let envs1: Value = serde_json::from_str(&env_page1.text().expect("envs1")).expect("json");
    assert_eq!(envs1["envelopes"].as_array().map(Vec::len), Some(2));
    let next_seq = envs1["next_after_seq"].as_u64().expect("next_after_seq");
    assert!(next_seq > 0);

    let env_page2 = http
        .get(server.url(&format!(
            "/mls/v1/groups/grp:pq-room-1/envelopes?after_seq={next_seq}&limit=10"
        )))
        .send()
        .expect("fetch envelopes page 2");
    assert_eq!(env_page2.status(), StatusCode::OK);
    let envs2: Value = serde_json::from_str(&env_page2.text().expect("envs2")).expect("json");
    assert_eq!(envs2["envelopes"].as_array().map(Vec::len), Some(1));
    assert_eq!(envs2["envelopes"][0]["kind"], "commit");
    assert_eq!(envs2["envelopes"][0]["epoch"], 2);

    let welcomes_resp = http
        .get(server.url("/mls/v1/welcomes/bob%2302?after_seq=0&limit=10"))
        .send()
        .expect("fetch welcomes");
    assert_eq!(welcomes_resp.status(), StatusCode::OK);
    assert_security_headers(&welcomes_resp);
    let welcomes: Value =
        serde_json::from_str(&welcomes_resp.text().expect("welcomes")).expect("json");
    assert_eq!(welcomes["welcomes"].as_array().map(Vec::len), Some(1));
    assert_eq!(welcomes["welcomes"][0]["kind"], "welcome");
    assert_eq!(welcomes["welcomes"][0]["recipient_id"], "bob#02");

    // 9. Verify RFC 9162 Merkle checkpoint + consistency proof from tree_size=1 to tree_size=3.
    let cp3_resp = http
        .get(server.url("/mls/v1/groups/grp:pq-room-1/checkpoint?old_size=1"))
        .send()
        .expect("checkpoint 3 with proof");
    assert_eq!(cp3_resp.status(), StatusCode::OK);
    assert_security_headers(&cp3_resp);
    let cp3: Value = serde_json::from_str(&cp3_resp.text().expect("cp3")).expect("json");
    assert_eq!(cp3["tree_size"], 3);
    assert_eq!(cp3["consistency_from_size"], 1);
    let root3_hex = cp3["root_hash_hex"].as_str().expect("root3_hex");
    let proof_hex: Vec<String> = cp3["consistency_proof_hex"]
        .as_array()
        .expect("proof array")
        .iter()
        .map(|v| v.as_str().expect("hex str").to_owned())
        .collect();
    let proof_hashes: Vec<[u8; 32]> = proof_hex
        .iter()
        .map(|h| merkle::decode_hex(h).expect("valid proof hex"))
        .collect();
    let old_root = merkle::decode_hex(&root1_hex).expect("old root hex");
    let new_root = merkle::decode_hex(root3_hex).expect("new root hex");
    assert!(
        merkle::verify_consistency_proof(1, 3, &old_root, &new_root, &proof_hashes),
        "RFC 9162 consistency proof must verify across epochs"
    );
}

#[test]
fn adversarial_inputs_content_type_body_limits_duplicate_keys_and_ttl_prune() {
    let server = TestRelayServer::start(RelayConfig {
        writes_enabled: true,
        retention_seconds: 3_600,
    });
    let http = client();

    // 1. Wrong Content-Type -> 415 unsupported_media_type.
    let bad_ct = http
        .post(server.url("/mls/v1/groups"))
        .header("content-type", "text/plain")
        .body(json!({ "group_id": "grp:ct", "ciphersuite": PQ_SUITE }).to_string())
        .send()
        .expect("wrong content-type");
    assert_problem_json(
        bad_ct,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "unsupported_media_type",
    );

    let bad_prune_ct = http
        .post(server.url("/mls/v1/prune"))
        .header("content-type", "text/plain")
        .body("{}")
        .send()
        .expect("wrong prune content-type");
    assert_problem_json(
        bad_prune_ct,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "unsupported_media_type",
    );

    // 1b. Duplicate Content-Type headers -> 415 unsupported_media_type.
    let mut dup_ct_headers = reqwest::header::HeaderMap::new();
    dup_ct_headers.append(
        reqwest::header::CONTENT_TYPE,
        reqwest::header::HeaderValue::from_static("application/json"),
    );
    dup_ct_headers.append(
        reqwest::header::CONTENT_TYPE,
        reqwest::header::HeaderValue::from_static("text/plain"),
    );
    let dup_ct = http
        .post(server.url("/mls/v1/groups"))
        .headers(dup_ct_headers)
        .body(json!({ "group_id": "grp:dupct", "ciphersuite": PQ_SUITE }).to_string())
        .send()
        .expect("duplicate content-type");
    assert_problem_json(
        dup_ct,
        StatusCode::UNSUPPORTED_MEDIA_TYPE,
        "unsupported_media_type",
    );

    // 1c. Invalid UTF-8 path parameter -> 400 invalid_input application/problem+json.
    let bad_utf8_path = http
        .get(server.url("/mls/v1/groups/%FF"))
        .send()
        .expect("invalid utf-8 path");
    assert_problem_json(bad_utf8_path, StatusCode::BAD_REQUEST, "invalid_input");

    // 2. Unknown JSON field -> 400 invalid_input.
    let unknown_field = http
        .post(server.url("/mls/v1/groups"))
        .header("content-type", "application/json")
        .body(
            json!({
                "group_id": "grp:extra",
                "ciphersuite": PQ_SUITE,
                "evil_extra_field": true
            })
            .to_string(),
        )
        .send()
        .expect("unknown field");
    assert_problem_json(unknown_field, StatusCode::BAD_REQUEST, "invalid_input");

    // 3. Duplicate JSON object key -> 400 invalid_input.
    let dup_key = http
        .post(server.url("/mls/v1/groups"))
        .header("content-type", "application/json")
        .body(format!(
            r#"{{"group_id":"grp:dup","group_id":"grp:dup2","ciphersuite":"{PQ_SUITE}"}}"#
        ))
        .send()
        .expect("duplicate key");
    assert_problem_json(dup_key, StatusCode::BAD_REQUEST, "invalid_input");

    // 4. Unsupported ciphersuite -> 400 unsupported_ciphersuite.
    let bad_suite = http
        .post(server.url("/mls/v1/groups"))
        .header("content-type", "application/json")
        .body(json!({ "group_id": "grp:badsuite", "ciphersuite": "MLS_NULL_CIPHER" }).to_string())
        .send()
        .expect("bad suite");
    assert_problem_json(
        bad_suite,
        StatusCode::BAD_REQUEST,
        "unsupported_ciphersuite",
    );

    // 5. Oversized JSON body on standard POST (> 128 KiB) -> 413 payload_too_large.
    let oversized_msg = "A".repeat(MAX_JSON_BODY_BYTES + 1024);
    let too_large_post = http
        .post(server.url("/mls/v1/groups"))
        .header("content-type", "application/json")
        .body(format!(
            r#"{{"group_id":"grp:big","ciphersuite":"{oversized_msg}"}}"#
        ))
        .send()
        .expect("oversized post");
    assert_problem_json(
        too_large_post,
        StatusCode::PAYLOAD_TOO_LARGE,
        "payload_too_large",
    );

    // Oversized JSON body on /commits (> 256 KiB) -> 413 payload_too_large.
    let oversized_commit = "A".repeat(MAX_COMMIT_BODY_BYTES + 1024);
    let too_large_commit = http
        .post(server.url("/mls/v1/groups/grp:x/commits"))
        .header("content-type", "application/json")
        .body(format!(
            r#"{{"expected_epoch":0,"commit_blob_b64":"{oversized_commit}"}}"#
        ))
        .send()
        .expect("oversized commit");
    assert_problem_json(
        too_large_commit,
        StatusCode::PAYLOAD_TOO_LARGE,
        "payload_too_large",
    );

    // Create a valid group for query-parameter, quota, and TTL tests.
    let created = http
        .post(server.url("/mls/v1/groups"))
        .header("content-type", "application/json")
        .body(json!({ "group_id": "grp:adv", "ciphersuite": PQ_SUITE }).to_string())
        .send()
        .expect("create grp:adv");
    assert_eq!(created.status(), StatusCode::CREATED);

    // Decoded blob > MAX_OPAQUE_BLOB_BYTES (within HTTP body limit) -> 413 from store.
    let big_blob = b64(&vec![0xAB; MAX_OPAQUE_BLOB_BYTES + 1]);
    let big_blob_resp = http
        .post(server.url("/mls/v1/groups/grp:adv/messages"))
        .header("content-type", "application/json")
        .body(json!({ "epoch": 0, "blob_b64": big_blob }).to_string())
        .send()
        .expect("oversized opaque blob");
    assert_problem_json(
        big_blob_resp,
        StatusCode::PAYLOAD_TOO_LARGE,
        "payload_too_large",
    );

    // 6. Malformed after_seq, limit, old_size, and unknown query params -> 400 invalid_input.
    for bad_url in [
        "/mls/v1/groups/grp:adv/envelopes?after_seq=-1",
        "/mls/v1/groups/grp:adv/envelopes?after_seq=abc",
        "/mls/v1/groups/grp:adv/envelopes?after_seq=01",
        "/mls/v1/groups/grp:adv/envelopes?limit=0",
        "/mls/v1/groups/grp:adv/envelopes?limit=201",
        "/mls/v1/groups/grp:adv/envelopes?unknown=1",
        "/mls/v1/welcomes/bob%2302?after_seq=-5",
        "/mls/v1/groups/grp:adv/checkpoint?old_size=-1",
        "/mls/v1/groups/grp:adv/checkpoint?old_size=notanumber",
        "/mls/v1/groups/grp:adv/checkpoint?old_size=99",
        "/mls/v1/groups/grp:adv/checkpoint?extra=1",
    ] {
        let resp = http.get(server.url(bad_url)).send().expect("bad query");
        assert_problem_json(resp, StatusCode::BAD_REQUEST, "invalid_input");
    }

    // 7. Unknown route -> 404 problem JSON; wrong method -> 405 problem JSON.
    let missing_route = http
        .get(server.url("/mls/v1/does-not-exist"))
        .send()
        .expect("404 route");
    assert_problem_json(missing_route, StatusCode::NOT_FOUND, "not_found");

    let bad_method = http
        .delete(server.url("/mls/v1/info"))
        .send()
        .expect("405 method");
    assert_problem_json(
        bad_method,
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
    );

    // 8. Quota exceeded -> 429 quota_exceeded.
    for i in 0..MAX_KEY_PACKAGES_PER_PRINCIPAL {
        let resp = http
            .post(server.url("/mls/v1/key-packages"))
            .header("content-type", "application/json")
            .body(
                json!({
                    "kp_id": format!("kp:quota-{i}"),
                    "principal_id": "carol#03",
                    "ciphersuite": PQ_SUITE,
                    "blob_b64": b64(b"kp-quota")
                })
                .to_string(),
            )
            .send()
            .expect("publish quota kp");
        assert_eq!(resp.status(), StatusCode::CREATED);
    }
    let over_quota = http
        .post(server.url("/mls/v1/key-packages"))
        .header("content-type", "application/json")
        .body(
            json!({
                "kp_id": "kp:quota-overflow",
                "principal_id": "carol#03",
                "ciphersuite": PQ_SUITE,
                "blob_b64": b64(b"kp-quota")
            })
            .to_string(),
        )
        .send()
        .expect("over quota kp");
    assert_problem_json(over_quota, StatusCode::TOO_MANY_REQUESTS, "quota_exceeded");

    // 9. Populate a Commit + Welcome on grp:adv, advance deterministic clock past TTL, and prune!
    let commit = http
        .post(server.url("/mls/v1/groups/grp:adv/commits"))
        .header("content-type", "application/json")
        .body(
            json!({
                "expected_epoch": 0,
                "commit_blob_b64": b64(b"expiring-commit"),
                "welcomes": [
                    {
                        "recipient_id": "dave#04",
                        "blob_b64": b64(b"expiring-welcome")
                    }
                ]
            })
            .to_string(),
        )
        .send()
        .expect("expiring commit");
    assert_eq!(commit.status(), StatusCode::CREATED);

    // Advance clock past the 3600-second retention window.
    server.advance_seconds(3_601);

    // Expired group is immediately invisible on GET.
    let expired_get = http
        .get(server.url("/mls/v1/groups/grp:adv"))
        .send()
        .expect("expired group get");
    assert_problem_json(expired_get, StatusCode::NOT_FOUND, "not_found");

    // POST /mls/v1/prune hard-deletes expired group, envelopes (commit + welcome), and key packages.
    let prune_resp = http
        .post(server.url("/mls/v1/prune"))
        .header("content-type", "application/json")
        .body("{}")
        .send()
        .expect("prune expired");
    assert_eq!(prune_resp.status(), StatusCode::OK);
    assert_security_headers(&prune_resp);
    let report: Value =
        serde_json::from_str(&prune_resp.text().expect("prune report")).expect("json");
    assert_eq!(report["groups_deleted"], 1);
    assert_eq!(report["envelopes_deleted"], 2);
    assert_eq!(
        report["key_packages_deleted"],
        MAX_KEY_PACKAGES_PER_PRINCIPAL
    );

    // Second prune is a clean no-op.
    let prune_again = http
        .post(server.url("/mls/v1/prune"))
        .header("content-type", "application/json")
        .send()
        .expect("prune again empty body");
    assert_eq!(prune_again.status(), StatusCode::OK);
    let report2: Value =
        serde_json::from_str(&prune_again.text().expect("prune report 2")).expect("json");
    assert_eq!(report2["groups_deleted"], 0);
    assert_eq!(report2["envelopes_deleted"], 0);
    assert_eq!(report2["key_packages_deleted"], 0);
}

struct ChildGuard(std::process::Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_mls_relay_cli(extra_args: &[&str]) -> (ChildGuard, Value) {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_agentciv-host"));
    cmd.arg("mls-relay")
        .args(extra_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn agentciv-host mls-relay");
    let stdout = child.stdout.take().expect("piped stdout");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    let bytes = reader
        .read_line(&mut line)
        .expect("read readiness line from mls-relay");
    assert!(
        bytes > 0,
        "mls-relay exited without printing readiness JSON"
    );
    let readiness: Value = serde_json::from_str(line.trim()).expect("valid readiness JSON");
    (ChildGuard(child), readiness)
}

#[test]
fn mls_relay_cli_help_and_argument_validation() {
    use std::process::Command;

    for flag in ["--help", "-h"] {
        let output = Command::new(env!("CARGO_BIN_EXE_agentciv-host"))
            .args(["mls-relay", flag])
            .output()
            .expect("run mls-relay --help");
        assert!(output.status.success(), "expected exit 0 for {flag}");
        assert!(output.stderr.is_empty());
        let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
        assert!(stdout.contains("agentciv-host mls-relay"));
        assert!(stdout.contains("--db"));
        assert!(stdout.contains("--writes-enabled"));
    }

    for bad_args in [
        vec!["mls-relay"],
        vec!["mls-relay", "--host", "0.0.0.0", "--db", "/tmp/x.db"],
        vec!["mls-relay", "--host", "192.0.2.1", "--db", "/tmp/x.db"],
        vec!["mls-relay", "--host", "::", "--db", "/tmp/x.db"],
        vec!["mls-relay", "--host", "example.com", "--db", "/tmp/x.db"],
        vec![
            "mls-relay",
            "--db",
            "/tmp/x.db",
            "--retention-seconds",
            "59",
        ],
        vec![
            "mls-relay",
            "--db",
            "/tmp/x.db",
            "--retention-seconds",
            "86401",
        ],
        vec!["mls-relay", "--db", "/tmp/x.db", "--port", "70000"],
        vec!["mls-relay", "--db", "/tmp/x.db", "--unknown-flag"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_agentciv-host"))
            .args(&bad_args)
            .output()
            .expect("run invalid mls-relay args");
        assert!(
            !output.status.success(),
            "expected failure for args {bad_args:?}"
        );
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}

#[test]
fn mls_relay_cli_serves_closed_by_default_and_writes_enabled_opt_in() {
    let dir = tempfile::tempdir().expect("tempdir");
    let http = client();

    // 1. Default mode: writes_enabled is false (503 writes_disabled on write routes).
    let db_closed = dir.path().join("closed.sqlite");
    let db_closed_str = db_closed.to_str().expect("utf8 db path");
    let (_closed_guard, closed_ready) =
        spawn_mls_relay_cli(&["--db", db_closed_str, "--port", "0"]);
    assert_eq!(closed_ready["outcome"], "serving");
    assert_eq!(closed_ready["profile"], PROFILE_ID);
    assert_eq!(closed_ready["writes_enabled"], false);
    assert_eq!(closed_ready["retention_seconds"], DEFAULT_RETENTION_SECONDS);
    let closed_origin = closed_ready["origin"].as_str().expect("origin str");
    assert!(closed_origin.starts_with("http://127.0.0.1:"));

    let info_closed = http
        .get(format!("{closed_origin}/mls/v1/info"))
        .send()
        .expect("GET info closed");
    assert_eq!(info_closed.status(), StatusCode::OK);
    let info_closed_json: Value =
        serde_json::from_str(&info_closed.text().expect("info text")).expect("json");
    assert_eq!(info_closed_json["writes_enabled"], false);

    let write_closed = http
        .post(format!("{closed_origin}/mls/v1/groups"))
        .header("content-type", "application/json")
        .body(json!({ "group_id": "grp:cli-closed", "ciphersuite": PQ_SUITE }).to_string())
        .send()
        .expect("POST groups closed");
    assert_problem_json(
        write_closed,
        StatusCode::SERVICE_UNAVAILABLE,
        "writes_disabled",
    );
    drop(_closed_guard);

    // 2. Opt-in --writes-enabled with custom --retention-seconds.
    let db_open = dir.path().join("open.sqlite");
    let db_open_str = db_open.to_str().expect("utf8 db path");
    let (_open_guard, open_ready) = spawn_mls_relay_cli(&[
        "--db",
        db_open_str,
        "--host",
        "127.0.0.1",
        "--port",
        "0",
        "--retention-seconds",
        "3600",
        "--writes-enabled",
    ]);
    assert_eq!(open_ready["outcome"], "serving");
    assert_eq!(open_ready["profile"], PROFILE_ID);
    assert_eq!(open_ready["writes_enabled"], true);
    assert_eq!(open_ready["retention_seconds"], 3600);
    let open_origin = open_ready["origin"].as_str().expect("open origin str");

    let info_open = http
        .get(format!("{open_origin}/mls/v1/info"))
        .send()
        .expect("GET info open");
    assert_eq!(info_open.status(), StatusCode::OK);
    let info_open_json: Value =
        serde_json::from_str(&info_open.text().expect("info open text")).expect("json");
    assert_eq!(info_open_json["writes_enabled"], true);
    assert_eq!(info_open_json["retention_seconds"], 3600);

    let created = http
        .post(format!("{open_origin}/mls/v1/groups"))
        .header("content-type", "application/json")
        .body(json!({ "group_id": "grp:cli-open", "ciphersuite": PQ_SUITE }).to_string())
        .send()
        .expect("POST groups open");
    assert_eq!(created.status(), StatusCode::CREATED);
}
