use crate::tests::{artifact, body_json, client, config, problem_code};
use crate::{Credential, Visibility, start_test_host};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{Value, json};
use tempfile::tempdir;

fn record(kind: &str, id: &str, target: &str, revision: &str) -> String {
    let mut value = json!({"protocol_version":"0.1-draft","type":kind,"id":id,"world":"civ:local",
        "from":"agent:abc123","to":["agent:reader"],"artifact_id":target,
        "body":{"extension":{"integer":18446744073709551615_u64,"boolean":false,"text":"keep"}}});
    if kind == "artifact_revision" {
        value["artifact_id"] = json!(id);
        value["media_type"] = json!("application/json");
        value["derived_from"] =
            json!({"from":"agent:abc123","artifact_id":target,"revision":"LEXEME"});
    } else {
        value["target_from"] = json!("agent:abc123");
        value["revision"] = json!("LEXEME");
    }
    value.to_string().replace("\"LEXEME\"", revision)
}

fn post(base: &str, token: &str, raw: &str) -> reqwest::blocking::Response {
    client()
        .post(format!("{base}/collaborate"))
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json")
        .body(raw.to_owned())
        .send()
        .unwrap()
}

fn history(base: &str) -> Value {
    body_json(
        client()
            .get(format!("{base}/events"))
            .header(AUTHORIZATION, "Bearer token-writer")
            .send()
            .unwrap(),
    )["events"]
        .clone()
}

#[tokio::test]
async fn exact_integral_citations_roundtrip_retry_and_keep_target_permissions() {
    let directory = tempdir().unwrap();
    let mut settings = config(directory.path(), Visibility::Members);
    settings.max_payload_bytes = 8192;
    settings.credentials.push(Credential {
        principal: "agent:other".into(),
        token: "token-other".into(),
        read: true,
        write: true,
    });
    let host = start_test_host(settings).await.unwrap();
    let base = host
        .discovery
        .trim_end_matches("/.well-known/agentciv")
        .to_owned();
    tokio::task::spawn_blocking(move || {
        for (index, spelling) in ["1.0", "1e0", "10e-1", "1.0000000000000000000000000000000"]
            .iter()
            .enumerate()
        {
            let target = format!("artifact:integer-{index}");
            let mut seed = artifact(&format!("seed:{index}"), "agent:abc123", &["agent:reader"]);
            seed["artifact_id"] = json!(target);
            let seeded = body_json(post(&base, "token-writer", &seed.to_string()));
            for kind in ["artifact_revision", "objection", "decline", "withdrawal"] {
                let id = format!("submission:{index}:{kind}");
                let raw = record(kind, &id, &target, spelling);
                let before = history(&base);
                let receipt_response = post(&base, "token-writer", &raw);
                assert_eq!(receipt_response.status(), 200, "{kind} {spelling}");
                let receipt = receipt_response.bytes().unwrap();
                let retry = post(&base, "token-writer", &raw);
                assert_eq!(retry.status(), 200);
                assert_eq!(retry.bytes().unwrap(), receipt);
                let changed = raw.replace(spelling, if *spelling == "1e0" { "1.0" } else { "1e0" });
                assert_eq!(
                    problem_code(post(&base, "token-writer", &changed)),
                    (409, "id_conflict".into())
                );
                let parsed: Value = serde_json::from_slice(&receipt).unwrap();
                let after = history(&base);
                let events = after.as_array().unwrap();
                let found = events
                    .iter()
                    .find(|event| event["id"] == parsed["event_id"])
                    .unwrap();
                if kind == "withdrawal" {
                    assert_eq!(events.len(), before.as_array().unwrap().len());
                    assert_eq!(found["id"], seeded["event_id"]);
                    assert_eq!(found["sequence"], seeded["sequence"]);
                    assert_eq!(found["kind"], "artifact.withdrawn");
                    assert_eq!(found["body"], json!({}));
                } else {
                    let mut expected: Value = serde_json::from_str(&raw).unwrap();
                    if kind == "artifact_revision" {
                        expected["revision"] = parsed["revision"].clone();
                    }
                    assert_eq!(found["body"][kind], expected);
                    assert_eq!(events.len(), before.as_array().unwrap().len() + 1);
                }
            }
            let foreign = record("withdrawal", &format!("foreign:{index}"), &target, spelling)
                .replace("\"from\":\"agent:abc123\"", "\"from\":\"agent:other\"");
            assert_eq!(
                problem_code(post(&base, "token-other", &foreign)),
                (403, "forbidden".into())
            );
        }
        for kind in ["artifact_revision", "objection", "decline", "withdrawal"] {
            let before = history(&base);
            for (index, spelling) in [
                "1.5",
                "1.00000000000000000001",
                "9007199254740991.1",
                "1e-999",
                "9007199254740992",
                "0",
                "-1",
                "true",
                "null",
                "\"1\"",
            ]
            .iter()
            .enumerate()
            {
                let raw = record(
                    kind,
                    &format!("invalid:{kind}:{index}"),
                    "artifact:integer-0",
                    spelling,
                );
                assert_eq!(
                    problem_code(post(&base, "token-writer", &raw)),
                    (422, "invalid_record".into()),
                    "{kind} {spelling}"
                );
            }
            for spelling in [
                "1.0",
                "1e0",
                "9007199254740991",
                "9007199254740991.0",
                "9.007199254740991e15",
            ] {
                let raw = record(
                    kind,
                    &format!("absent:{kind}:{spelling}"),
                    "artifact:absent",
                    spelling,
                );
                assert_eq!(
                    problem_code(post(&base, "token-writer", &raw)),
                    (422, "unknown_target".into())
                );
            }
            assert_eq!(history(&base), before);
        }
    })
    .await
    .unwrap();
}

