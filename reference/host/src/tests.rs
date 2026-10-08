use std::time::Duration;

use agentciv_conformance::{CaseStatus, run, run_extended_with_peer};
use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;
use tempfile::tempdir;

use crate::store::{ReadError, Store, StoreError, SubmitError};
use crate::{
    Credential, HostConfig, HostError, Visibility, checked_config, load_config, start_test_host,
};

fn config(dir: &std::path::Path, visibility: Visibility) -> HostConfig {
    HostConfig {
        world_id: "civ:local".to_owned(),
        title: "Local".to_owned(),
        database_path: dir.join("world.sqlite"),
        listen: "127.0.0.1:0".parse().expect("loopback"),
        visibility,
        retention_seconds: 60,
        max_payload_bytes: 1024,
        credentials: vec![
            Credential {
                principal: "agent:abc123".to_owned(),
                token: "token-writer".to_owned(),
                read: true,
                write: true,
            },
            Credential {
                principal: "agent:reader".to_owned(),
                token: "token-reader".to_owned(),
                read: true,
                write: false,
            },
        ],
    }
}

fn message(id: &str, text: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "protocol_version": "0.1-draft",
        "type": "message",
        "id": id,
        "world": "civ:local",
        "from": "agent:abc123",
        "to": ["agent:reader"],
        "body": {"text": text},
        "note": {"keep": true}
    }))
    .expect("message")
}

fn client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("client")
}

fn body_json(response: reqwest::blocking::Response) -> serde_json::Value {
    let text = response.text().expect("body");
    serde_json::from_str(&text).expect("json")
}

#[test]
fn configuration_rejects_non_loopback_and_duplicate_tokens() {
    let dir = tempdir().expect("temp");
    let mut host = config(dir.path(), Visibility::Members);
    host.listen = "0.0.0.0:8787".parse().expect("address");
    assert!(checked_config(host).is_err());
    let mut host = config(dir.path(), Visibility::Members);
    host.credentials[1].token = host.credentials[0].token.clone();
    assert!(checked_config(host).is_err());
}

#[test]
fn configuration_file_round_trip() {
    let dir = tempdir().expect("temp");
    let path = dir.path().join("host.json");
    std::fs::write(
        &path,
        r#"{
          "world_id": "civ:local",
          "title": "Local",
          "database_path": "world.sqlite",
          "listen": "127.0.0.1:0",
          "visibility": "members",
          "retention_seconds": 60,
          "max_payload_bytes": 1024,
          "credentials": [
            {"principal": "agent:abc123", "token": "token-writer", "read": true, "write": true}
          ]
        }"#,
    )
    .expect("write");
    let loaded = load_config(&path).expect("config");
    assert_eq!(loaded.world_id, "civ:local");
    assert_eq!(loaded.credentials.len(), 1);
}

#[test]
fn failed_commit_leaves_no_event_and_a_dropped_transaction_rolls_back() {
    let dir = tempdir().expect("temp");
    let host = config(dir.path(), Visibility::Members);
    let grants = vec![("agent:abc123".to_owned(), true, true)];
    let store = Store::open(
        &host.database_path,
        &host.world_id,
        host.visibility,
        host.retention_seconds,
        &grants,
    )
    .expect("store");
    store
        .abandon_write("agent:abc123", "civ:local", &message("message:1", "one"))
        .expect("rollback");
    assert_eq!(store.event_count().expect("count"), 0);

    drop(store);
    let path = host.database_path.clone();
    let conn = rusqlite::Connection::open(&path).expect("db");
    conn.execute_batch("BEGIN IMMEDIATE").expect("begin");
    conn.execute(
        "INSERT INTO events (sequence, id, actor, event_json, message_json) VALUES (1, 'event:x', 'agent:abc123', '{}', '{}')",
        [],
    )
    .expect("insert");
    drop(conn);
    let store = Store::open(&path, "civ:local", Visibility::Members, 60, &grants).expect("reopen");
    assert_eq!(store.event_count().expect("count"), 0);
}

