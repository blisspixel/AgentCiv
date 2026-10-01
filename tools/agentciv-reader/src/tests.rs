use super::*;
use serde_json::json;

pub(crate) const WORLD: &str = "civ:reader-test";

pub(crate) fn event(id: &str, sequence: u64, author: &str) -> String {
    serde_json::to_string_pretty(&json!({
        "protocol_version":"0.1-draft", "type":"event", "id":id,
        "world":WORLD,"sequence":sequence,"timestamp":"2026-09-30T00:00:00Z",
        "kind":"artifact.recorded","actor":author,"body":{"artifact_revision":{
            "protocol_version":"0.1-draft","type":"artifact_revision","id":format!("submission:{id}"),
            "world":WORLD,"from":author,"to":["agent:reader"],"artifact_id":"artifact:same-name",
            "revision":1,"media_type":"application/json","body":{"untrusted":"do not execute"},
            "unknown":{"unicode":"é","authority":false}
        }}
    })).unwrap()
}

pub(crate) fn page(records: &[String], more: bool, cursor: &str) -> Vec<u8> {
    format!("{{\"protocol_version\":\"0.1-draft\",\"type\":\"event_page\",\"world\":\"{WORLD}\",\"events\":[{}],\"has_more\":{more},\"next_cursor\":{}}}",
        records.join(",\n"), serde_json::to_string(cursor).unwrap()).into_bytes()
}

fn read_pages(pages: Vec<Vec<u8>>) -> Result<ReadResult> {
    let mut pages = pages.into_iter();
    collect(
        WORLD,
        &Budgets::default(),
        Traversal::All,
        None,
        |_, _, _| Ok(pages.next().unwrap()),
    )
}

fn failed(pages: Vec<Vec<u8>>, expected: Error) {
    let error = read_pages(pages).unwrap_err();
    assert_eq!(error, expected);
    assert!(!error.code().contains("untrusted"));
}

#[test]
fn exact_originals_survive_pages_and_integral_numbers_have_numeric_identity() {
    let one = event("event:one", 0, "agent:a")
        .replace("\"revision\": 1", "\"revision\": 1e0")
        .replace("\"unicode\": \"é\"", "\"unicode\": \"\\u00e9\"");
    let two = event("event:two", 7, "agent:b").replace("\"sequence\": 7", "\"sequence\": 7.0");
    let mut calls = Vec::new();
    let mut responses = [
        page(std::slice::from_ref(&one), true, "opaque:&first"),
        page(std::slice::from_ref(&two), false, "opaque:last"),
    ]
    .into_iter();
    let result = collect(
        WORLD,
        &Budgets::default(),
        Traversal::All,
        None,
        |after, remaining, max| {
            assert!(remaining > Duration::ZERO);
            assert!(max <= Budgets::default().max_response_bytes);
            calls.push(after.map(str::to_owned));
            Ok(responses.next().unwrap())
        },
    )
    .unwrap();
    assert_eq!(calls, [None, Some("opaque:&first".into())]);
    assert_eq!(result.snapshot.records, [one, two]);
    assert_eq!(result.report.pages, 2);
    assert_eq!(result.report.events, 2);
    assert!(result.report.reached_end);
    let public = serde_json::to_string(&result.report).unwrap();
    assert!(!public.contains("opaque"));
    assert_eq!(result.report.copying_permission, "not_granted");
}

#[test]
fn first_page_fetches_once_and_reports_deliberate_partial_selection() {
    let mut calls = 0;
    let result = collect(
        WORLD,
        &Budgets::default(),
        Traversal::FirstPage,
        None,
        |after, _, _| {
            assert!(after.is_none());
            calls += 1;
            Ok(page(
                &[event("event:one", 1, "agent:a")],
                true,
                "private-cursor",
            ))
        },
    )
    .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(result.snapshot.records.len(), 1);
    assert!(!result.report.reached_end);
}

#[test]
fn empty_final_page_and_unchanged_polling_cursor_are_valid() {
    let one = event("event:one", 1, "agent:a");
    let result = read_pages(vec![
        page(std::slice::from_ref(&one), true, "same"),
        page(&[], false, "same"),
    ])
    .unwrap();
    assert_eq!(result.snapshot.records, [one]);
    assert_eq!(result.report.pages, 2);
    assert_eq!(
        read_pages(vec![page(&[], false, "empty")])
            .unwrap()
            .report
            .events,
        0
    );
}