#[test]
fn exact_citation_keys_refuse_malformed_and_overflow_without_rounding() {
    for spelling in [
        "1",
        "1.0",
        "1e0",
        "1000e-3",
        "9007199254740991.0",
        "90071992547409910e-1",
    ] {
        let raw = record("objection", "key", "target", spelling);
        let value: Value = serde_json::from_str(&raw).unwrap();
        assert!(crate::validate::exact_citation_revision(&value, raw.as_bytes()).is_some());
    }
    for spelling in [
        "0",
        "-0.0",
        "true",
        "null",
        "\"1\"",
        "1.00000000000000000001",
        "1e-999",
        "1e999",
        "1e9223372036854775807",
        "1e-9223372036854775808",
        "9007199254740992",
    ] {
        let raw = record("objection", "key", "target", spelling);
        assert!(
            crate::validate::exact_citation_revision(&json!({"type":"objection"}), raw.as_bytes())
                .is_none(),
            "{spelling}"
        );
    }
    assert!(
        crate::validate::exact_citation_revision(&json!({"type":"artifact_revision"}), b"{}")
            .is_none()
    );
    assert!(
        crate::validate::exact_citation_revision(&json!({"type":"objection"}), b"not json")
            .is_none()
    );
}

#[tokio::test]
async fn integral_spellings_do_not_reveal_hidden_citation_targets() {
    for visibility in [Visibility::Addressed, Visibility::SenderOnly] {
        let directory = tempdir().unwrap();
        let mut settings = config(directory.path(), visibility);
        settings.credentials.push(Credential {
            principal: "agent:other".into(),
            token: "token-other".into(),
            read: true,
            write: true,
        });
        let host = start_test_host(settings).await.unwrap();
        let base = host
            .discovery
            .trim_end_matches("/.well-known/agentciv")
            .to_owned();
        tokio::task::spawn_blocking(move || {
            let source = artifact("hidden-source", "agent:abc123", &["agent:reader"]);
            assert_eq!(
                post(&base, "token-writer", &source.to_string()).status(),
                200
            );
            let before = history(&base);
            for kind in ["artifact_revision", "objection", "decline", "withdrawal"] {
                for spelling in ["1.0", "1e0"] {
                    let mut foreign: Value = serde_json::from_str(&record(
                        kind,
                        &format!("hidden:{kind}:{spelling}"),
                        "artifact:plan",
                        "1.0",
                    ))
                    .unwrap();
                    foreign["from"] = json!("agent:other");
                    let raw = foreign.to_string().replace("1.0", spelling);
                    assert_eq!(
                        problem_code(post(&base, "token-other", &raw)),
                        (422, "unknown_target".into())
                    );
                }
            }
            assert_eq!(history(&base), before);
        })
        .await
        .unwrap();
    }
}