#[test]
fn retry_conflict_expiry_visibility_and_pages() {
    let dir = tempdir().expect("temp");
    let host = config(dir.path(), Visibility::Addressed);
    let grants = vec![
        ("agent:abc123".to_owned(), true, true),
        ("agent:reader".to_owned(), true, false),
        ("agent:other".to_owned(), true, false),
    ];
    let store = Store::open(
        &host.database_path,
        &host.world_id,
        Visibility::Addressed,
        10,
        &grants,
    )
    .expect("store");
    let bytes = message("message:1", "one");
    let first = store
        .submit_at("agent:abc123", "civ:local", &bytes, 1_000)
        .expect("record");
    let retry = store
        .submit_at("agent:abc123", "civ:local", &bytes, 1_005)
        .expect("retry");
    assert_eq!(first, retry);
    let mut changed = bytes.clone();
    changed.push(b' ');
    assert!(
        store
            .submit_at("agent:abc123", "civ:local", &changed, 1_005)
            .is_err()
    );
    let again = message("message:1", "later");
    // Retention is 10 seconds, so 1010 is the first instant the id may be reused.
    let second = store
        .submit_at("agent:abc123", "civ:local", &again, 1_010)
        .expect("reused id");
    assert_ne!(first["event_id"], second["event_id"]);
    assert_eq!(store.event_count().expect("count"), 2);

    let sender = store
        .read_page("agent:abc123", "civ:local", None, true)
        .expect("sender");
    let reader = store
        .read_page("agent:reader", "civ:local", None, true)
        .expect("reader");
    let other = store
        .read_page("agent:other", "civ:local", None, true)
        .expect("other");
    assert_eq!(sender["events"].as_array().expect("events").len(), 2);
    assert_eq!(reader["events"].as_array().expect("events").len(), 2);
    assert!(other["events"].as_array().expect("events").is_empty());
    assert!(
        sender["events"][0]["body"]["message"]["note"]["keep"]
            .as_bool()
            .expect("field")
    );
    assert_ne!(sender["next_cursor"].as_str().expect("cursor"), "2");

    for index in 0..101 {
        let body = message(&format!("message:{index}"), "page");
        store
            .submit_at("agent:abc123", "civ:local", &body, 2_000 + i64::from(index))
            .expect("page record");
    }
    let page = store
        .read_page("agent:abc123", "civ:local", None, true)
        .expect("page");
    assert_eq!(page["events"].as_array().expect("events").len(), 100);
    assert!(page["has_more"].as_bool().expect("more"));
    let next = store
        .read_page(
            "agent:abc123",
            "civ:local",
            page["next_cursor"].as_str(),
            true,
        )
        .expect("next");
    assert!(!next["events"].as_array().expect("events").is_empty());
    assert!(matches!(
        store.read_page("agent:reader", "civ:local", None, false),
        Err(ReadError::Forbidden)
    ));
}

#[test]
fn same_message_id_is_scoped_to_each_principal() {
    let dir = tempdir().expect("temp");
    let grants = vec![
        ("agent:abc123".to_owned(), true, true),
        ("agent:two".to_owned(), true, true),
    ];
    let store = Store::open(
        &dir.path().join("world.sqlite"),
        "civ:local",
        Visibility::Members,
        60,
        &grants,
    )
    .expect("store");
    store
        .submit("agent:abc123", "civ:local", &message("message:same", "one"))
        .expect("first principal");
    store
        .submit("agent:two", "civ:local", &message("message:same", "two"))
        .expect("second principal");
    assert_eq!(store.event_count().expect("count"), 2);
    assert!(matches!(
        store.read_page("agent:abc123", "civ:local", Some(""), true),
        Err(ReadError::InvalidCursor)
    ));
}

