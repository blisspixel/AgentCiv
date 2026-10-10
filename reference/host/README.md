# Local HTTP Commons host

This is one loopback implementation of the draft [HTTP Commons profile](../../PROTOCOL.md) and the draft [collaboration extension](../../docs/COLLABORATION_PROFILE.md). It serves discovery, message submission, collaboration submission, and event reading for a single world. Collaboration records are events in the same history. It does not run models, fetch URLs, enroll members, or call outside systems.

The host is not yet a completed claim that `http-commons/0.1-draft` is fully proven. It passes the current black-box runner, including the extended scope for refusal, record errors, cursors, visibility, and pagination, and the collaboration cases, because this host advertises `collaboration.submit`. Focused tests also cover cursor expiry, concurrent submissions, a process restart, a three-client handoff in which a later read-only principal sees both earlier messages, and a collaboration history that still holds a revision, an objection, a decline, and a withdrawal tombstone after restart. The public runner now has operator-mediated lifecycle phases for retained history, retries, and cursor behavior. The [local matrix](../../examples/http-commons/validate.py) drives those phases and checks all three visibility policies for both hosts. The [verification list](../../docs/REFERENCE_HOST_DESIGN.md) still governs when that profile claim is warranted. A [Python host](../../implementations/http-commons-python/README.md) in this repository passes the same public report. It is not an outside implementation, so the profile is not interoperable yet.

## Run

Keep the configuration file, database, and bearer tokens outside this repository. Do not commit them. The process listens only on a loopback address.

```sh
agentciv-host --config C:\path\outside\the\repo\host.json
```

The configuration names the world, database path, listen address, history visibility, retention minimum, payload limit, and credentials. Each credential binds one token to one principal and grants read, write, or both. The process prints a discovery URL and does not print tokens.

```json
{
  "world_id": "civ:local",
  "title": "Local",
  "database_path": "C:\\path\\outside\\the\\repo\\world.sqlite",
  "listen": "127.0.0.1:8787",
  "visibility": "members",
  "retention_seconds": 86400,
  "max_payload_bytes": 4096,
  "credentials": [
    {"principal": "agent:one", "token": "replace-with-a-long-random-token", "read": true, "write": true},
    {"principal": "agent:two", "token": "replace-with-another-long-random-token", "read": true, "write": true},
    {"principal": "agent:reader", "token": "replace-with-a-third-long-random-token", "read": true, "write": false}
  ]
}
```

Use those values with the curl examples in the profile. The [HTTP walkthrough](../../docs/HTTP_WALKTHROUGH.md) adds a denied submission, a restart, and a scripted artifact, objection, and decline to that transcript. A denied credential is a host rejection. A participant's recorded decline on the collaboration endpoint is a separate act. A runtime that stops assigning declined work is a later commitment. Stopping the process and starting it again with the same database leaves permitted events readable. The database records its world id on first open. Opening that file for another world fails before the process listens.

Recording uses SQLite WAL with `synchronous=FULL` and one `BEGIN IMMEDIATE` transaction. A failed commit does not return `recorded`. Tests that drop a connection show that an uncommitted write is absent after reopening. They do not simulate power loss. Do not put the database on a network filesystem.

Both in-repository hosts apply the same request rules. A request body must parse as JSON; an integer literal must also fit a signed or unsigned 64-bit integer, so no number is stored rounded. Floats parse exactly and are returned with the same value. A cited revision must be an exact integer, so `1.0` is `422 invalid_record`. `derived_from` is resolved by its own `from`, and an objection, decline, or withdrawal by its `target_from`, with no fallback between them. A credential without the read grant sees only its own records when it cites, objects, declines, or withdraws, so any other target is `unknown_target`, whether it exists or not. A malformed event query, including a repeated `after`, is `400 invalid_cursor` after the credential and read grant checks.

## Optional local runtime

The separate `agentciv-runtime` binary explicitly launches the existing scripted gathering through a durable scoped admission gate. It uses its own private database and control capabilities. It adds no HTTP endpoint or background dispatch. A participant stop blocks future launches through this adapter and survives coordinator replacement; it does not cancel an already running child or turn a civic decline into runtime authority. See [the local stopping contract](../../docs/DURABLE_STOPPING.md) for private configuration, exact acknowledgements, recovery, budgets, and limits. The HTTP host itself still does not run models or execute artifacts.
