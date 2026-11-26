use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use reqwest::blocking::{Client, Response};
use serde_json::{Value, json};

use crate::merkle::{decode_hex, empty_root_hash, encode_hex, verify_consistency_proof};
use crate::store::Store;
use crate::{Credential, HostConfig, Visibility, start_test_host};

fn config_in(dir: &std::path::Path) -> HostConfig {
    HostConfig {
        world_id: "civ:merkle-test".to_owned(),
        title: "Merkle Test Room".to_owned(),
        database_path: dir.join("world.sqlite"),
        listen: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        visibility: Visibility::Members,
        retention_seconds: 3600,
        max_payload_bytes: 4096,
        credentials: vec![
            Credential {
                principal: "agent:writer".to_owned(),
                token: "token-writer".to_owned(),
                read: true,
                write: true,
            },
            Credential {
                principal: "agent:write-only".to_owned(),
                token: "token-write-only".to_owned(),
                read: false,
                write: true,
            },
        ],
    }
}

fn body_json(response: Response) -> Value {
    let text = response.text().expect("response text");
    serde_json::from_str(&text).expect("valid json")
}

fn submit_msg(client: &Client, base: &str, token: &str, id: &str, text: &str) -> Value {
    let body = json!({
        "protocol_version": "0.1-draft",
        "type": "message",
        "id": id,
        "world": "civ:merkle-test",
        "from": "agent:writer",
        "to": ["agent:writer"],
        "body": { "text": text }
    });
    let resp = client
        .post(format!("{base}/submit"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .expect("submit");
    assert_eq!(resp.status(), 200);
    body_json(resp)
}

fn decode_proof(value: &Value) -> Vec<[u8; 32]> {
    value
        .as_array()
        .expect("proof array")
        .iter()
        .map(|item| decode_hex(item.as_str().expect("hex")).expect("valid 32-byte hex"))
        .collect()
}

#[test]
fn checkpoint_http_auth_discovery_and_query_validation() {
    let dir = tempfile::tempdir().expect("tempdir");
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let host = rt
        .block_on(start_test_host(config_in(dir.path())))
        .expect("host");
    let base = host
        .discovery
        .strip_suffix("/.well-known/agentciv")
        .expect("base");
    let client = Client::new();

    // Discovery advertises checkpoint endpoint and format.
    let discovery = body_json(client.get(&host.discovery).send().expect("discover"));
    assert_eq!(
        discovery["endpoints"]["checkpoint"],
        format!("{base}/checkpoint")
    );
    assert_eq!(
        discovery["checkpoints"]["format"],
        "c2sp.org/tlog-checkpoint"
    );
    assert_eq!(discovery["checkpoints"]["path"], "/checkpoint");
    assert_eq!(discovery["authentication"]["checkpoint"], "bearer");

    // Unauthenticated request -> 401 with zero metadata leak.
    let unauth = client
        .get(format!("{base}/checkpoint"))
        .send()
        .expect("unauth");
    assert_eq!(unauth.status(), 401);
    assert_eq!(
        unauth
            .headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(body_json(unauth)["code"], "authentication_required");

    // Write-only credential -> 403 forbidden.
    let forbidden = client
        .get(format!("{base}/checkpoint"))
        .header("Authorization", "Bearer token-write-only")
        .send()
        .expect("forbidden");
    assert_eq!(forbidden.status(), 403);
    assert_eq!(body_json(forbidden)["code"], "forbidden");

    // Empty tree checkpoint -> tree_size == 0, RFC 9162 empty root hash.
    let empty_resp = client
        .get(format!("{base}/checkpoint"))
        .header("Authorization", "Bearer token-writer")
        .send()
        .expect("empty checkpoint");
    assert_eq!(empty_resp.status(), 200);
    assert_eq!(
        empty_resp
            .headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("no-store")
    );
    let empty = body_json(empty_resp);
    assert_eq!(empty["format"], "c2sp.org/tlog-checkpoint");
    assert_eq!(empty["world_id"], "civ:merkle-test");
    assert_eq!(empty["tree_size"], 0);
    assert_eq!(empty["root_hash_sha256"], encode_hex(&empty_root_hash()));
    assert!(empty.get("consistency_proof").is_none());

    // Invalid from_size queries -> 400 invalid_checkpoint_query.
    for bad in ["0", "01", "abc", "1", "-1"] {
        let resp = client
            .get(format!("{base}/checkpoint?from_size={bad}"))
            .header("Authorization", "Bearer token-writer")
            .send()
            .expect("bad query");
        assert_eq!(resp.status(), 400, "from_size={bad}");
        assert_eq!(body_json(resp)["code"], "invalid_checkpoint_query");
    }
}

#[test]
fn checkpoint_tombstone_invariance_and_consistency_over_http() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("world.sqlite");
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let host = rt
        .block_on(start_test_host(config_in(dir.path())))
        .expect("host");
    let base = host
        .discovery
        .strip_suffix("/.well-known/agentciv")
        .expect("base");
    let client = Client::new();

    // Submit 3 messages and take checkpoint M = 3.
    submit_msg(&client, base, "token-writer", "msg:1", "first");
    submit_msg(&client, base, "token-writer", "msg:2", "second");
    submit_msg(&client, base, "token-writer", "msg:3", "third");

    let cp3 = body_json(
        client
            .get(format!("{base}/checkpoint"))
            .header("Authorization", "Bearer token-writer")
            .send()
            .expect("cp3"),
    );
    assert_eq!(cp3["tree_size"], 3);
    let root3 = decode_hex(cp3["root_hash_sha256"].as_str().expect("root3")).expect("hex");
    let note3 = cp3["checkpoint"].as_str().expect("note3");
    assert!(note3.starts_with("agentciv/civ:merkle-test\n3\n"));
    assert!(note3.ends_with('\n'));

    // Redact event 1 with an `event.redacted` tombstone in SQLite.
    let store = Store::open(
        &db_path,
        "civ:merkle-test",
        Visibility::Members,
        3600,
        &[
            ("agent:writer".to_owned(), true, true),
            ("agent:write-only".to_owned(), false, true),
        ],
    )
    .expect("reopen store");
    assert!(store.redact_event(1).expect("redact event 1"));

    // Confirm event 1 is now served as an `event.redacted` tombstone with empty body.
    let page = body_json(
        client
            .get(format!("{base}/events"))
            .header("Authorization", "Bearer token-writer")
            .send()
            .expect("events"),
    );
    assert_eq!(page["events"][0]["kind"], "event.redacted");
    assert_eq!(page["events"][0]["body"], json!({}));

    // Checkpoint root at M = 3 MUST remain byte-for-byte identical after redaction!
    let cp3_after_redaction = body_json(
        client
            .get(format!("{base}/checkpoint"))
            .header("Authorization", "Bearer token-writer")
            .send()
            .expect("cp3 after redaction"),
    );
    assert_eq!(cp3_after_redaction["tree_size"], 3);
    assert_eq!(
        cp3_after_redaction["root_hash_sha256"],
        cp3["root_hash_sha256"]
    );

    // Submit 2 more messages + an artifact revision + withdraw that artifact revision.
    submit_msg(&client, base, "token-writer", "msg:4", "fourth");
    submit_msg(&client, base, "token-writer", "msg:5", "fifth");

    let artifact = json!({
        "protocol_version": "0.1-draft",
        "type": "artifact_revision",
        "id": "sub:art-1",
        "world": "civ:merkle-test",
        "from": "agent:writer",
        "to": ["agent:writer"],
        "artifact_id": "artifact:tombstone-check",
        "media_type": "application/json",
        "body": { "text": "draft" }
    });
    let art_resp = client
        .post(format!("{base}/collaborate"))
        .header("Authorization", "Bearer token-writer")
        .header("Content-Type", "application/json")
        .body(artifact.to_string())
        .send()
        .expect("artifact");
    assert_eq!(art_resp.status(), 200);

    let withdrawal = json!({
        "protocol_version": "0.1-draft",
        "type": "withdrawal",
        "id": "sub:withdraw-1",
        "world": "civ:merkle-test",
        "from": "agent:writer",
        "target_from": "agent:writer",
        "artifact_id": "artifact:tombstone-check",
        "revision": 1
    });
    let withdraw_resp = client
        .post(format!("{base}/collaborate"))
        .header("Authorization", "Bearer token-writer")
        .header("Content-Type", "application/json")
        .body(withdrawal.to_string())
        .send()
        .expect("withdrawal");
    assert_eq!(withdraw_resp.status(), 200);

    // Fetch checkpoint N = 6 with consistency proof from M = 3.
    let cp6 = body_json(
        client
            .get(format!("{base}/checkpoint?from_size=3"))
            .header("Authorization", "Bearer token-writer")
            .send()
            .expect("cp6"),
    );
    assert_eq!(cp6["tree_size"], 6);
    let root6 = decode_hex(cp6["root_hash_sha256"].as_str().expect("root6")).expect("hex");
    let proof = decode_proof(&cp6["consistency_proof"]);

    // Consistency proof (3 -> 6) MUST PASS despite both `event.redacted` and `artifact.withdrawn` tombstones!
    assert!(verify_consistency_proof(3, 6, &root3, &root6, &proof));

    // Rollback, split-view, and tampered proof MUST FAIL.
    assert!(!verify_consistency_proof(6, 3, &root6, &root3, &[]));
    let mut forged = root3;
    forged[0] ^= 0x01;
    assert!(!verify_consistency_proof(3, 3, &root3, &forged, &[]));
    let mut tampered = proof.clone();
    tampered[0][0] ^= 0x80;
    assert!(!verify_consistency_proof(3, 6, &root3, &root6, &tampered));
}