#[test]
fn concurrent_submissions_keep_one_receipt_per_exact_submission() {
    let dir = tempdir().expect("temp");
    let path = dir.path().join("world.sqlite");
    let grants = [("agent:abc123".to_owned(), true, true)];
    let left = Store::open(&path, "civ:local", Visibility::Members, 60, &grants).expect("left");
    let right = Store::open(&path, "civ:local", Visibility::Members, 60, &grants).expect("right");

    let exact = message("message:exact", "same");
    let (left_exact, right_exact) = std::thread::scope(|scope| {
        let left_job = scope.spawn(|| left.submit("agent:abc123", "civ:local", &exact));
        let right_job = scope.spawn(|| right.submit("agent:abc123", "civ:local", &exact));
        (
            left_job.join().expect("left"),
            right_job.join().expect("right"),
        )
    });
    assert_eq!(
        left_exact.expect("recorded"),
        right_exact.expect("recorded")
    );

    let alpha = message("message:alpha", "a");
    let beta = message("message:beta", "b");
    let (alpha_receipt, beta_receipt) = std::thread::scope(|scope| {
        let alpha_job = scope.spawn(|| left.submit("agent:abc123", "civ:local", &alpha));
        let beta_job = scope.spawn(|| right.submit("agent:abc123", "civ:local", &beta));
        (
            alpha_job.join().expect("alpha"),
            beta_job.join().expect("beta"),
        )
    });
    assert_ne!(
        alpha_receipt.expect("alpha")["event_id"],
        beta_receipt.expect("beta")["event_id"]
    );

    let first_bytes = message("message:race", "one");
    let second_bytes = message("message:race", "two");
    let (first, second) = std::thread::scope(|scope| {
        let first_job = scope.spawn(|| left.submit("agent:abc123", "civ:local", &first_bytes));
        let second_job = scope.spawn(|| right.submit("agent:abc123", "civ:local", &second_bytes));
        (
            first_job.join().expect("first"),
            second_job.join().expect("second"),
        )
    });
    let outcomes = [first, second];
    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|result| matches!(result, Err(SubmitError::Conflict)))
            .count(),
        1
    );
    assert_eq!(left.event_count().expect("count"), 4);
}

#[test]
fn sender_only_hides_a_message_from_another_principal() {
    let dir = tempdir().expect("temp");
    let grants = [
        ("agent:abc123".to_owned(), true, true),
        ("agent:reader".to_owned(), true, false),
    ];
    let store = Store::open(
        &dir.path().join("world.sqlite"),
        "civ:local",
        Visibility::SenderOnly,
        60,
        &grants,
    )
    .expect("store");
    store
        .submit(
            "agent:abc123",
            "civ:local",
            &message("message:private", "mine"),
        )
        .expect("record");
    let sender = store
        .read_page("agent:abc123", "civ:local", None, true)
        .expect("sender");
    let reader = store
        .read_page("agent:reader", "civ:local", None, true)
        .expect("reader");
    assert_eq!(sender["events"].as_array().expect("events").len(), 1);
    assert!(reader["events"].as_array().expect("events").is_empty());
    let cursor = sender["next_cursor"].as_str().expect("cursor");
    assert!(matches!(
        store.read_page("agent:reader", "civ:local", Some(cursor), true),
        Err(ReadError::Forbidden)
    ));
}

#[test]
fn a_database_keeps_the_world_it_was_opened_for() {
    let dir = tempdir().expect("temp");
    let path = dir.path().join("world.sqlite");
    let grants = [("agent:abc123".to_owned(), true, true)];
    let store = Store::open(&path, "civ:local", Visibility::Members, 60, &grants).expect("open");
    store
        .submit(
            "agent:abc123",
            "civ:local",
            &message("message:kept", "stay"),
        )
        .expect("record");
    assert_eq!(store.event_count().expect("count"), 1);
    drop(store);

    let mismatch = Store::open(&path, "civ:other", Visibility::Members, 60, &grants);
    assert!(matches!(mismatch, Err(StoreError::WorldMismatch)));

    let reopened =
        Store::open(&path, "civ:local", Visibility::Members, 60, &grants).expect("same world");
    assert_eq!(reopened.event_count().expect("count"), 1);
    let page = reopened
        .read_page("agent:abc123", "civ:local", None, true)
        .expect("read");
    let events = page["events"].as_array().expect("events");
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0]["body"]["message"]["id"].as_str(),
        Some("message:kept")
    );
}

#[tokio::test]
async fn reopening_for_another_world_fails_before_listen() {
    let dir = tempdir().expect("temp");
    let host_config = config(dir.path(), Visibility::Members);
    let grants = vec![("agent:abc123".to_owned(), true, true)];
    let store = Store::open(
        &host_config.database_path,
        &host_config.world_id,
        host_config.visibility,
        host_config.retention_seconds,
        &grants,
    )
    .expect("open");
    drop(store);
    let mut other = host_config.clone();
    other.world_id = "civ:other".to_owned();
    match start_test_host(other).await {
        Err(HostError::Config(detail)) => {
            assert_eq!(detail, "database belongs to a different world");
        }
        Ok(host) => {
            drop(host);
            panic!("host listened for a foreign world");
        }
        Err(error) => panic!("unexpected host error: {error}"),
    }
}

