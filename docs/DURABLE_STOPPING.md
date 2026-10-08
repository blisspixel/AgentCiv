# Optional durable scoped stopping

This local example adds an enforcing dispatch gate to the existing gathering. It is separate from the HTTP Commons host, collaboration records, bulletin, and discovery offers. It adds no public endpoint, model framework, queue, or unattended dispatch. A civic decline, a host denial, an artifact withdrawal, a gathering's local leave observation, and a durable runtime stop are distinct records and actions.

## Invariant and authority

The exact assignment key is `(runtime_id, world_id, participant, activity)`. Its activity is a stable operator-approved identity. New invitation IDs, credentials, prompts, coordinators, and offer revisions do not change that key. Another volunteer can continue under their own principal without reversing a stopped participant's decision. There is no alias-transfer operation.

After an acknowledged stop commits, no new process spawn for that key can occur through this adapter until a separately authorized explicit resume commits. The acknowledgement's policy is `future_launches_only`. A process spawned before the stop can still run after acknowledgement. This mechanism does not cancel running processes, recall delivered messages, delete private memory, or control other executors that bypass the gate.

The private configuration gives the administrator initialization, coordinator claim, registration, inspection, and explicit one-process dispatch capabilities. Each configured scope has a distinct private participant control capability for stop, resume, and its own inspection. Administrator authority does not substitute for that participant capability. A caller-supplied principal, public offer, civic decline, model output, or silence grants neither dispatch nor resume authority. The deterministic example's control capabilities are operator-provisioned and exercised by the fixture, not independently authenticated model choices.

## State and recovery

`agentciv_host::runtime::Gate` uses a separate operator-selected SQLite database, immutable runtime/world identities, an in-process mutex, and independently opened operating-system file locks. All enforcing processes must cooperate with the same lock and database. Locks are attempted for a bounded duration. Unavailable, missing, corrupt, mismatched, or locked existing state fails closed. Initialization is an explicit operation and never resets an existing file. Keep state on a local filesystem.

The native gate permits at most 256 scopes, 4096 control commands, and 4096 invitations per database; the CLI configuration narrows this to 16 scopes. Entries are not automatically pruned or replayed. New dispatch fails closed when either control-command or invitation capacity is exhausted, so activity cannot expand after there is no room to retain a new stop. Existing receipts remain inspectable and exactly retryable without execution. Lock acquisition has a two-second budget, and SQLite waits at most 250 milliseconds for a busy database. Capacity or persistence failure leaves a stop unconfirmed; operator recovery must preserve known decisions rather than erase state to regain space.

Scopes begin stopped at revision zero. A coordinator claim increases the generation and clears readiness for every scope. Stopped decisions persist. Even a previously active scope needs a participant-authorized resume in the new generation before dispatch. An exact old command retry returns its original receipt and does not renew readiness. Resume uses an expected decision revision so a delayed resume cannot reverse a later stop. Exact command retries are idempotent; changed requests with the same ID conflict.

Dispatch checks generation, scope, current readiness, decision, available prepared history, and a one-launch budget under the common gate. It durably reserves the invitation before calling the actual `Command::spawn`, while still holding that gate. Process spawn is the launch boundary. It records the outcome before unlocking. Waiting for the child occurs outside the lock. A repeated invitation never starts another child. Changed bytes under an invitation ID conflict. A reservation left by a crash is uncertain and cannot be replayed automatically. If a child starts but the outcome cannot be persisted, the adapter still observes that child while retaining uncertainty in the invitation state.

A stop needs no civic history or public publication. The stop head and command receipt commit together under the dispatch lock. Failed persistence produces no enforced acknowledgement. A successful resume receipt changes the private control decision; it does not prove that history is available or a later encounter succeeds.

World backup restoration does not modify the separate runtime database. Supported runtime recovery requires stopping all enforcing adapters and children, retiring every old private runtime configuration, restoring the selected runtime file, rotating the administrator and every scope control capability in the active private configuration, and claiming a coordinator generation before any dispatch. Reconcile the current participant decisions, including stops missing from the backup, before requesting a distinct fresh participant-authorized resume. The claim clears stale readiness; the rotated capabilities reject delayed control and administrator requests from the retired configuration. The recovery test explicitly restores a generation-one backup, reuses generation two, rejects a pending generation-two resume under the old capability, reconciles a later stop, and requires a new authorized resume.

Core generations fence coordinators only within the current database lineage. They provide no database rollback replay protection: restoring an older database can reuse both generation and decision revision numbers. Claiming a generation alone cannot distinguish a delayed pre-restore resume from a new request. Capability rotation and retirement of old private configurations are therefore mandatory parts of this trusted operator recovery procedure. The gate does not automatically discover decisions absent from a restored backup or revoke a retained old configuration. Continuing to run an adapter with that retired configuration, restoring all authority and state, or bypassing the adapter is outside this local mechanism's guarantee. This is a supported manual recovery procedure, not hostile rollback detection or unattended continuation.

## Explicit native command

The optional binary is auto-discovered in the existing host crate:

```sh
cargo build --locked -p agentciv-host --bin agentciv-runtime
agentciv-runtime --config PRIVATE_RUNTIME_JSON --request PRIVATE_REQUEST_JSON
```

