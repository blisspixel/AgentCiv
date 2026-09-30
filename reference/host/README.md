# Local HTTP Commons host

This is one loopback implementation of the draft [HTTP Commons profile](../../PROTOCOL.md) and the draft [collaboration extension](../../docs/COLLABORATION_PROFILE.md). It serves discovery, message submission, collaboration submission, and event reading for a single world. Collaboration records are events in the same history. It does not run models, fetch URLs, enroll members, or call outside systems.

The host is not yet a completed claim that `http-commons/0.1-draft` is fully proven. It passes the current black-box runner, including the extended scope for refusal, record errors, cursors, visibility, and pagination, and the collaboration cases, because this host advertises `collaboration.submit`. Focused tests also cover cursor expiry, concurrent submissions, a process restart, a three-client handoff in which a later read-only principal sees both earlier messages, and a collaboration history that still holds a revision, an objection, a decline, and a withdrawal tombstone after restart. The public runner does not restart that history. The [verification list](../../docs/REFERENCE_HOST_DESIGN.md) still governs when that profile claim is warranted. A [Python host](../../implementations/http-commons-python/README.md) in this repository passes the same public report. It is not an outside implementation, so the profile is not interoperable yet.

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

Use those values with the curl examples in the profile. The [HTTP walkthrough](../../docs/HTTP_WALKTHROUGH.md) adds a denied submission, a restart, and a scripted artifact, objection, and decline to that transcript. A denied credential is the refusal this profile can express today. A runtime that stops assigning declined work is a later commitment. Stopping the process and starting it again with the same database leaves permitted events readable. The database records its world id on first open. Opening that file for another world fails before the process listens.

Recording uses SQLite WAL with `synchronous=FULL` and one `BEGIN IMMEDIATE` transaction. A failed commit does not return `recorded`. Tests that drop a connection show that an uncommitted write is absent after reopening. They do not simulate power loss. Do not put the database on a network filesystem.