#[tokio::test]
async fn public_http_covers_recording_refusal_and_restart() {
    let dir = tempdir().expect("temp");
    let mut host_config = config(dir.path(), Visibility::Members);
    host_config.credentials.push(Credential {
        principal: "agent:peer".to_owned(),
        token: "token-peer".to_owned(),
        read: true,
        write: true,
    });
    let running = start_test_host(host_config.clone()).await.expect("start");
    let discovery = running.discovery.clone();
    let base = discovery
        .trim_end_matches("/.well-known/agentciv")
        .to_owned();
    let report = tokio::task::spawn_blocking({
        let discovery = discovery.clone();
        move || {
            run_extended_with_peer(
                &discovery,
                "agent:abc123",
                "token-writer",
                "agent:reader",
                "token-reader",
                Some(("agent:peer", "token-peer")),
            )
        }
    })
    .await
    .expect("runner");
    assert_eq!(report.cases.len(), 61, "{report:?}");
    let hidden_cases = [
        "collaborate.hidden_visibility",
        "collaborate.hidden_derivation",
        "collaborate.hidden_objection",
        "collaborate.hidden_decline",
        "collaborate.hidden_withdrawal",
    ];
    assert!(report.passed(), "{report:?}");
    for id in hidden_cases {
        assert!(
            report.cases.iter().any(|case| {
                case.id == id && case.status == CaseStatus::Skipped && !case.required
            }),
            "{id} missing from {report:?}"
        );
    }
    assert!(
        report
            .cases
            .iter()
            .all(|case| if hidden_cases.contains(&case.id) {
                case.status == CaseStatus::Skipped && !case.required
            } else {
                case.status == CaseStatus::Passed && case.required
            }),
        "{report:?}"
    );
    for id in [
        "submit.concurrent_retry",
        "submit.concurrent_conflict",
        "submit.concurrent_distinct",
        "submit.json_charset",
        "events.empty_cursor",
        "collaborate.client_revision",
        "collaborate.forbidden",
        "collaborate.unauthenticated",
        "collaborate.payload_too_large",
        "collaborate.unsupported_media_type",
        "collaborate.malformed_json",
        "collaborate.unsupported_version",
        "collaborate.unsupported_record_type",
        "collaborate.partial_note",
        "collaborate.wrong_world",
        "collaborate.from_mismatch",
        "collaborate.revision",
        "collaborate.retry",
        "collaborate.json_charset",
        "collaborate.conflict",
        "collaborate.objection",
        "collaborate.decline",
        "collaborate.absent_revision",
        "collaborate.withdrawal_forbidden",
        "collaborate.withdrawal",
        "collaborate.withdrawn_citation",
        "collaborate.unknown_target",
        "collaborate.other_chain",
        "collaborate.revision_sequence",
        "collaborate.revision_sequence_retry",
        "collaborate.revision_sequence_rejection",
        "collaborate.concurrent_revisions",
        "collaborate.concurrent_revisions_retry",
        "collaborate.concurrent_revisions_isolation",
    ] {
        assert!(
            report.cases.iter().any(|case| case.id == id),
            "{id} missing from {report:?}"
        );
    }
    let unauthenticated = tokio::task::spawn_blocking({
        let discovery = discovery.clone();
        move || run(&discovery)
    })
    .await
    .expect("runner");
    assert!(
        unauthenticated
            .cases
            .iter()
            .all(|case| case.status == CaseStatus::Passed)
    );

    let cursor = tokio::task::spawn_blocking(move || exercise_refusals(&base))
        .await
        .expect("http");
    drop(running);

    let running = start_test_host(host_config.clone()).await.expect("restart");
    let base = running
        .discovery
        .trim_end_matches("/.well-known/agentciv")
        .to_owned();
    let survived = tokio::task::spawn_blocking(move || reader_sees_roundtrip(&base))
        .await
        .expect("history");
    assert!(survived);
    drop(running);

    let mut narrowed = host_config;
    narrowed.visibility = Visibility::SenderOnly;
    let running = start_test_host(narrowed).await.expect("narrow");
    let base = running
        .discovery
        .trim_end_matches("/.well-known/agentciv")
        .to_owned();
    let expired = tokio::task::spawn_blocking(move || cursor_expired(&base, &cursor))
        .await
        .expect("expired");
    assert!(expired);
    drop(running);
}