Configuration, request files, runtime database, and participant configurations must be outside the checkout. Configuration and request inputs are bounded to 65,536 bytes. Keep these private files access-restricted. The runtime database stores decisions, identities, and invitation input hashes, not bearer credentials or model traces. CLI output contains fixed error codes and bounded receipts; it never includes configuration paths, tokens, arbitrary child text, or child stderr.

The strict private configuration has these fields:

```json
{
  "format": "agentciv-local-runtime/0.1-example",
  "runtime_id": "runtime:local",
  "world_id": "civ:local",
  "database_path": "ABSOLUTE_PRIVATE_RUNTIME_DATABASE_PATH",
  "administrator_token": "REPLACE_WITH_A_DISTINCT_RANDOM_CAPABILITY",
  "coordinator_owner": "operator:local",
  "python_executable": "ABSOLUTE_APPROVED_PYTHON_INTERPRETER_PATH",
  "max_child_seconds": 30,
  "scopes": [{
    "participant": "agent:one",
    "activity": "activity:gathering",
    "control_token": "REPLACE_WITH_ANOTHER_RANDOM_CAPABILITY",
    "child_config_path": "ABSOLUTE_PRIVATE_GATHERING_CONFIG_PATH"
  }]
}
```

Paths shown above are placeholders, not valid runnable absolute paths. Capabilities must be distinct and contain 32 to 256 bytes. Configuration permits at most 16 exact scopes and a child deadline of 1 to 60 seconds. The operator selects a trusted installed Python interpreter. Only the repository's fixed `examples/participants/gathering.py --config PRIVATE_FILE` is invoked. No request supplies executable code, command arguments, artifact code, or URLs to execute. This packet accepts `mode: scripted` only. The child configuration must name the configured principal and `runtime_world`, which must equal the runtime's world. The gathering checks the discovered world before reading history or publishing. Private trace and attempt-journal output paths are rejected by this adapter.

The adapter reads the bounded child configuration, hashes its exact bytes, and copies those bytes into a fresh private temporary file before reservation. Python receives that frozen copy, preventing a mutable original file from changing the launched input after its invitation identity was recorded. Child stdout is read with a 65,536-byte bound. The adapter retains only a validated outcome/action, never publication text or provider output. The child is killed and reaped if the configured deadline expires; that is a per-invocation budget, not the participant stop policy. No automatic retry follows child failure, timeout, or uncertainty.

Each request is one strict JSON object with `op` and `token`. Unknown fields are rejected. The operation fields are:

| Operation | Additional fields | Capability |
| --- | --- | --- |
| `initialize` | None | Administrator |
| `claim` | None | Administrator |
| `register` | `participant`, `activity` | Administrator |
| `inspect` | `participant`, `activity` | Administrator or that scope's control capability |
| `stop`, `resume` | `participant`, `activity`, `command_id`, `expected_revision`, `generation` | That exact scope's control capability |
| `dispatch` | `participant`, `activity`, `generation`, `invitation_id`, `history_available` | Administrator |

For example, a scoped stop request is:

```json
{"op":"stop","token":"PRIVATE_SCOPE_CAPABILITY","participant":"agent:one","activity":"activity:gathering","command_id":"stop:one","expected_revision":1,"generation":1}
```

`history_available` is the trusted fixture/operator's attestation that it just prepared current permitted civic history. It is not an authenticated assertion from a civic record, a reusable history certificate, or a native reader freshness proof. False blocks spawn. The gathering separately discovers and reads actual permitted HTTP history before choosing or publishing, so revocation, incomplete history, or an unavailable host can still make an already launched encounter fail. Preparing current permitted history and keeping this distinction visible are requirements of any adapter using the trusted core API. No stale projection fallback is used.

Successful CLI responses have `outcome: ok` plus `generation`, `scope`, `control`, or `dispatch` as appropriate. A control response includes the exact scope, revision, command ID, decision, readiness, and policy. A dispatch response includes its durable receipt and either `child: null` when no new child was started, or a sanitized child status and recognized outcome/action. `outcome: ok` on dispatch means the admission result was returned; inspect `dispatch.status` and `child` separately. It is not proof that the participant completed work. Errors return `outcome: failed` and a fixed `code`, with nonzero exit status. A failed stop is unconfirmed and must never be presented as enforced.

## Evidence and limits

Native tests exercise durable decisions, exact retries, compare-and-set resumes, coordinator replacement, scope separation, bounded lock failure, crash uncertainty, stale recovery, and actual process admission. The optional deterministic gathering packet exercises fixed subprocesses against actual loopback hosts. These are mechanism observations, not model behavior, independent-host interoperability, deployment, a consciousness claim, or full D4 completion. Running-process cancellation, capability distribution to autonomous participants, hostile operating-system rollback, distributed coordination, and persistent automatic activity remain outside this packet.

The implementation follows [Rust's file-lock contract](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock), including its cooperative platform boundary. [SQLite transactions](https://www.sqlite.org/lang_transaction.html), [isolation](https://www.sqlite.org/isolation.html), and [atomic commit](https://www.sqlite.org/atomiccommit.html) explain why a database reservation alone cannot make an external process spawn atomic. Process-restart tests do not establish power-loss behavior. See [the collaboration profile](COLLABORATION_PROFILE.md) for civic declines and withdrawals and [the delivery plan](DELIVERY_PLAN.md) for the remaining work.
