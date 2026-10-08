use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags, OptionalExtension, Row, params};

use super::{DispatchReceipt, Error, Gate, LaunchStatus, Scope, ScopeState};

pub(super) fn resolved_path(path: &Path) -> Result<PathBuf, Error> {
    if path.is_file() {
        return path.canonicalize().map_err(|_| Error::StateUnavailable);
    }
    let name = path.file_name().ok_or(Error::InvalidInput)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok(parent
        .canonicalize()
        .map_err(|_| Error::StateUnavailable)?
        .join(name))
}

fn connection(path: &Path) -> Result<Connection, Error> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(Duration::from_millis(250))?;
    connection.execute_batch("PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;")?;
    Ok(connection)
}

pub(super) fn initialize(path: &Path, runtime: &str, world: &str) -> Result<(), Error> {
    let mut connection = connection(path)?;
    let tx = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    tx.execute_batch("CREATE TABLE metadata (singleton INTEGER PRIMARY KEY CHECK(singleton=1), format INTEGER NOT NULL CHECK(format=1), runtime TEXT NOT NULL, world TEXT NOT NULL, generation INTEGER NOT NULL CHECK(generation>=0), owner TEXT);
        CREATE TABLE scopes (participant TEXT NOT NULL, activity TEXT NOT NULL, revision INTEGER NOT NULL CHECK(revision>=0), stopped INTEGER NOT NULL CHECK(stopped IN (0,1)), ready_generation INTEGER CHECK(ready_generation>0), command_id TEXT, PRIMARY KEY(participant,activity));
        CREATE TABLE commands (participant TEXT NOT NULL, activity TEXT NOT NULL, command_id TEXT NOT NULL, request TEXT NOT NULL, receipt TEXT NOT NULL, PRIMARY KEY(participant,activity,command_id), FOREIGN KEY(participant,activity) REFERENCES scopes(participant,activity));
        CREATE TABLE invitations (participant TEXT NOT NULL, activity TEXT NOT NULL, invitation_id TEXT NOT NULL, request TEXT NOT NULL, generation INTEGER NOT NULL CHECK(generation>=0), revision INTEGER NOT NULL CHECK(revision>=0), status TEXT NOT NULL CHECK(status IN ('reserved','launched','not_launched','uncertain')), reason TEXT NOT NULL, PRIMARY KEY(participant,activity,invitation_id), FOREIGN KEY(participant,activity) REFERENCES scopes(participant,activity));")?;
    tx.execute(
        "INSERT INTO metadata VALUES(1,1,?1,?2,0,NULL)",
        params![runtime, world],
    )?;
    tx.commit()?;
    Ok(())
}

pub(super) fn open(path: &Path, runtime: &str, world: &str) -> Result<Connection, Error> {
    let connection = connection(path)?;
    let integrity: String = connection.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(Error::StateUnavailable);
    }
    let identity: (i64, String, String, u64) = connection.query_row(
        "SELECT format,runtime,world,generation FROM metadata WHERE singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, unsigned(row, 3)?)),
    )?;
    if identity.0 != 1 {
        return Err(Error::StateUnavailable);
    }
    if identity.1 != runtime || identity.2 != world {
        return Err(Error::IdentityMismatch);
    }
    // Missing or ambiguous tables are unavailable, never migrated to active state.
    for table in ["metadata", "scopes", "commands", "invitations"] {
        let count: i64 = connection.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name=?1",
            [table],
            |row| row.get(0),
        )?;
        if count != 1 {
            return Err(Error::StateUnavailable);
        }
    }
    Ok(connection)
}

pub(super) fn generation(connection: &Connection) -> Result<u64, Error> {
    Ok(connection.query_row(
        "SELECT generation FROM metadata WHERE singleton=1",
        [],
        |row| unsigned(row, 0),
    )?)
}

pub(super) fn state(
    connection: &Connection,
    gate: &Gate,
    scope: &Scope,
) -> Result<Option<ScopeState>, Error> {
    let generation = generation(connection)?;
    Ok(connection.query_row("SELECT revision,stopped,ready_generation,command_id FROM scopes WHERE participant=?1 AND activity=?2", params![scope.participant,scope.activity], |row| {
        let ready_generation: Option<i64> = row.get(2)?;
        Ok(ScopeState { runtime_id: gate.runtime_id.clone(), world_id: gate.world_id.clone(), scope: scope.clone(), revision: unsigned(row,0)?, stopped: row.get(1)?, generation, ready: ready_generation == i64::try_from(generation).ok() && generation != 0, command_id: row.get(3)? })
    }).optional()?)
}

pub(super) fn invitation(
    connection: &Connection,
    gate: &Gate,
    scope: &Scope,
    id: &str,
) -> Result<Option<(String, DispatchReceipt)>, Error> {
    let row: Option<(String,u64,u64,String,String)> = connection.query_row("SELECT request,generation,revision,status,reason FROM invitations WHERE participant=?1 AND activity=?2 AND invitation_id=?3", params![scope.participant,scope.activity,id], |row| Ok((row.get(0)?,unsigned(row,1)?,unsigned(row,2)?,row.get(3)?,row.get(4)?))).optional()?;
    let Some((request, generation, revision, status, reason)) = row else {
        return Ok(None);
    };
    if !matches!(
        (status.as_str(), reason.as_str()),
        ("reserved", "reserved")
            | ("launched", "executor_started")
            | (
                "not_launched",
                "stale_generation"
                    | "stopped"
                    | "not_ready"
                    | "history_unavailable"
                    | "budget_unavailable"
                    | "executor_not_started"
            )
            | ("uncertain", "executor_uncertain" | "coordinator_replaced")
    ) {
        return Err(Error::StateUnavailable);
    }
    let (status, reason) = match status.as_str() {
        "launched" => (LaunchStatus::Launched, reason),
        "not_launched" => (LaunchStatus::NotLaunched, reason),
        "uncertain" => (LaunchStatus::Uncertain, reason),
        "reserved" => (
            LaunchStatus::Uncertain,
            "reservation_unconfirmed".to_owned(),
        ),
        _ => return Err(Error::StateUnavailable),
    };
    Ok(Some((
        request,
        DispatchReceipt {
            runtime_id: gate.runtime_id.clone(),
            world_id: gate.world_id.clone(),
            scope: scope.clone(),
            invitation_id: id.to_owned(),
            generation,
            revision,
            status,
            reason,
        },
    )))
}

fn unsigned(row: &Row<'_>, column: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(column)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(column, value))
}
