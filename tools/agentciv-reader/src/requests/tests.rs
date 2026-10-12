use super::*;
use serde_json::json;

const FIXTURES: &[&str] = &[
    include_str!("../../../../conformance/fixtures/valid/message.json"),
    include_str!("../../../../conformance/fixtures/valid/collaboration-artifact.json"),
    include_str!("../../../../conformance/fixtures/valid/collaboration-objection.json"),
    include_str!("../../../../conformance/fixtures/valid/collaboration-decline.json"),
    include_str!("../../../../conformance/fixtures/valid/collaboration-withdrawal.json"),
];

fn fixture(index: usize) -> Value {
    serde_json::from_str(FIXTURES[index]).unwrap()
}

fn replaced(mut request: Value, path: &[&str], raw: &str) -> String {
    let mut value = &mut request;
    for field in path {
        value = &mut value[*field];
    }
    *value = json!("RAW-VALUE-MARKER");
    request.to_string().replace("\"RAW-VALUE-MARKER\"", raw)
}

#[test]
fn all_five_shared_incoming_fixtures_and_distinct_withdrawal_shape_pass() {
    for raw in FIXTURES {
        assert_eq!(check(raw), Ok(()));
    }
    for raw in [
        include_str!(
            "../../../../conformance/fixtures/valid/collaboration-artifact-note-omitted.json"
        ),
        include_str!(
            "../../../../conformance/fixtures/valid/collaboration-artifact-note-max-characters.json"
        ),
    ] {
        assert_eq!(check(raw), Ok(()));
    }
    let mut withdrawal = fixture(4);
    withdrawal["body"] = json!({});
    // Withdrawal has no recipient constraint; a claimed target still confers no grant.
    withdrawal["to"] = json!(false);
    withdrawal["target_from"] = json!("agent:someone-else");
    assert_eq!(check(&withdrawal.to_string()), Ok(()));
    withdrawal["body"] = Value::Null;
    assert_eq!(check(&withdrawal.to_string()), Err(Error::InvalidRequest));
}

#[test]
fn every_required_field_is_checked_by_its_own_shared_schema() {
    for (index, source) in SCHEMAS.iter().enumerate() {
        let schema: Value = serde_json::from_str(source).unwrap();
        for field in schema["required"].as_array().unwrap() {
            let field = field.as_str().unwrap();
            let mut request = fixture(index);
            request.as_object_mut().unwrap().remove(field);
            assert_eq!(
                check(&request.to_string()),
                Err(Error::InvalidRequest),
                "missing {index} {field}"
            );
            for wrong in [Value::Null, json!(false), json!([])] {
                let mut request = fixture(index);
                request[field] = wrong;
                assert_eq!(
                    check(&request.to_string()),
                    Err(Error::InvalidRequest),
                    "wrong type {index} {field}"
                );
            }
        }
        for (field, rule) in schema["properties"].as_object().unwrap() {
            if rule["minLength"] == 1 {
                let mut request = fixture(index);
                request[field] = json!("");
                assert_eq!(
                    check(&request.to_string()),
                    Err(Error::InvalidRequest),
                    "empty {index} {field}"
                );
            }
        }
        for (field, wrong) in [("protocol_version", "9.9"), ("type", "unknown")] {
            let mut request = fixture(index);
            request[field] = json!(wrong);
            assert_eq!(check(&request.to_string()), Err(Error::InvalidRequest));
        }
        if index != 4 {
            for recipients in [
                json!([]),
                json!([""]),
                json!([false]),
                json!(["same", "same"]),
                json!({}),
            ] {
                let mut request = fixture(index);
                request["to"] = recipients;
                assert_eq!(check(&request.to_string()), Err(Error::InvalidRequest));
            }
        }
        for body in [Value::Null, json!([]), json!("text"), json!(false)] {
            let mut request = fixture(index);
            request["body"] = body;
            assert_eq!(check(&request.to_string()), Err(Error::InvalidRequest));
        }
    }
    for raw in ["null", "[]", "true", "1", "\"source\"", "{}"] {
        assert_eq!(check(raw), Err(Error::InvalidRequest));
    }
}

#[test]
fn artifact_assignment_and_optional_objects_keep_their_incoming_rules() {
    for revision in [json!(1), json!(1.0), Value::Null, json!(false), json!("1")] {
        let mut request = fixture(1);
        request["revision"] = revision;
        assert_eq!(check(&request.to_string()), Err(Error::InvalidRequest));
    }
    for field in ["derived_from", "continuity_note"] {
        for object in [Value::Null, json!([]), json!(true), json!({})] {
            let mut request = fixture(1);
            request[field] = object;
            assert_eq!(check(&request.to_string()), Err(Error::InvalidRequest));
        }
        let required: &[&str] = if field == "derived_from" {
            &["from", "artifact_id", "revision"]
        } else {
            &["aim", "resume_hint"]
        };
        for nested in required {
            let mut request = fixture(1);
            request[field].as_object_mut().unwrap().remove(*nested);
            assert_eq!(check(&request.to_string()), Err(Error::InvalidRequest));
            for wrong in [Value::Null, json!(false), json!(""), json!([])] {
                let mut request = fixture(1);
                request[field][nested] = wrong;
                assert_eq!(check(&request.to_string()), Err(Error::InvalidRequest));
            }
        }
    }
    for field in ["aim", "resume_hint"] {
        for (length, expected) in [(1024, Ok(())), (1025, Err(Error::InvalidRequest))] {
            let mut request = fixture(1);
            request["continuity_note"][field] = json!("界".repeat(length));
            assert_eq!(check(&request.to_string()), expected);
        }
    }
}

