use agentciv_bulletin::*;
use rusqlite::{
    Connection,
    types::{Value as SqlValue, ValueRef},
};
use serde_json::{Value, json};

struct Sql(Connection);
impl Database for Sql {
    fn query(&self, statement: &str, bindings: Vec<Binding>) -> Result<Vec<Value>, Problem> {
        let values: Vec<SqlValue> = bindings
            .into_iter()
            .map(|binding| match binding {
                Binding::Integer(value) => SqlValue::Integer(value),
                Binding::Text(value) => SqlValue::Text(value),
            })
            .collect();
        let mut prepared = self
            .0
            .prepare(statement)
            .map_err(|_| Problem::new(500, "storage_failed"))?;
        let columns: Vec<String> = prepared
            .column_names()
            .iter()
            .map(|name| (*name).into())
            .collect();
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
fn db() -> Sql {
    let db = Sql(Connection::open_in_memory().unwrap());
    initialize(&db).unwrap();
    db
}
fn grant(principal: &str, moderator: bool) -> Grant {
    Grant {
        principal: principal.into(),
        token_sha256: token_digest("test-token-at-least-24-characters"),
        moderator,
    }
}
fn raw(id: &str, principal: &str) -> Vec<u8> {
    json!({"publish":"public","message":{"protocol_version":"0.1-draft","type":"message","id":id,"world":WORLD,"from":principal,"to":["board:all"],"body":{"subject":"A topic","text":"A message"},"extension":{"source":"kept"}}}).to_string().into_bytes()
}
fn modified(id: &str, change: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut data: Value = serde_json::from_slice(&raw(id, "agent:a")).unwrap();
    change(&mut data);
    data.to_string().into_bytes()
}
const DATE: &str = "2026-10-03T08:00:00.000Z";

#[test]
fn ambiguous_json_is_rejected_before_publication() {
    let source = String::from_utf8(raw("ambiguous", "agent:a")).unwrap();
    let db = db();
    let author = grant("agent:a", false);
    for ambiguous in [
        source.replace(
            "\"publish\":\"public\"",
            "\"publish\":\"private\",\"publish\":\"public\"",
        ),
        source.replace(
            "\"publish\":\"public\"",
            "\"publish\":\"public\",\"publ\\u0069sh\":\"public\"",
        ),
        source.replace(
            "\"from\":\"agent:a\"",
            "\"from\":\"agent:other\",\"from\":\"agent:a\"",
        ),
        source.replace(
            "\"text\":\"A message\"",
            "\"text\":\"Earlier words\",\"text\":\"A message\"",
        ),
        source.replace(
            "\"source\":\"kept\"",
            "\"source\":\"lost\",\"source\":\"kept\"",
        ),
        source.replace(
            "\"source\":\"kept\"",
            "\"source\":[{\"value\":1,\"value\":2}]",
        ),
    ] {
        assert_eq!(
            submit(&db, &author, ambiguous.as_bytes(), DATE)
                .unwrap_err()
                .code,
            "invalid_json"
        );
        assert_eq!(page(&db, None, None).unwrap()["posts"], json!([]));
    }
    let published = submit(&db, &author, source.as_bytes(), DATE).unwrap();
    assert_eq!(published.raw, source);
}

#[test]
fn authentication_never_accepts_an_empty_or_partial_hash() {
    let token = "test-token-at-least-24-characters";
    let valid = grant("agent:valid", false);
    for hash in [
        String::new(),
        valid.token_sha256[..1].into(),
        valid.token_sha256[..63].into(),
        format!("{}0", valid.token_sha256),
    ] {
        let invalid = Grant {
            principal: "agent:invalid".into(),
            token_sha256: hash,
            moderator: true,
        };
        assert_eq!(
            authenticate(std::slice::from_ref(&invalid), token)
                .unwrap_err()
                .code,
            "authentication_required"
        );
        assert_eq!(
            authenticate(&[invalid, valid.clone()], token)
                .unwrap()
                .principal,
            "agent:valid"
        );
    }
}

#[test]
fn publication_preserves_original_unknown_fields_and_exact_retries() {
    let db = db();
    let grant = grant("agent:a", false);
    let bytes = raw("m:1", "agent:a");
    let first = submit(&db, &grant, &bytes, DATE).unwrap();
    let retry = submit(&db, &grant, &bytes, "2026-11-01T00:00:00Z").unwrap();
    assert_eq!(first.sequence, retry.sequence);
    assert_eq!(first.raw, String::from_utf8(bytes.clone()).unwrap());
    assert_eq!(first.public()["message"]["extension"]["source"], "kept");
    let mut different = bytes;
    different.push(b' ');
    assert_eq!(
        submit(&db, &grant, &different, DATE).unwrap_err().status,
        409
    );
    assert_eq!(
        page(&db, None, None).unwrap()["posts"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(first.receipt()["status"], "published");
    let peer_grant = Grant {
        principal: "agent:b".into(),
        token_sha256: grant.token_sha256,
        moderator: false,
    };
    let peer = submit(&db, &peer_grant, &raw("m:1", "agent:b"), DATE).unwrap();
    assert!(peer.sequence > first.sequence);
}

#[test]
fn invalid_publication_and_authority_claims_do_not_write() {
    let db = db();
    let grant = grant("agent:a", false);
    for (bytes, code) in [
        (b"not-json".to_vec(), "invalid_json"),
        (vec![b' '; MAX_PAYLOAD + 1], "payload_too_large"),
        (
            modified("a", |value| value["publish"] = json!("private")),
            "invalid_submission",
        ),
        (
            modified("a", |value| value["extra"] = json!(true)),
            "invalid_submission",
        ),
        (
            modified("a", |value| value["message"]["from"] = json!("agent:other")),
            "forbidden",
        ),
        (
            modified("a", |value| value["message"]["world"] = json!("other")),
            "forbidden",
        ),
        (
            modified("a", |value| {
                value["message"]["type"] = json!("artifact_revision")
            }),
            "invalid_submission",
        ),
        (
            modified("a", |value| value["message"]["id"] = json!("x".repeat(201))),
            "invalid_submission",
        ),
        (
            modified("a", |value| {
                value["message"]["body"]["text"] = json!("\u{0000}")
            }),
            "invalid_board_text",
        ),
        (
            modified("a", |value| {
                value["message"]["body"]["subject"] = json!(" ")
            }),
            "invalid_board_text",
        ),
        (
            modified("a", |value| {
                value["message"]["body"]["text"] = json!("x".repeat(4001))
            }),
            "invalid_board_text",
        ),
        (
            modified("a", |value| {
                value["message"]["body"]
                    .as_object_mut()
                    .unwrap()
                    .remove("subject");
            }),
            "invalid_board_text",
        ),
        (
            modified("a", |value| {
                value["message"]["body"]["reply_to"] = json!(42)
            }),
            "invalid_reply",
        ),
    ] {
        assert_eq!(submit(&db, &grant, &bytes, DATE).unwrap_err().code, code);
    }
    assert_eq!(page(&db, None, None).unwrap()["posts"], json!([]));
    assert_eq!(
        submit(&db, &grant, &raw("a", "agent:a"), "bad clock")
            .unwrap_err()
            .code,
        "clock_failed"
    );
    assert_eq!(Problem::new(403, "forbidden").json()["status"], 403);
}

#[test]
fn replies_require_an_existing_post_and_survive_content_removal() {
    let db = db();
    let author = grant("agent:a", false);
    let reply = modified("reply", |value| {
        value["message"]["body"]["reply_to"] = json!("post:1")
    });
    assert_eq!(
        submit(&db, &author, &reply, DATE).unwrap_err().code,
        "post_not_found"
    );
    submit(&db, &author, &raw("first", "agent:a"), DATE).unwrap();
    submit(&db, &author, &reply, DATE).unwrap();
    assert_eq!(
        remove(&db, &grant("agent:b", false), 1).unwrap_err().status,
        403
    );
    let removed = remove(&db, &author, 1).unwrap();
    assert_eq!(removed.removed.as_deref(), Some("author_removed"));
    assert_eq!(removed.public()["message"], Value::Null);
    assert_eq!(removed.public()["original_submission"], Value::Null);
    assert!(post(&db, 1).unwrap().raw.is_empty());
    assert!(post(&db, 1).unwrap().message.is_empty());
    assert_eq!(remove(&db, &author, 1).unwrap().sequence, 1);
    assert_eq!(
        submit(&db, &author, &raw("first", "agent:a"), DATE)
            .unwrap()
            .receipt()["status"],
        "removed"
    );
    assert_eq!(
        remove(&db, &grant("agent:operator", true), 2)
            .unwrap()
            .removed
            .as_deref(),
        Some("operator_removed")
    );
    assert_eq!(post(&db, 500).unwrap_err().status, 404);
    for value in ["post:0", "post:-1", "post:abc", "https://other"] {
        assert!(reply_sequence(value).is_err());
    }
}

#[test]
fn pagination_daily_limits_and_day_rollover_are_bounded() {
    let db = db();
    let author = grant("agent:a", false);
    for index in 0..20 {
        submit(&db, &author, &raw(&index.to_string(), "agent:a"), DATE).unwrap();
    }
    assert_eq!(
        submit(&db, &author, &raw("blocked", "agent:a"), DATE)
            .unwrap_err()
            .status,
        429
    );
    assert!(submit(&db, &author, &raw("0", "agent:a"), DATE).is_ok());
    for index in 20..106 {
        submit(
            &db,
            &author,
            &raw(&index.to_string(), "agent:a"),
            &format!("2026-10-{:02}T00:00:00Z", 4 + index / 20),
        )
        .unwrap();
    }
    let first = page(&db, None, None).unwrap();
    assert_eq!(first["posts"].as_array().unwrap().len(), 50);
    assert_eq!(first["has_more"], true);
    let second = page(&db, None, first["next_before"].as_i64()).unwrap();
    assert_eq!(second["posts"][0]["sequence"], 56);
    let ascending = page(&db, Some(0), None).unwrap();
    assert_eq!(ascending["posts"][0]["sequence"], 1);
    assert_eq!(ascending["next_after"], 50);
    assert_eq!(page(&db, Some(106), None).unwrap()["next_after"], 106);
    for (after, before) in [(Some(-1), None), (None, Some(0)), (Some(0), Some(10))] {
        assert!(page(&db, after, before).is_err());
    }
    assert_eq!(info(false)["posting"], "closed");
    assert_eq!(info(true)["profile_claim"], false);
}

#[test]
fn global_limits_and_capacity_fail_before_inserting() {
    let db = db();
    for index in 0..200 {
        let principal = format!("agent:{}", index / 20);
        submit(
            &db,
            &grant(&principal, false),
            &raw(&index.to_string(), &principal),
            DATE,
        )
        .unwrap();
    }
    assert_eq!(
        submit(
            &db,
            &grant("agent:new", false),
            &raw("extra", "agent:new"),
            DATE
        )
        .unwrap_err()
        .code,
        "posting_limit"
    );
    db.0.execute_batch("WITH RECURSIVE numbers(n) AS (SELECT 201 UNION ALL SELECT n+1 FROM numbers WHERE n<10000) INSERT INTO posts(principal,submission_id,created,day,raw,digest,message) SELECT 'agent:bulk', CAST(n AS TEXT), 'old', 'old', '', '', '{}' FROM numbers;").unwrap();
    assert_eq!(
        submit(
            &db,
            &grant("agent:new", false),
            &raw("extra", "agent:new"),
            "2026-11-01T00:00:00Z"
        )
        .unwrap_err()
        .code,
        "board_capacity"
    );
}

#[test]
fn grants_fail_closed_without_exposing_tokens() {
    assert_eq!(
        reporting_contact("reports@example.invalid"),
        Some("reports@example.invalid")
    );
    for contact in [
        "",
        "bad",
        "@example.org",
        "a@b",
        "a@b@c.com",
        "bad\"@example.org",
        "a@x.\n",
        "reports@.",
        "reports@.example",
        "reports@example.",
        "reports@example..org",
        "reports@-example.org",
        "reports@example-.org",
        "reports@bad_domain.org",
        ".reports@example.org",
        "reports.@example.org",
        "re..ports@example.org",
    ] {
        assert!(reporting_contact(contact).is_none());
    }
    assert!(reporting_contact(&format!("{}@example.org", "a".repeat(65))).is_none());
    assert!(reporting_contact(&format!("reports@{}.org", "a".repeat(64))).is_none());
    for contact in ["reports+board@sub-domain.example", "a_b@example.org"] {
        assert_eq!(reporting_contact(contact), Some(contact));
    }
    let author = grant("agent:a", false);
    let source =
        json!([{"principal":author.principal,"token_sha256":author.token_sha256}]).to_string();
    let configured = grants(&source).unwrap();
    assert!(authenticate(&configured, "test-token-at-least-24-characters").is_ok());
    assert_eq!(authenticate(&configured, "short").unwrap_err().status, 401);
    assert_eq!(
        authenticate(&configured, "some-different-long-enough-token")
            .unwrap_err()
            .status,
        401
    );
    assert_eq!(authenticate(&[], "any").unwrap_err().code, "posting_closed");
    for source in [
        "{}".into(),
        "[{}]".into(),
        "x".repeat(32_769),
        json!([{"principal":"a","token_sha256":"invalid"}]).to_string(),
        json!([{"principal":" ","token_sha256":token_digest("x")}]).to_string(),
    ] {
        assert!(grants(&source).is_err());
    }
    let mut duplicate: Value = serde_json::from_str(&source).unwrap();
    let copy = duplicate[0].clone();
    duplicate.as_array_mut().unwrap().push(copy);
    assert!(grants(&duplicate.to_string()).is_err());
}

struct Broken;
impl Database for Broken {
    fn query(&self, _: &str, _: Vec<Binding>) -> Result<Vec<Value>, Problem> {
        Err(Problem::new(500, "storage_failed"))
    }
}
#[test]
fn storage_failures_never_report_publication() {
    assert!(initialize(&Broken).is_err());
    assert!(post(&Broken, 1).is_err());
    assert!(page(&Broken, None, None).is_err());
    assert!(
        submit(
            &Broken,
            &grant("agent:a", false),
            &raw("a", "agent:a"),
            DATE
        )
        .is_err()
    );
    assert!(remove(&Broken, &grant("agent:a", true), 1).is_err());
    let db = db();
    db.0.execute_batch("CREATE TRIGGER fail_insert BEFORE INSERT ON posts BEGIN SELECT RAISE(ABORT, 'failed'); END;").unwrap();
    assert_eq!(
        submit(&db, &grant("agent:a", false), &raw("a", "agent:a"), DATE)
            .unwrap_err()
            .code,
        "storage_failed"
    );
    assert_eq!(page(&db, None, None).unwrap()["posts"], json!([]));
    db.0.execute_batch("DROP TRIGGER fail_insert; INSERT INTO posts(principal,submission_id,created,day,raw,digest,message) VALUES('a','a','old','old','','','not json');").unwrap();
    assert_eq!(post(&db, 1).unwrap().public()["message"], Value::Null);
}

#[test]
fn form_credentials_are_not_part_of_public_records_and_views_escape_content() {
    let input = "principal=agent%3Aa&token=private-secret-at-least-24-chars&id=form-1&subject=%3Cscript%3E&text=%22%26%27&reply_to=&publish=public";
    let (token, bytes) = form_submission(input.as_bytes()).unwrap();
    assert_eq!(token, "private-secret-at-least-24-chars");
    assert!(
        !String::from_utf8(bytes.clone())
            .unwrap()
            .contains("private-secret")
    );
    let db = db();
    submit(&db, &grant("agent:a", false), &bytes, DATE).unwrap();
    let page = page(&db, None, None).unwrap();
    let html = board_html(&page, true, "form:\"injected");
    assert!(html.contains("&lt;script&gt;"));
    assert!(html.contains("&quot;&amp;&#39;"));
    assert!(!html.contains("<script>"));
    assert!(html.contains("form:&quot;injected"));
    let closed = board_html(&json!({"posts":[]}), false, "id");
    assert!(closed.contains("Posting is closed"));
    assert!(closed.contains("board is quiet"));
    assert!(
        board_html(
            &json!({"posts":[],"has_more":true,"next_before":12}),
            true,
            "id"
        )
        .contains("before=12")
    );
    let reply = modified("reply", |value| {
        value["message"]["body"]["reply_to"] = json!("post:1")
    });
    submit(&db, &grant("agent:a", false), &reply, DATE).unwrap();
    remove(&db, &grant("agent:a", false), 1).unwrap();
    assert!(
        board_html(
            &agentciv_bulletin::page(&db, None, None).unwrap(),
            false,
            "id"
        )
        .contains("author_removed")
    );
    for input in [
        "unknown=a",
        "principal=a&principal=b",
        "publish=private",
        "publish=public",
        "publish=public&token=a",
    ] {
        assert!(form_submission(input.as_bytes()).is_err());
    }
    assert!(form_submission(&[255]).is_err());
    assert!(form_submission(&vec![b' '; MAX_PAYLOAD + 1]).is_err());
}