#[test]
fn cross_page_event_order_and_revision_conflicts_fail() {
    let first = event("event:one", 5, "agent:a");
    let start = page(std::slice::from_ref(&first), true, "cursor:first");
    failed(
        vec![start.clone(), page(&[first], false, "last")],
        Error::EventConflict,
    );
    for number in [0, 4, 5] {
        failed(
            vec![
                start.clone(),
                page(&[event("event:two", number, "agent:b")], false, "last"),
            ],
            Error::SequenceConflict,
        );
    }
    failed(
        vec![
            start,
            page(&[event("event:two", 6, "agent:a")], false, "last"),
        ],
        Error::RevisionConflict,
    );
}

#[test]
fn repeated_cursor_fails_but_rotating_opaque_tokens_are_not_interpreted() {
    failed(
        vec![page(&[], true, "same"), page(&[], true, "same")],
        Error::CursorConflict,
    );
    let result = read_pages(vec![
        page(&[], true, "not-a-number"),
        page(&[], true, "totally-different"),
        page(&[], false, "done"),
    ])
    .unwrap();
    assert_eq!(result.report.pages, 3);
}

#[test]
fn nested_record_authorship_world_and_tombstone_content_are_checked() {
    let base = event("event:one", 1, "agent:a");
    for (field, value) in [("actor", json!("agent:impostor")), ("body", json!({}))] {
        let mut altered: Value = serde_json::from_str(&base).unwrap();
        altered[field] = value;
        failed(
            vec![page(&[altered.to_string()], false, "last")],
            Error::InvalidEvent,
        );
    }
    let mut altered: Value = serde_json::from_str(&base).unwrap();
    altered["body"]["artifact_revision"]["world"] = json!("civ:other");
    failed(
        vec![page(&[altered.to_string()], false, "last")],
        Error::InvalidEvent,
    );
    altered["kind"] = json!("artifact.withdrawn");
    failed(
        vec![page(&[altered.to_string()], false, "last")],
        Error::InvalidEvent,
    );
    altered["body"] = json!({});
    assert!(read_pages(vec![page(&[altered.to_string()], false, "last")]).is_ok());
}

#[test]
fn pages_follow_written_schema_and_world_including_empty_history() {
    let valid = page(&[], false, "last");
    for (field, value) in [
        ("world", json!("civ:other")),
        ("type", json!("artifact")),
        ("has_more", json!(0)),
        ("next_cursor", json!("")),
        ("protocol_version", json!("future")),
    ] {
        let mut invalid: Value = serde_json::from_slice(&valid).unwrap();
        invalid[field] = value;
        failed(vec![invalid.to_string().into_bytes()], Error::InvalidPage);
    }
    let mut event: Value = serde_json::from_str(&event("event:one", 1, "agent:a")).unwrap();
    event["world"] = json!("civ:other");
    failed(
        vec![page(&[event.to_string()], false, "last")],
        Error::InvalidEvent,
    );
}

#[test]
fn duplicate_members_nonfinite_json_bad_utf8_and_deep_json_fail() {
    for payload in [
        b"{\"world\":1,\"\\u0077orld\":2}".to_vec(),
        b"{\"n\":NaN}".to_vec(),
        b"{\"nested\":{\"x\":1,\"x\":2}}".to_vec(),
        format!("{{\"deep\":{}0{}}}", "[".repeat(200), "]".repeat(200)).into_bytes(),
    ] {
        failed(vec![payload], Error::InvalidJson);
    }
    failed(vec![vec![0xff]], Error::InvalidUtf8);
}

#[test]
fn rounded_fractional_or_underflowed_identity_numbers_fail_without_rewriting_freeform() {
    let original = event("event:one", 1, "agent:a");
    for spelling in ["9007199254740991.1", "1.00000000000000000001", "1e-999"] {
        for field in ["sequence", "revision"] {
            let invalid = original.replace(
                &format!("\"{field}\": 1"),
                &format!("\"{field}\": {spelling}"),
            );
            assert!(read_pages(vec![page(&[invalid], false, "last")]).is_err());
        }
    }
    let untouched = original.replace(
        "\"authority\": false",
        "\"authority\": false, \"freeform\": 9007199254740991.1, \"tiny\": 1e-999",
    );
    assert_eq!(
        read_pages(vec![page(std::slice::from_ref(&untouched), false, "last")])
            .unwrap()
            .snapshot
            .records,
        [untouched]
    );
}

#[test]
fn unknown_records_preserve_data_without_inventing_authority_or_execution() {
    let mut unknown: Value = serde_json::from_str(&event("event:one", 0, "agent:a")).unwrap();
    unknown["kind"] = json!("unfamiliar.recorded");
    unknown["body"] = json!({"execute":"file:///outside", "authority":"operator", "unknown":[1,2]});
    unknown["actor"] = Value::Null;
    let original = unknown.to_string();
    assert_eq!(
        read_pages(vec![page(std::slice::from_ref(&original), false, "last")])
            .unwrap()
            .snapshot
            .records,
        [original]
    );
}