#[test]
fn exact_revision_lexemes_are_checked_at_every_known_path() {
    for index in 1..5 {
        let path: &[&str] = if index == 1 {
            &["derived_from", "revision"]
        } else {
            &["revision"]
        };
        for raw in [
            "1",
            "1.0",
            "1e0",
            "10e-1",
            "1000e-3",
            "1E+0",
            "1.0000000000000000000000000000000000000000000000000000",
            "9007199254740991",
            "9007199254740991.0",
            "9.007199254740991e15",
            "90071992547409910e-1",
        ] {
            assert_eq!(
                check(&replaced(fixture(index), path, raw)),
                Ok(()),
                "{index} {raw}"
            );
        }
        for raw in [
            "true",
            "null",
            "\"1\"",
            "0",
            "-0.0",
            "-1",
            "0.5",
            "1.5",
            "1e-999",
            "9007199254740992",
            "9007199254740991.1",
            "1.00000000000000000001",
            "0.9999999999999999999999",
        ] {
            assert_eq!(
                check(&replaced(fixture(index), path, raw)),
                Err(Error::InvalidRequest),
                "{index} {raw}"
            );
        }
    }
}

#[test]
fn unreserved_numbers_and_authority_like_fields_remain_data() {
    for index in 0..5 {
        let mut request = fixture(index);
        request["unknown"] = json!({"authority":"all", "$ref":"https://127.0.0.1:1/private"});
        request["$schema"] = json!("http://127.0.0.1:1/not-a-schema");
        request["body"] = json!({"private":"\u{1b}[31m", "unknown":true});
        for raw in ["1.00000000000000000001", "9007199254740991.1", "1e-999"] {
            assert_eq!(
                check(&replaced(request.clone(), &["unknown", "number"], raw)),
                Ok(())
            );
            assert_eq!(
                check(&replaced(request.clone(), &["body", "number"], raw)),
                Ok(())
            );
        }
    }
    let mut artifact = fixture(1);
    artifact["derived_from"]["unreserved"] = json!([true, 2, "citation"]);
    artifact["continuity_note"]["unreserved"] = json!({"authority":true});
    assert_eq!(check(&artifact.to_string()), Ok(()));
    let mut message = fixture(0);
    message["revision"] = json!("not a reserved message control");
    message["derived_from"] = json!(false);
    message["continuity_note"] = Value::Null;
    assert_eq!(check(&message.to_string()), Ok(()));
}

#[test]
fn decoded_duplicates_fail_at_every_depth_including_unknown_arrays() {
    let mut cases = Vec::new();
    let mut envelope = fixture(0).to_string();
    envelope.pop();
    cases.push(format!("{envelope},\"\\u0069d\":\"duplicate-id\"}}"));
    for raw in [
        "{\"same\":1,\"same\":2}",
        "{\"same\":1,\"\\u0073ame\":2}",
        "{\"unknown\":[{\"same\":1,\"same\":2}]}",
    ] {
        cases.push(replaced(fixture(0), &["body"], raw));
        cases.push(replaced(fixture(0), &["unknown"], raw));
        cases.push(replaced(fixture(1), &["derived_from"], raw));
        cases.push(replaced(fixture(1), &["continuity_note"], raw));
    }
    for raw in cases {
        assert_eq!(check(&raw), Err(Error::InvalidJson));
    }
    let raw = replaced(fixture(0), &["to"], "[\"agent:a\",\"agent:\\u0061\"]");
    assert_eq!(check(&raw), Err(Error::InvalidRequest));
}

#[test]
fn local_input_and_parser_limits_fail_without_changing_stored_event_bounds() {
    let mut request = fixture(0);
    request["body"] = json!({"text":""});
    let base = request.to_string().len();
    for size in [16_001, MAX_REQUEST_BYTES, MAX_REQUEST_BYTES + 1] {
        let padding = size - base;
        request["body"]["text"] = json!(format!(
            "{}{}",
            "é".repeat(padding / 2),
            "x".repeat(padding % 2)
        ));
        let raw = request.to_string();
        assert_eq!(raw.len(), size);
        assert_eq!(
            check(&raw),
            if size <= MAX_REQUEST_BYTES {
                Ok(())
            } else {
                Err(Error::InputLimit)
            }
        );
    }
    assert_eq!(agentciv_archive::MAX_RECORD_BYTES, 16_000);
    for raw in [
        "",
        "{",
        "{}{}",
        "{} trailing",
        "{\"bad\":NaN}",
        "{\"bad\":Infinity}",
        "{\"bad\":01}",
        "{\"bad\":\"\\uD800\"}",
        "{\"bad\":1e999999}",
    ] {
        assert_eq!(check(raw), Err(Error::InvalidJson));
    }
    let deep = format!("{}0{}", "[".repeat(140), "]".repeat(140));
    assert_eq!(
        check(&replaced(fixture(0), &["body", "deep"], &deep)),
        Err(Error::InvalidJson)
    );
}

#[test]
fn trusted_schema_compilation_is_explicitly_offline() {
    assert!(compile(&json!({"$ref":"http://127.0.0.1:1/unregistered-schema"})).is_err());
    assert_eq!(validators().len(), 5);
}