fn exercise_refusals(base: &str) -> String {
    let http = client();
    let denied = http
        .post(format!("{base}/submit"))
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, "Bearer token-reader")
        .body(message("message:denied", "no"))
        .send()
        .expect("denied");
    assert_eq!(denied.status(), 403);
    assert_eq!(
        denied
            .headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(body_json(denied)["code"], "forbidden");

    let version = http
        .post(format!("{base}/submit"))
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, "Bearer token-writer")
        .body(
            serde_json::to_vec(&json!({"protocol_version": "9", "type": "message"})).expect("body"),
        )
        .send()
        .expect("version");
    assert_eq!(version.status(), 422);
    assert_eq!(body_json(version)["code"], "unsupported_version");

    let malformed = http
        .post(format!("{base}/submit"))
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, "Bearer token-writer")
        .body("{")
        .send()
        .expect("malformed");
    assert_eq!(malformed.status(), 400);
    assert_eq!(body_json(malformed)["code"], "malformed_json");

    let mut wrong_world =
        serde_json::from_slice::<serde_json::Value>(&message("message:world", "elsewhere"))
            .expect("json");
    wrong_world["world"] = json!("civ:other");
    let wrong = http
        .post(format!("{base}/submit"))
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, "Bearer token-writer")
        .body(serde_json::to_vec(&wrong_world).expect("body"))
        .send()
        .expect("world");
    assert_eq!(wrong.status(), 422);
    assert_eq!(body_json(wrong)["code"], "wrong_world");

    let mut other_sender =
        serde_json::from_slice::<serde_json::Value>(&message("message:from", "no")).expect("json");
    other_sender["from"] = json!("agent:reader");
    let mismatched = http
        .post(format!("{base}/submit"))
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, "Bearer token-writer")
        .body(serde_json::to_vec(&other_sender).expect("body"))
        .send()
        .expect("from");
    assert_eq!(mismatched.status(), 403);

    let kind = http
        .post(format!("{base}/submit"))
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, "Bearer token-writer")
        .body(
            serde_json::to_vec(&json!({"protocol_version": "0.1-draft", "type": "action"}))
                .expect("body"),
        )
        .send()
        .expect("type");
    assert_eq!(kind.status(), 422);
    assert_eq!(body_json(kind)["code"], "unsupported_record_type");

    let text = http
        .post(format!("{base}/submit"))
        .header(CONTENT_TYPE, "text/plain")
        .header(AUTHORIZATION, "Bearer token-writer")
        .body("{}")
        .send()
        .expect("media");
    assert_eq!(text.status(), 415);

    let unknown = http
        .get(format!("{base}/events?after=not-a-cursor"))
        .header(AUTHORIZATION, "Bearer token-writer")
        .send()
        .expect("cursor");
    assert_eq!(unknown.status(), 400);
    assert_eq!(body_json(unknown)["code"], "invalid_cursor");

    let large = http
        .post(format!("{base}/submit"))
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, "Bearer token-writer")
        .body("x".repeat(1100))
        .send()
        .expect("large");
    assert_eq!(large.status(), 413);

    let page = body_json(
        http.get(format!("{base}/events"))
            .header(AUTHORIZATION, "Bearer token-writer")
            .send()
            .expect("events"),
    );
    let cursor = page["next_cursor"].as_str().expect("cursor").to_owned();
    let foreign = http
        .get(format!("{base}/events?after={cursor}"))
        .header(AUTHORIZATION, "Bearer token-reader")
        .send()
        .expect("foreign cursor");
    assert_eq!(foreign.status(), 403);
    assert_eq!(body_json(foreign)["code"], "forbidden");
    cursor
}

fn reader_sees_roundtrip(base: &str) -> bool {
    let history = body_json(
        client()
            .get(format!("{base}/events"))
            .header(AUTHORIZATION, "Bearer token-reader")
            .send()
            .expect("history"),
    );
    history["events"]
        .as_array()
        .expect("events")
        .iter()
        .any(|event| event["body"]["message"]["id"] == "message:conformance-roundtrip")
}

