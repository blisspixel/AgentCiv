use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use reqwest::blocking::Client;
use serde_json::json;

struct HostProcess(Child);

impl Drop for HostProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn process_restart_keeps_a_permitted_event() {
    let dir = tempfile::tempdir().expect("temp");
    let config_path = dir.path().join("host.json");
    let database = dir.path().join("world.sqlite");
    std::fs::write(
        &config_path,
        serde_json::to_vec_pretty(&json!({
            "world_id": "civ:local",
            "title": "Local",
            "database_path": database,
            "listen": "127.0.0.1:0",
            "visibility": "members",
            "retention_seconds": 3600,
            "max_payload_bytes": 4096,
            "credentials": [
                {"principal": "agent:abc123", "token": "token-writer", "read": true, "write": true},
                {"principal": "agent:reader", "token": "token-reader", "read": true, "write": false}
            ]
        }))
        .expect("config"),
    )
    .expect("write");

    let binary = env!("CARGO_BIN_EXE_agentciv-host");
    let (discovery, first) = spawn(binary, &config_path);
    let base = discovery.trim_end_matches("/.well-known/agentciv");
    let http = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("client");
    let recorded = http
        .post(format!("{base}/submit"))
        .header("content-type", "application/json")
        .header("authorization", "Bearer token-writer")
        .body(
            serde_json::to_vec(&json!({
                "protocol_version": "0.1-draft",
                "type": "message",
                "id": "message:restart",
                "world": "civ:local",
                "from": "agent:abc123",
                "to": ["agent:reader"],
                "body": {"text": "survive"}
            }))
            .expect("body"),
        )
        .send()
        .expect("submit");
    assert_eq!(recorded.status(), 200);
    let receipt: serde_json::Value =
        serde_json::from_str(&recorded.text().expect("receipt")).expect("json");
    drop(first);

    let (discovery, _second) = spawn(binary, &config_path);
    let base = discovery.trim_end_matches("/.well-known/agentciv");
    let page: serde_json::Value = serde_json::from_str(
        &http
            .get(format!("{base}/events"))
            .header("authorization", "Bearer token-reader")
            .send()
            .expect("read")
            .text()
            .expect("page"),
    )
    .expect("json");
    assert!(
        page["events"]
            .as_array()
            .expect("events")
            .iter()
            .any(|event| {
                event["id"] == receipt["event_id"]
                    && event["body"]["message"]["body"]["text"] == "survive"
            })
    );
}

#[test]
fn later_reader_sees_both_messages_after_restart() {
    let dir = tempfile::tempdir().expect("temp");
    let config_path = dir.path().join("host.json");
    let database = dir.path().join("world.sqlite");
    std::fs::write(
        &config_path,
        serde_json::to_vec_pretty(&json!({
            "world_id": "civ:local",
            "title": "Local",
            "database_path": database,
            "listen": "127.0.0.1:0",
            "visibility": "members",
            "retention_seconds": 3600,
            "max_payload_bytes": 4096,
            "credentials": [
                {"principal": "agent:a", "token": "token-a", "read": true, "write": true},
                {"principal": "agent:b", "token": "token-b", "read": true, "write": true},
                {"principal": "agent:c", "token": "token-c", "read": true, "write": false}
            ]
        }))
        .expect("config"),
    )
    .expect("write");
    let binary = env!("CARGO_BIN_EXE_agentciv-host");
    let (discovery, first) = spawn(binary, &config_path);
    let base = discovery.trim_end_matches("/.well-known/agentciv");
    let http = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("client");
    let first_id = submit(
        &http,
        base,
        "token-a",
        "agent:a",
        "message:task",
        "unfinished",
    );
    let second_id = submit(
        &http,
        base,
        "token-b",
        "agent:b",
        "message:revision",
        "question",
    );
    drop(first);

    let (discovery, _second) = spawn(binary, &config_path);
    let base = discovery.trim_end_matches("/.well-known/agentciv");
    let page: serde_json::Value = serde_json::from_str(
        &http
            .get(format!("{base}/events"))
            .header("authorization", "Bearer token-c")
            .send()
            .expect("read")
            .text()
            .expect("page"),
    )
    .expect("json");
    let events = page["events"].as_array().expect("events");
    assert!(events.iter().any(|event| {
        event["id"] == first_id && event["body"]["message"]["body"]["text"] == "unfinished"
    }));
    assert!(events.iter().any(|event| {
        event["id"] == second_id && event["body"]["message"]["body"]["text"] == "question"
    }));
}

fn submit(http: &Client, base: &str, token: &str, principal: &str, id: &str, text: &str) -> String {
    let response = http
        .post(format!("{base}/submit"))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(
            serde_json::to_vec(&json!({
                "protocol_version": "0.1-draft",
                "type": "message",
                "id": id,
                "world": "civ:local",
                "from": principal,
                "to": ["agent:c"],
                "body": {"text": text}
            }))
            .expect("body"),
        )
        .send()
        .expect("submit");
    assert_eq!(response.status(), 200);
    let receipt: serde_json::Value =
        serde_json::from_str(&response.text().expect("receipt")).expect("json");
    receipt["event_id"].as_str().expect("event").to_owned()
}

fn spawn(binary: &str, config: &std::path::Path) -> (String, HostProcess) {
    let mut child = Command::new(binary)
        .arg("--config")
        .arg(config)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    let stdout = child.stdout.take().expect("stdout");
    let mut line = String::new();
    BufReader::new(stdout)
        .read_line(&mut line)
        .expect("discovery");
    let discovery = line
        .strip_prefix("discovery ")
        .expect("discovery line")
        .trim()
        .to_owned();
    (discovery, HostProcess(child))
}
