use super::*;

fn message(author: &str, id: &str) -> Vec<u8> {
    json!({"protocol_version":"0.1-draft","type":"message","world":"civ:prefix","from":author,"id":id,"to":[author],"body":{"text":"Exact source"}}).to_string().into_bytes()
}

#[test]
fn prefix_checks_legacy_access_hidden_changes_and_replaced_originals() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(
        &directory.path().join("world.sqlite"),
        "civ:prefix",
        Visibility::SenderOnly,
        60,
        &[
            ("agent:a".into(), true, true),
            ("agent:b".into(), true, true),
        ],
    )
    .unwrap();
    let base = store
        .read_page("agent:a", "civ:prefix", None, true)
        .unwrap();
    assert!(matches!(
        store.read_prefix_page("agent:a", "civ:prefix", base["next_cursor"].as_str(), true),
        Err(ReadError::PrefixUnavailable)
    ));
    let first = store
        .read_prefix_page("agent:a", "civ:prefix", None, true)
        .unwrap();
    let cursor = first["next_cursor"].as_str();
    assert!(matches!(
        store.read_prefix_page("agent:b", "civ:prefix", cursor, true),
        Err(ReadError::ForeignCursor)
    ));
    assert!(matches!(
        store.read_prefix_page("agent:a", "civ:prefix", Some("unknown"), false),
        Err(ReadError::Forbidden)
    ));
    assert!(matches!(
        store.read_prefix_page("agent:a", "civ:prefix", Some("unknown"), true),
        Err(ReadError::InvalidCursor)
    ));
    store
        .submit(
            "agent:b",
            "civ:prefix",
            &message("agent:b", "message:hidden"),
        )
        .unwrap();
    assert_eq!(
        first,
        store
            .read_prefix_page("agent:a", "civ:prefix", cursor, true)
            .unwrap()
    );
    store
        .submit(
            "agent:a",
            "civ:prefix",
            &message("agent:a", "message:visible"),
        )
        .unwrap();
    let next = store
        .read_prefix_page("agent:a", "civ:prefix", cursor, true)
        .unwrap();
    assert_eq!(next["validated_prefix_sha256"], first["prefix_sha256"]);
    assert_eq!(next["events"].as_array().unwrap().len(), 1);
    assert_ne!(next["prefix_sha256"], first["prefix_sha256"]);
    let connection = store.conn.lock().unwrap();
    connection.execute("UPDATE events SET event_json=json_set(event_json, '$.kind', 'event.redacted', '$.body', json('{}')) WHERE actor='agent:b'", []).unwrap();
    drop(connection);
    assert!(
        store
            .read_prefix_page("agent:a", "civ:prefix", next["next_cursor"].as_str(), true)
            .is_ok()
    );
    let connection = store.conn.lock().unwrap();
    connection.execute("UPDATE events SET event_json=json_set(event_json, '$.kind', 'event.redacted', '$.body', json('{}')) WHERE actor='agent:a'", []).unwrap();
    drop(connection);
    assert!(matches!(
        store.read_prefix_page("agent:a", "civ:prefix", next["next_cursor"].as_str(), true),
        Err(ReadError::PrefixChanged)
    ));
}

#[test]
fn cursor_generation_changes_only_when_this_callers_read_policy_changes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("world.sqlite");
    let grants = [("agent:a".into(), true, true)];
    let store = Store::open(&path, "civ:prefix", Visibility::Members, 60, &grants).unwrap();
    let first = store
        .read_prefix_page("agent:a", "civ:prefix", None, true)
        .unwrap();
    drop(store);
    let added = [
        ("agent:a".into(), true, false),
        ("agent:b".into(), true, true),
    ];
    let store = Store::open(&path, "civ:prefix", Visibility::Members, 60, &added).unwrap();
    assert_eq!(
        first,
        store
            .read_prefix_page("agent:a", "civ:prefix", first["next_cursor"].as_str(), true)
            .unwrap()
    );
    drop(store);
    let revoked = [
        ("agent:a".into(), false, true),
        ("agent:b".into(), true, true),
    ];
    let store = Store::open(&path, "civ:prefix", Visibility::Members, 60, &revoked).unwrap();
    assert!(matches!(
        store.read_prefix_page(
            "agent:a",
            "civ:prefix",
            first["next_cursor"].as_str(),
            false
        ),
        Err(ReadError::Forbidden)
    ));
    drop(store);
    let store = Store::open(&path, "civ:prefix", Visibility::Members, 60, &grants).unwrap();
    assert!(matches!(
        store.read_prefix_page("agent:a", "civ:prefix", first["next_cursor"].as_str(), true),
        Err(ReadError::Expired)
    ));
}

#[test]
fn oversized_prefix_fails_without_disabling_base_reads_or_issuing_partial_proof() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(
        &directory.path().join("world.sqlite"),
        "civ:prefix",
        Visibility::Members,
        60,
        &[],
    )
    .unwrap();
    store
        .submit(
            "agent:a",
            "civ:prefix",
            &message("agent:a", "message:template"),
        )
        .unwrap();
    let connection = store.conn.lock().unwrap();
    connection.execute("WITH RECURSIVE n(x) AS (SELECT 2 UNION ALL SELECT x+1 FROM n WHERE x < 4097) INSERT INTO events SELECT x, 'event:extra-' || x, actor, json_set(event_json, '$.sequence', x, '$.id', 'event:extra-' || x), message_json FROM n, events WHERE sequence=1", []).unwrap();
    assert!(matches!(
        prefix_digest(&connection, "agent:a", Visibility::Members, 4097),
        Err(ReadError::PrefixUnavailable)
    ));
    connection
        .execute("DELETE FROM events WHERE sequence > 1", [])
        .unwrap();
    let large = "x".repeat(MAX_PREFIX_BYTES);
    connection
        .execute(
            "UPDATE events SET event_json=json_set(event_json, '$.extension', ?1)",
            params![large],
        )
        .unwrap();
    assert!(matches!(
        prefix_digest(&connection, "agent:a", Visibility::Members, 1),
        Err(ReadError::PrefixUnavailable)
    ));
    drop(connection);
    assert!(store.read_page("agent:a", "civ:prefix", None, true).is_ok());
    assert!(matches!(
        store.read_prefix_page("agent:a", "civ:prefix", None, true),
        Err(ReadError::PrefixUnavailable)
    ));
}