#[tokio::test]
async fn collaboration_records_survive_restart() {
    let dir = tempdir().expect("temp");
    let mut host_config = config(dir.path(), Visibility::Addressed);
    host_config.credentials.push(Credential {
        principal: "agent:peer".to_owned(),
        token: "token-peer".to_owned(),
        read: true,
        write: true,
    });
    host_config.credentials.push(Credential {
        principal: "agent:outsider".to_owned(),
        token: "token-outsider".to_owned(),
        read: true,
        write: true,
    });
    host_config.max_payload_bytes = 4096;
    let running = start_test_host(host_config.clone()).await.expect("start");
    let base = running
        .discovery
        .trim_end_matches("/.well-known/agentciv")
        .to_owned();
    let advertised = tokio::task::spawn_blocking({
        let discovery = running.discovery.clone();
        move || body_json(client().get(discovery).send().expect("discovery"))
    })
    .await
    .expect("discovery");
    assert!(
        advertised["capabilities"]
            .as_array()
            .expect("capabilities")
            .iter()
            .any(|item| item == "collaboration.submit")
    );
    assert_eq!(
        advertised["endpoints"]["collaborate"],
        format!("{base}/collaborate")
    );
    tokio::task::spawn_blocking(move || exercise_collaboration(&base))
        .await
        .expect("collaboration");
    drop(running);

    let running = start_test_host(host_config).await.expect("restart");
    let base = running
        .discovery
        .trim_end_matches("/.well-known/agentciv")
        .to_owned();
    let survived = tokio::task::spawn_blocking(move || collaboration_survived(&base))
        .await
        .expect("restart read");
    assert!(survived);
}

fn collaborate(base: &str, token: &str, body: serde_json::Value) -> reqwest::blocking::Response {
    client()
        .post(format!("{base}/collaborate"))
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(serde_json::to_vec(&body).expect("body"))
        .send()
        .expect("collaborate")
}

fn artifact(id: &str, sender: &str, recipients: &[&str]) -> serde_json::Value {
    json!({
        "protocol_version": "0.1-draft",
        "type": "artifact_revision",
        "id": id,
        "artifact_id": "artifact:plan",
        "world": "civ:local",
        "from": sender,
        "to": recipients,
        "media_type": "application/json",
        "body": {"text": "Keep the source pages addressable."},
        "continuity_note": {
            "aim": "Leave a plan a later participant can resume or reject.",
            "resume_hint": "Read the objection before choosing a design."
        },
        "note": {"keep": true}
    })
}