#[test]
fn decoded_keys_and_values_cannot_reflect_secret_with_unicode_escapes() {
    for payload in [
        "{\"s\\u0065cret-token\":null}",
        "{\"unknown\":\"s\\u0065cret-token\"}",
    ] {
        let error = collect(
            WORLD,
            &Budgets::default(),
            Traversal::All,
            Some("secret-token"),
            |_, _, _| Ok(payload.as_bytes().to_vec()),
        )
        .unwrap_err();
        assert_eq!(error, Error::CredentialReflection);
        assert_eq!(error.code(), "credential_reflected");
    }
}

#[test]
fn every_budget_stops_without_an_extra_request_or_partial_result() {
    let first = page(&[event("event:one", 1, "agent:a")], true, "first");
    let second = page(&[event("event:two", 2, "agent:b")], false, "last");
    for (budgets, expected) in [
        (
            Budgets {
                max_pages: 1,
                ..Budgets::default()
            },
            Error::PageLimit,
        ),
        (
            Budgets {
                max_events: 1,
                ..Budgets::default()
            },
            Error::EventLimit,
        ),
        (
            Budgets {
                max_response_bytes: 1,
                ..Budgets::default()
            },
            Error::ResponseLimit,
        ),
        (
            Budgets {
                max_record_bytes: 1,
                ..Budgets::default()
            },
            Error::RecordLimit,
        ),
        (
            Budgets {
                max_total_bytes: first.len(),
                ..Budgets::default()
            },
            Error::TotalLimit,
        ),
        (
            Budgets {
                max_total_bytes: first.len() - 1,
                ..Budgets::default()
            },
            Error::TotalLimit,
        ),
    ] {
        let mut pages = [first.clone(), second.clone()].into_iter();
        let mut calls = 0;
        let error = collect(WORLD, &budgets, Traversal::All, None, |_, _, _| {
            calls += 1;
            Ok(pages.next().unwrap())
        })
        .unwrap_err();
        assert_eq!(error, expected);
        assert!(!error.code().is_empty());
        assert!(calls <= 2);
        if matches!(expected, Error::PageLimit | Error::TotalLimit) {
            assert_eq!(calls, 1);
        }
    }
}

#[test]
fn invalid_budget_and_world_fail_before_callback() {
    for budgets in [
        Budgets {
            max_pages: 0,
            ..Budgets::default()
        },
        Budgets {
            max_events: 257,
            ..Budgets::default()
        },
        Budgets {
            max_response_bytes: MAX_INPUT_BYTES + 1,
            ..Budgets::default()
        },
        Budgets {
            max_record_bytes: MAX_RECORD_BYTES + 1,
            ..Budgets::default()
        },
        Budgets {
            max_total_bytes: 0,
            ..Budgets::default()
        },
        Budgets {
            seconds: 151,
            ..Budgets::default()
        },
    ] {
        assert_eq!(
            collect(WORLD, &budgets, Traversal::All, None, |_, _, _| panic!(
                "must not fetch"
            ))
            .unwrap_err(),
            Error::Configuration
        );
    }
    assert_eq!(
        collect(
            "",
            &Budgets::default(),
            Traversal::All,
            None,
            |_, _, _| panic!("must not fetch")
        )
        .unwrap_err(),
        Error::Configuration
    );
}

#[test]
fn deadlines_and_typed_transport_failure_never_retry() {
    let error = collect_until(
        WORLD,
        &Budgets::default(),
        Traversal::All,
        None,
        Instant::now() - Duration::from_secs(1),
        0,
        |_, _, _| panic!("expired"),
    )
    .unwrap_err();
    assert_eq!(error, Error::Deadline);
    let error = collect_until(
        WORLD,
        &Budgets::default(),
        Traversal::All,
        None,
        Instant::now() + Duration::from_millis(5),
        0,
        |_, _, _| {
            std::thread::sleep(Duration::from_millis(15));
            Ok(page(&[], false, "last"))
        },
    )
    .unwrap_err();
    assert_eq!(error, Error::Deadline);
    assert_eq!(error.code(), "deadline_exceeded");
    for expected in [
        Error::Transport,
        Error::Authentication,
        Error::Forbidden,
        Error::CursorExpired,
    ] {
        let mut calls = 0;
        let error = collect(
            WORLD,
            &Budgets::default(),
            Traversal::All,
            None,
            |_, _, _| {
                calls += 1;
                Err(expected)
            },
        )
        .unwrap_err();
        assert_eq!(error, expected);
        assert_eq!(calls, 1);
        assert!(!error.code().is_empty());
    }
}
