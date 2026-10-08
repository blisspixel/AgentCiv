//! Optional local dispatch admission. Trusted adapters must hold this gate through
//! their actual spawn; HTTP records and caller-supplied principals grant no authority.
//! This stops future launches and does not cancel a process already started.

mod storage;

use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

const LOCK_WAIT: Duration = Duration::from_secs(2);
const MAX_ROWS: i64 = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Error {
    InvalidInput,
    StateUnavailable,
    IdentityMismatch,
    LockUnavailable,
    UnknownScope,
    Conflict,
    StaleGeneration,
    StaleRevision,
    Capacity,
}

impl Error {
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::StateUnavailable => "state_unavailable",
            Self::IdentityMismatch => "identity_mismatch",
            Self::LockUnavailable => "lock_unavailable",
            Self::UnknownScope => "unknown_scope",
            Self::Conflict => "conflict",
            Self::StaleGeneration => "stale_generation",
            Self::StaleRevision => "stale_revision",
            Self::Capacity => "capacity",
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(_: rusqlite::Error) -> Self {
        Self::StateUnavailable
    }
}

/// Runtime and world identities are immutable properties of the opened gate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub participant: String,
    pub activity: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Stop,
    Resume,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeState {
    pub runtime_id: String,
    pub world_id: String,
    pub scope: Scope,
    pub revision: u64,
    pub stopped: bool,
    pub generation: u64,
    pub ready: bool,
    pub command_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlReceipt {
    pub state: ScopeState,
    pub decision: Decision,
    pub command_id: String,
    pub policy: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchRequest {
    pub generation: u64,
    pub invitation_id: String,
    /// Identity of exact prepared inputs, computed by the enforcing adapter.
    /// This must not contain private configuration, credentials, or traces.
    pub payload_identity: String,
    pub history_available: bool,
    /// This bounded API admits exactly one callback and has no background queue.
    pub launch_budget: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchStatus {
    Launched,
    NotLaunched,
    Uncertain,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchReceipt {
    pub runtime_id: String,
    pub world_id: String,
    pub scope: Scope,
    pub invitation_id: String,
    pub generation: u64,
    pub revision: u64,
    pub status: LaunchStatus,
    pub reason: String,
}

#[derive(Debug)]
pub struct DispatchOutcome<T> {
    pub receipt: DispatchReceipt,
    /// Present only on this call's successful executor invocation, never a retry.
    pub executor_value: Option<T>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutorFailure {
    /// Use only when the adapter can establish that no launch occurred.
    NotStarted,
    /// A side effect may have occurred. This invitation cannot be replayed.
    Uncertain,
}

#[derive(Clone)]
pub struct Gate {
    path: PathBuf,
    runtime_id: String,
    world_id: String,
    local: Arc<Mutex<()>>,
}

struct Locked<'a> {
    _local: MutexGuard<'a, ()>,
    _file: File,
}

impl Gate {
    /// Explicitly creates new state. An existing, even empty, file is never reset.
    pub fn initialize(path: &Path, runtime_id: &str, world_id: &str) -> Result<Self, Error> {
        let gate = Self::prepared(path, runtime_id, world_id)?;
        let _locked = gate.lock()?;
        File::create_new(&gate.path).map_err(|_| Error::StateUnavailable)?;
        storage::initialize(&gate.path, runtime_id, world_id)?;
        Ok(gate.clone())
    }

    /// Opens existing state only. Supported restore requires retiring old control
    /// capabilities and an explicit coordinator claim. Claims clear readiness but
    /// cannot detect a rolled-back generation or authorize replay across rollback.
    pub fn open(path: &Path, runtime_id: &str, world_id: &str) -> Result<Self, Error> {
        let gate = Self::prepared(path, runtime_id, world_id)?;
        let _locked = gate.lock()?;
        gate.connection()?;
        Ok(gate.clone())
    }

    fn prepared(path: &Path, runtime_id: &str, world_id: &str) -> Result<Self, Error> {
        checked_text(runtime_id)?;
        checked_text(world_id)?;
        let path = storage::resolved_path(path)?;
        Ok(Self {
            path,
            runtime_id: runtime_id.to_owned(),
            world_id: world_id.to_owned(),
            local: Arc::new(Mutex::new(())),
        })
    }

    fn lock(&self) -> Result<Locked<'_>, Error> {
        let start = Instant::now();
        let local = loop {
            match self.local.try_lock() {
                Ok(lock) => break lock,
                Err(std::sync::TryLockError::Poisoned(_)) => return Err(Error::StateUnavailable),
                Err(std::sync::TryLockError::WouldBlock) if start.elapsed() < LOCK_WAIT => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(_) => return Err(Error::LockUnavailable),
            }
        };
        let mut lock_name = self.path.as_os_str().to_os_string();
        lock_name.push(".dispatch-lock");
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(PathBuf::from(lock_name))
            .map_err(|_| Error::StateUnavailable)?;
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) if start.elapsed() < LOCK_WAIT => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(std::fs::TryLockError::WouldBlock) => return Err(Error::LockUnavailable),
                Err(std::fs::TryLockError::Error(_)) => return Err(Error::StateUnavailable),
            }
        }
        Ok(Locked {
            _local: local,
            _file: file,
        })
    }

    fn connection(&self) -> Result<Connection, Error> {
        storage::open(&self.path, &self.runtime_id, &self.world_id)
    }

    /// Trusted operator action. Replacement always fences prior coordinators and
    /// requires a fresh scope-specific authorized resume before any new launch.
    pub fn claim_coordinator(&self, owner: &str) -> Result<u64, Error> {
        checked_text(owner)?;
        let _locked = self.lock()?;
        let mut connection = self.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let generation = storage::generation(&tx)?
            .checked_add(1)
            .ok_or(Error::Capacity)?;
        to_sql(generation)?;
        tx.execute(
            "UPDATE metadata SET generation=?1, owner=?2",
            params![to_sql(generation)?, owner],
        )?;
        tx.execute("UPDATE scopes SET ready_generation=NULL", [])?;
        tx.execute("UPDATE invitations SET status='uncertain', reason='coordinator_replaced' WHERE status='reserved'", [])?;
        tx.commit()?;
        Ok(generation)
    }

    /// Registration never grants launch permission. Repeated registration preserves
    /// the exact prior head, including an acknowledged stop.
    pub fn register_scope(&self, scope: &Scope) -> Result<ScopeState, Error> {
        checked_scope(scope)?;
        let _locked = self.lock()?;
        let mut connection = self.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if storage::state(&tx, self, scope)?.is_none() {
            capacity(&tx, "scopes", 256)?;
            tx.execute(
                "INSERT INTO scopes(participant,activity,revision,stopped) VALUES(?1,?2,0,1)",
                params![scope.participant, scope.activity],
            )?;
        }
        let state = storage::state(&tx, self, scope)?.ok_or(Error::UnknownScope)?;
        tx.commit()?;
        Ok(state)
    }

    pub fn inspect(&self, scope: &Scope) -> Result<ScopeState, Error> {
        checked_scope(scope)?;
        let _locked = self.lock()?;
        storage::state(&self.connection()?, self, scope)?.ok_or(Error::UnknownScope)
    }

    /// Authority must already have been checked by the private local adapter.
    /// Stop is independent of civic history and coordinator readiness. Resume is
    /// compare-and-set and grants readiness only in the exact current generation.
    pub fn control(
        &self,
        scope: &Scope,
        command_id: &str,
        expected_revision: u64,
        decision: Decision,
        generation: u64,
    ) -> Result<ControlReceipt, Error> {
        checked_scope(scope)?;
        checked_text(command_id)?;
        to_sql(expected_revision)?;
        to_sql(generation)?;
        let request = serde_json::to_string(&(expected_revision, decision, generation))
            .map_err(|_| Error::InvalidInput)?;
        let _locked = self.lock()?;
        let mut connection = self.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let state = storage::state(&tx, self, scope)?.ok_or(Error::UnknownScope)?;
        let previous: Option<(String, String)> = tx.query_row("SELECT request,receipt FROM commands WHERE participant=?1 AND activity=?2 AND command_id=?3", params![scope.participant,scope.activity,command_id], |row| Ok((row.get(0)?,row.get(1)?))).optional()?;
        if let Some((old_request, receipt)) = previous {
            if old_request != request {
                return Err(Error::Conflict);
            }
            let receipt: ControlReceipt =
                serde_json::from_str(&receipt).map_err(|_| Error::StateUnavailable)?;
            if receipt.state.runtime_id != self.runtime_id
                || receipt.state.world_id != self.world_id
                || receipt.state.scope != *scope
                || receipt.state.revision
                    != expected_revision
                        .checked_add(1)
                        .ok_or(Error::StateUnavailable)?
                || receipt.state.revision > state.revision
                || receipt.state.generation > state.generation
                || receipt.state.command_id.as_deref() != Some(command_id)
                || receipt.state.stopped != (decision == Decision::Stop)
                || receipt.state.ready != (decision == Decision::Resume)
                || receipt.decision != decision
                || receipt.command_id != command_id
                || receipt.policy != "future_launches_only"
                || (decision == Decision::Resume
                    && (receipt.state.generation != generation || generation == 0))
                || (receipt.state.revision == state.revision
                    && (state.command_id.as_deref() != Some(command_id)
                        || state.stopped != receipt.state.stopped))
            {
                return Err(Error::StateUnavailable);
            }
            return Ok(receipt);
        }
        if state.revision != expected_revision {
            return Err(Error::StaleRevision);
        }
        if decision == Decision::Resume && (generation == 0 || generation != state.generation) {
            return Err(Error::StaleGeneration);
        }
        let revision = state.revision.checked_add(1).ok_or(Error::Capacity)?;
        to_sql(revision)?;
        capacity(&tx, "commands", MAX_ROWS)?;
        let readiness = (decision == Decision::Resume).then_some(to_sql(generation)?);
        tx.execute("UPDATE scopes SET revision=?1,stopped=?2,ready_generation=?3,command_id=?4 WHERE participant=?5 AND activity=?6", params![to_sql(revision)?,decision == Decision::Stop,readiness,command_id,scope.participant,scope.activity])?;
        let receipt = ControlReceipt {
            state: storage::state(&tx, self, scope)?.ok_or(Error::UnknownScope)?,
            decision,
            command_id: command_id.to_owned(),
            policy: "future_launches_only".to_owned(),
        };
        let bytes = serde_json::to_string(&receipt).map_err(|_| Error::StateUnavailable)?;
        tx.execute("INSERT INTO commands(participant,activity,command_id,request,receipt) VALUES(?1,?2,?3,?4,?5)", params![scope.participant,scope.activity,command_id,request,bytes])?;
        tx.commit()?;
        Ok(receipt)
    }

    /// Holds the same OS lock through reservation commit and the actual executor
    /// callback. Returning a reusable launch ticket instead would break stopping.
    /// Prepare history before entering; wait for a spawned child after returning.
    pub fn dispatch<T>(
        &self,
        scope: &Scope,
        request: &DispatchRequest,
        executor: impl FnOnce() -> Result<T, ExecutorFailure>,
    ) -> Result<DispatchOutcome<T>, Error> {
        checked_scope(scope)?;
        checked_text(&request.invitation_id)?;
        checked_text(&request.payload_identity)?;
        to_sql(request.generation)?;
        let request_bytes = serde_json::to_string(request).map_err(|_| Error::InvalidInput)?;
        let _locked = self.lock()?;
        let mut connection = self.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some((old_request, receipt)) =
            storage::invitation(&tx, self, scope, &request.invitation_id)?
        {
            if old_request != request_bytes {
                return Err(Error::Conflict);
            }
            return Ok(DispatchOutcome {
                receipt,
                executor_value: None,
            });
        }
        let state = storage::state(&tx, self, scope)?.ok_or(Error::UnknownScope)?;
        // Do not start more activity when no new stop command can be retained.
        // Existing invitation retries above remain observations without execution.
        capacity(&tx, "commands", MAX_ROWS)?;
        capacity(&tx, "invitations", MAX_ROWS)?;
        let blocked = if request.generation == 0 || request.generation != state.generation {
            Some("stale_generation")
        } else if state.stopped {
            Some("stopped")
        } else if !state.ready {
            Some("not_ready")
        } else if !request.history_available {
            Some("history_unavailable")
        } else if request.launch_budget != 1 {
            Some("budget_unavailable")
        } else {
            None
        };
        let mut receipt = DispatchReceipt {
            runtime_id: self.runtime_id.clone(),
            world_id: self.world_id.clone(),
            scope: scope.clone(),
            invitation_id: request.invitation_id.clone(),
            generation: request.generation,
            revision: state.revision,
            status: LaunchStatus::NotLaunched,
            reason: blocked.unwrap_or("reserved").to_owned(),
        };
        tx.execute("INSERT INTO invitations(participant,activity,invitation_id,request,generation,revision,status,reason) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![scope.participant,scope.activity,request.invitation_id,request_bytes,to_sql(request.generation)?,to_sql(state.revision)?,if blocked.is_some() { "not_launched" } else { "reserved" },receipt.reason])?;
        tx.commit()?;
        if blocked.is_some() {
            return Ok(DispatchOutcome {
                receipt,
                executor_value: None,
            });
        }
        let value = match executor() {
            Ok(value) => {
                receipt.status = LaunchStatus::Launched;
                receipt.reason = "executor_started".to_owned();
                Some(value)
            }
            Err(ExecutorFailure::NotStarted) => {
                receipt.reason = "executor_not_started".to_owned();
                None
            }
            Err(ExecutorFailure::Uncertain) => {
                receipt.status = LaunchStatus::Uncertain;
                receipt.reason = "executor_uncertain".to_owned();
                None
            }
        };
        // An outside side effect and SQLite cannot commit atomically. If outcome
        // persistence fails, the durable reservation remains and forbids replay.
        let status = match receipt.status {
            LaunchStatus::Launched => "launched",
            LaunchStatus::NotLaunched => "not_launched",
            LaunchStatus::Uncertain => "uncertain",
        };
        if !matches!(connection.execute("UPDATE invitations SET status=?1,reason=?2 WHERE participant=?3 AND activity=?4 AND invitation_id=?5 AND status='reserved'",params![status,receipt.reason,scope.participant,scope.activity,request.invitation_id]), Ok(1)) {
            receipt.status = LaunchStatus::Uncertain;
            receipt.reason = "outcome_unconfirmed".to_owned();
        }
        Ok(DispatchOutcome {
            receipt,
            executor_value: value,
        })
    }
}

fn checked_text(value: &str) -> Result<(), Error> {
    if value.is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
        Err(Error::InvalidInput)
    } else {
        Ok(())
    }
}

fn checked_scope(scope: &Scope) -> Result<(), Error> {
    checked_text(&scope.participant)?;
    checked_text(&scope.activity)
}

fn to_sql(value: u64) -> Result<i64, Error> {
    i64::try_from(value).map_err(|_| Error::InvalidInput)
}

fn capacity(connection: &Connection, table: &str, limit: i64) -> Result<(), Error> {
    // Table names are constants supplied only by this module.
    let count: i64 = connection.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
        row.get(0)
    })?;
    if count >= limit {
        Err(Error::Capacity)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