fn exercise_collaboration(base: &str) {
    let denied = client()
        .post(format!("{base}/collaborate"))
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, "Bearer token-reader")
        .body(
            serde_json::to_vec(&artifact(
                "submission:denied",
                "agent:reader",
                &["agent:peer"],
            ))
            .expect("body"),
        )
        .send()
        .expect("denied");
    assert_eq!(denied.status(), 403);

    let first = artifact("submission:plan", "agent:abc123", &["agent:peer"]);
    let created = collaborate(base, "token-writer", first.clone());
    assert_eq!(created.status(), 200);
    assert_eq!(
        created
            .headers()
            .get("cache-control")
            .and_then(|value| value.to_str().ok()),
        Some("no-store")
    );
    let receipt = body_json(created);
    assert_eq!(receipt["status"], "recorded");
    assert_eq!(receipt["artifact_id"], "artifact:plan");
    assert_eq!(receipt["revision"], 1);
    assert!(receipt.get("aim").is_none());
    assert!(receipt.get("continuity_note").is_none());
    let event_id = receipt["event_id"].clone();

    let retried = client()
        .post(format!("{base}/collaborate"))
        .header(CONTENT_TYPE, "application/json; charset=utf-8")
        .header(AUTHORIZATION, "Bearer token-writer")
        .body(serde_json::to_vec(&first).expect("body"))
        .send()
        .expect("retry");
    assert_eq!(retried.status(), 200);
    assert_eq!(body_json(retried)["event_id"], event_id);

    let mut cites_missing = first.clone();
    cites_missing["derived_from"] = json!({
        "from": "agent:abc123",
        "artifact_id": "artifact:plan",
        "revision": 9
    });
    let missing_retry = collaborate(base, "token-writer", cites_missing);
    assert_eq!(missing_retry.status(), 422);
    assert_eq!(body_json(missing_retry)["code"], "unknown_target");

    let mut changed = first.clone();
    changed["body"] = json!({"text": "different bytes"});
    let conflict = collaborate(base, "token-writer", changed);
    assert_eq!(conflict.status(), 409);
    assert_eq!(body_json(conflict)["code"], "id_conflict");

    let mut chosen = first.clone();
    chosen["revision"] = json!(7);
    chosen["id"] = json!("submission:chosen-revision");
    let rejected = collaborate(base, "token-writer", chosen);
    assert_eq!(rejected.status(), 422);
    assert_eq!(body_json(rejected)["code"], "invalid_record");

    let mut second = artifact("submission:plan-2", "agent:abc123", &["agent:peer"]);
    second["body"] = json!({"text": "A second revision."});
    second
        .as_object_mut()
        .expect("object")
        .remove("continuity_note");
    let again = collaborate(base, "token-writer", second);
    assert_eq!(again.status(), 200);
    assert_eq!(body_json(again)["revision"], 2);

    let own = collaborate(
        base,
        "token-outsider",
        artifact(
            "submission:other-chain",
            "agent:outsider",
            &["agent:outsider"],
        ),
    );
    assert_eq!(own.status(), 200);
    let own_receipt = body_json(own);
    assert_eq!(own_receipt["revision"], 1);
    assert_ne!(own_receipt["event_id"], event_id);

    let hidden = collaborate(
        base,
        "token-outsider",
        json!({
            "protocol_version": "0.1-draft",
            "type": "artifact_revision",
            "id": "submission:hidden-cite",
            "artifact_id": "artifact:fork",
            "world": "civ:local",
            "from": "agent:outsider",
            "to": ["agent:outsider"],
            "media_type": "text/plain",
            "body": {"text": "cite"},
            "derived_from": {"from": "agent:abc123", "artifact_id": "artifact:plan", "revision": 1}
        }),
    );
    assert_eq!(hidden.status(), 422);
    assert_eq!(body_json(hidden)["code"], "unknown_target");

    let hidden_withdrawal = collaborate(
        base,
        "token-outsider",
        json!({
            "protocol_version": "0.1-draft",
            "type": "withdrawal",
            "id": "submission:hidden-withdraw",
            "world": "civ:local",
            "from": "agent:outsider",
            "artifact_id": "artifact:plan",
            "target_from": "agent:abc123",
            "revision": 1
        }),
    );
    assert_eq!(hidden_withdrawal.status(), 422);
    assert_eq!(body_json(hidden_withdrawal)["code"], "unknown_target");

    let missing = collaborate(
        base,
        "token-peer",
        json!({
            "protocol_version": "0.1-draft",
            "type": "objection",
            "id": "submission:missing",
            "world": "civ:local",
            "from": "agent:peer",
            "to": ["agent:abc123"],
            "artifact_id": "artifact:plan",
            "target_from": "agent:abc123",
            "revision": 9,
            "body": {"text": "No such revision."}
        }),
    );
    assert_eq!(missing.status(), 422);
    assert_eq!(body_json(missing)["code"], "unknown_target");

    let objection = collaborate(
        base,
        "token-peer",
        json!({
            "protocol_version": "0.1-draft",
            "type": "objection",
            "id": "submission:objection",
            "world": "civ:local",
            "from": "agent:peer",
            "to": ["agent:abc123"],
            "artifact_id": "artifact:plan",
            "target_from": "agent:abc123",
            "revision": 1,
            "body": {"text": "The plan still treats a summary as the source."}
        }),
    );
    assert_eq!(objection.status(), 200);
    let decline = collaborate(
        base,
        "token-peer",
        json!({
            "protocol_version": "0.1-draft",
            "type": "decline",
            "id": "submission:decline",
            "world": "civ:local",
            "from": "agent:peer",
            "to": ["agent:abc123"],
            "artifact_id": "artifact:plan",
            "target_from": "agent:abc123",
            "revision": 1,
            "body": {"text": "I will not take up this plan."}
        }),
    );
    assert_eq!(decline.status(), 200);

    let before = body_json(
        client()
            .get(format!("{base}/events"))
            .header(AUTHORIZATION, "Bearer token-peer")
            .send()
            .expect("before"),
    );
    let saw_note = before["events"]
        .as_array()
        .expect("events")
        .iter()
        .any(|event| {
            event["kind"] == "artifact.recorded"
                && event["body"]["artifact_revision"]["revision"] == 1
                && event["body"]["artifact_revision"]["continuity_note"]["aim"]
                    == "Leave a plan a later participant can resume or reject."
                && event["body"]["artifact_revision"]["note"]["keep"] == true
        });
    assert!(saw_note);

    let stolen = collaborate(
        base,
        "token-peer",
        json!({
            "protocol_version": "0.1-draft",
            "type": "withdrawal",
            "id": "submission:steal",
            "world": "civ:local",
            "from": "agent:peer",
            "artifact_id": "artifact:plan",
            "target_from": "agent:abc123",
            "revision": 1
        }),
    );
    assert_eq!(stolen.status(), 403);
    assert_eq!(body_json(stolen)["code"], "forbidden");

    let withdrawn = collaborate(
        base,
        "token-writer",
        json!({
            "protocol_version": "0.1-draft",
            "type": "withdrawal",
            "id": "submission:withdraw",
            "world": "civ:local",
            "from": "agent:abc123",
            "artifact_id": "artifact:plan",
            "target_from": "agent:abc123",
            "revision": 1
        }),
    );
    assert_eq!(withdrawn.status(), 200);
    assert_eq!(body_json(withdrawn)["event_id"], event_id);

    let cited_tombstone = collaborate(
        base,
        "token-peer",
        json!({
            "protocol_version": "0.1-draft",
            "type": "objection",
            "id": "submission:after-withdrawal",
            "world": "civ:local",
            "from": "agent:peer",
            "to": ["agent:abc123"],
            "artifact_id": "artifact:plan",
            "target_from": "agent:abc123",
            "revision": 1,
            "body": {"text": "The withdrawal leaves the objection standing."}
        }),
    );
    assert_eq!(cited_tombstone.status(), 200);

    let on_submit = client()
        .post(format!("{base}/submit"))
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, "Bearer token-writer")
        .body(
            serde_json::to_vec(&artifact(
                "submission:wrong-door",
                "agent:abc123",
                &["agent:peer"],
            ))
            .expect("body"),
        )
        .send()
        .expect("submit");
    assert_eq!(on_submit.status(), 422);
    assert_eq!(body_json(on_submit)["code"], "unsupported_record_type");

    let message_on_collaborate = collaborate(
        base,
        "token-writer",
        json!({
            "protocol_version": "0.1-draft",
            "type": "message",
            "id": "message:wrong-door",
            "world": "civ:local",
            "from": "agent:abc123",
            "to": ["agent:peer"],
            "body": {"text": "This door is not message submit."}
        }),
    );
    assert_eq!(message_on_collaborate.status(), 422);
    assert_eq!(
        body_json(message_on_collaborate)["code"],
        "unsupported_record_type"
    );

    assert!(collaboration_survived(base));
}

