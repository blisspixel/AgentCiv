use agentciv_archive::{export, inspect, validate_event};
use serde_json::{Value, json};

fn stored(sequence: &str, revision: &str, citation: Option<&str>) -> String {
    let source = citation.map_or(String::new(), |revision| format!(",\"derived_from\":{{\"from\":\"agent:prior\",\"artifact_id\":\"artifact:prior\",\"revision\":{revision}}}"));
    format!(
        "{{\"protocol_version\":\"0.1-draft\",\"type\":\"event\",\"id\":\"event:one\",\"world\":\"civ:precision\",\"sequence\":{sequence},\"timestamp\":\"2026-09-30T00:00:00Z\",\"kind\":\"artifact.recorded\",\"actor\":\"agent:a\",\"body\":{{\"artifact_revision\":{{\"protocol_version\":\"0.1-draft\",\"type\":\"artifact_revision\",\"id\":\"record:one\",\"world\":\"civ:precision\",\"from\":\"agent:a\",\"to\":[\"agent:reader\"],\"artifact_id\":\"artifact:one\",\"revision\":{revision},\"media_type\":\"application/json\",\"body\":{{\"unknown_fraction\":9007199254740991.1,\"unknown_small\":1e-999}}{source}}}}}}}"
    )
}

fn bundle(record: &str) -> String {
    let snapshot = json!({"world":"civ:precision","records":[record]}).to_string();
    let permit =
        json!({"world":"civ:precision","audience":["agent:reader"],"event_ids":["event:one"]})
            .to_string();
    serde_json::to_string(&export(&snapshot, &permit).unwrap()).unwrap()
}

#[test]
fn rounded_fractional_or_underflowed_controls_are_not_schema_integers() {
    for raw in [
        "9007199254740991.1",
        "0.9999999999999999999999",
        "1.00000000000000000001",
        "1e-999",
    ] {
        assert!(
            validate_event(&stored(raw, "1", None), "civ:precision").is_err(),
            "sequence {raw}"
        );
        assert!(
            validate_event(&stored("1", raw, None), "civ:precision").is_err(),
            "revision {raw}"
        );
        assert!(
            validate_event(&stored("1", "1", Some(raw)), "civ:precision").is_err(),
            "citation {raw}"
        );
        let snapshot =
            json!({"world":"civ:precision","records":[stored(raw,"1",None)]}).to_string();
        let permit =
            json!({"world":"civ:precision","audience":["agent:reader"],"event_ids":["event:one"]})
                .to_string();
        assert!(export(&snapshot, &permit).is_err());
    }
}

#[test]
fn legal_integer_spellings_preserve_originals_and_unknown_freeform_numbers() {
    for raw in [
        "1.0",
        "1e0",
        "1000e-3",
        "1.0000000000000000000000000000000000000000000000000000000000000",
    ] {
        let record = stored(raw, raw, Some(raw));
        let copied: Value = serde_json::from_str(&bundle(&record)).unwrap();
        assert_eq!(copied["entries"][0]["record_utf8"], record);
        assert!(inspect(&copied.to_string()).is_ok());
    }
    for raw in ["-0.0", "0e0", "0.000"] {
        assert!(validate_event(&stored(raw, "1", None), "civ:precision").is_ok());
    }
}

#[test]
fn objection_and_decline_target_revisions_use_exact_lexemes_too() {
    for kind in ["objection", "decline"] {
        for revision in ["9007199254740991.1", "1.00000000000000000001"] {
            let record = format!(
                "{{\"protocol_version\":\"0.1-draft\",\"type\":\"event\",\"id\":\"event:one\",\"world\":\"civ:precision\",\"sequence\":0,\"timestamp\":\"2026-09-30T00:00:00Z\",\"kind\":\"{kind}.recorded\",\"actor\":\"agent:a\",\"body\":{{\"{kind}\":{{\"protocol_version\":\"0.1-draft\",\"type\":\"{kind}\",\"id\":\"record:one\",\"world\":\"civ:precision\",\"from\":\"agent:a\",\"to\":[\"agent:reader\"],\"artifact_id\":\"artifact:one\",\"target_from\":\"agent:prior\",\"revision\":{revision},\"body\":{{}}}}}}}}"
            );
            assert_eq!(
                validate_event(&record, "civ:precision").unwrap_err(),
                "invalid_nested_record"
            );
        }
    }
}

#[test]
fn legal_safe_boundary_controls_have_exact_identity_despite_float_parser_rounding() {
    for raw in [
        "9007199254740991",
        "9007199254740991.0",
        "9.007199254740991e15",
        "90071992547409910e-1",
    ] {
        let record = stored(raw, raw, None);
        let view = inspect(&bundle(&record)).unwrap();
        assert_eq!(
            view["events"][0]["sequence"].as_f64().unwrap() as u64,
            9_007_199_254_740_991,
            "{raw}"
        );
        assert_eq!(
            view["artifacts"][0]["revision"].as_f64().unwrap() as u64,
            9_007_199_254_740_991,
            "{raw}"
        );
    }
}
