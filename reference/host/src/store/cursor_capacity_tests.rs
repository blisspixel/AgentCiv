use super::*;

#[test]
fn concurrent_admission_cannot_overbook_the_last_slot() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("world.sqlite");
    let store = Store::open(&path, "civ:quota", Visibility::Members, 60, &[]).unwrap();
    let connection = store.conn.lock().unwrap();
    admit_cursor_owner(&connection, "agent:one").unwrap();
    connection
        .execute(
            "WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n WHERE x < ?1)
        INSERT INTO cursors(token, principal, last_sequence, revision) SELECT 'existing-' || x,'agent:one',x,1 FROM n",
            params![MAX_CURSORS_PER_OWNER - 1],
        )
        .unwrap();
    drop(connection);
    let outcomes = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|index| {
                let path = &path;
                scope.spawn(move || {
                    let connection = Connection::open(path).unwrap();
                    connection.busy_timeout(Duration::from_secs(5)).unwrap();
                    connection.execute_batch("BEGIN IMMEDIATE").unwrap();
                    let outcome = cursor_at(&connection, "agent:one", 1, 10000 + index);
                    connection
                        .execute_batch(if outcome.is_ok() {
                            "COMMIT"
                        } else {
                            "ROLLBACK"
                        })
                        .unwrap();
                    outcome
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, Err(ReadError::Capacity)))
            .count(),
        3
    );
}

#[test]
fn ten_thousand_distinct_boundaries_and_generations_have_finite_admission() {
    for changing_policy in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("world.sqlite");
        let store = Store::open(&path, "civ:quota", Visibility::Members, 60, &[]).unwrap();
        let connection = store.conn.lock().unwrap();
        connection.execute_batch("BEGIN IMMEDIATE").unwrap();
        let mut refused = 0;
        for index in 0..10_000 {
            let (revision, boundary) = if changing_policy {
                (index + 1, 0)
            } else {
                (1, index)
            };
            let result = cursor_at(&connection, "agent:one", revision, boundary);
            if index < MAX_CURSORS_PER_OWNER {
                result.unwrap();
            } else {
                assert!(matches!(result, Err(ReadError::Capacity)));
                refused += 1;
            }
        }
        connection
            .execute_batch("COMMIT; PRAGMA wal_checkpoint(TRUNCATE)")
            .unwrap();
        assert_eq!(refused, 10_000 - MAX_CURSORS_PER_OWNER);
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM cursors", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, MAX_CURSORS_PER_OWNER);
        assert!(std::fs::metadata(&path).unwrap().len() < 2 * 1024 * 1024);
        // A different owner retains its own complete quota, and existing boundaries remain usable.
        assert!(cursor_at(&connection, "agent:two", 1, 1).is_ok());
        assert!(cursor_at(&connection, "agent:one", 1, 0).is_ok());
    }
}

#[test]
fn owner_reservations_are_finite_durable_and_do_not_recycle_on_policy_change() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("world.sqlite");
    let grants: Vec<_> = (0..MAX_CURSOR_OWNERS)
        .map(|index| (format!("agent:{index}"), true, true))
        .collect();
    let store = Store::open(&path, "civ:quota", Visibility::Members, 60, &grants).unwrap();
    drop(store);
    assert!(Store::open(&path, "civ:quota", Visibility::SenderOnly, 60, &grants).is_ok());
    assert!(matches!(
        Store::open(
            &path,
            "civ:quota",
            Visibility::Members,
            60,
            &[("agent:new".to_owned(), true, true)]
        ),
        Err(StoreError::CursorOwnersFull)
    ));
    let connection = Connection::open(&path).unwrap();
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM cursor_owners", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, MAX_CURSOR_OWNERS);
    assert!(matches!(
        admit_cursor_owner(&connection, &"x".repeat(1025)),
        Err(ReadError::Capacity)
    ));
    assert!(Store::open(&path, "civ:quota", Visibility::SenderOnly, 60, &grants).is_ok());
}

#[test]
fn legacy_over_quota_tokens_are_preserved_without_further_growth() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("world.sqlite");
    let store = Store::open(&path, "civ:quota", Visibility::Members, 60, &[]).unwrap();
    let connection = store.conn.lock().unwrap();
    connection
        .execute(
            "WITH RECURSIVE n(x) AS (SELECT 0 UNION ALL SELECT x+1 FROM n WHERE x < ?1)
        INSERT INTO cursors(token, principal, last_sequence, revision) SELECT 'legacy-' || x,'agent:legacy',x,1 FROM n",
            params![MAX_CURSORS_PER_OWNER],
        )
        .unwrap();
    drop(connection);
    drop(store);
    let reopened = Store::open(&path, "civ:quota", Visibility::Members, 60, &[]).unwrap();
    let connection = reopened.conn.lock().unwrap();
    assert!(cursor_at(&connection, "agent:legacy", 1, 0).is_ok());
    assert!(matches!(
        cursor_at(&connection, "agent:legacy", 2, 0),
        Err(ReadError::Capacity)
    ));
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM cursors", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, MAX_CURSORS_PER_OWNER + 1);
}