fn collaboration_survived(base: &str) -> bool {
    let history = body_json(
        client()
            .get(format!("{base}/events"))
            .header(AUTHORIZATION, "Bearer token-peer")
            .send()
            .expect("history"),
    );
    let events = history["events"].as_array().expect("events");
    let tombstone = events.iter().any(|event| {
        event["kind"] == "artifact.withdrawn"
            && event["body"].as_object().expect("body").is_empty()
            && event["actor"] == "agent:abc123"
    });
    let objection = events.iter().any(|event| {
        event["kind"] == "objection.recorded"
            && event["body"]["objection"]["body"]["text"]
                == "The plan still treats a summary as the source."
    });
    let decline = events
        .iter()
        .any(|event| event["kind"] == "decline.recorded");
    let after = events.iter().any(|event| {
        event["kind"] == "objection.recorded"
            && event["body"]["objection"]["body"]["text"]
                == "The withdrawal leaves the objection standing."
    });
    let kept = events.iter().any(|event| {
        event["kind"] == "artifact.recorded"
            && event["body"]["artifact_revision"]["revision"] == 2
            && event["actor"] == "agent:abc123"
    });
    tombstone && objection && decline && kept && after
}

fn cursor_expired(base: &str, cursor: &str) -> bool {
    let expired = client()
        .get(format!("{base}/events?after={cursor}"))
        .header(AUTHORIZATION, "Bearer token-writer")
        .send()
        .expect("expired");
    expired.status() == 410 && body_json(expired)["code"] == "cursor_expired"
}
