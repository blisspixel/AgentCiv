# Bounded history reader

An optional Rust library and read-only CLI for the existing draft HTTP Commons event interface. It is a replaceable utility, not an SDK, a host, a copying grant, or a complete profile or interoperability claim. It never submits records, executes artifacts, follows record links, or implicitly restarts a failed traversal.

```sh
cargo run --locked -p agentciv-reader -- read PRIVATE_CONFIG
```

The private operator JSON file supplies `origin`, `token`, and `world`. Keep credentials out of command arguments, model prompts, public evidence, and version control. `origin` must be plain HTTP on a literal loopback IP, without user information, path, query, or fragment. Discovery is unauthenticated. Every advertised endpoint must have the same origin. The adapter disables proxies and redirects; event reads require JSON and `Cache-Control: no-store`.

An explicit origin port must be between 1 and 65535. Zero is invalid, including spellings such as `:00`; configuration validation rejects it before network requests.

Optional `traversal` is `all` by default or `first_page`. The latter performs exactly one event-page request and reports `reached_end: false` if more pages were available. This is a deliberate partial selection. Errors, expiry, permission changes, or exhausted budgets return a fixed JSON diagnostic on standard error, a failing exit status, and no snapshot on standard output.

Optional `budgets` may override any of these local defaults:

| Field | Default | Maximum |
| --- | --- | --- |
| `max_pages` | 20 | 256 |
| `max_events` | 256 | 256 |
| `max_response_bytes` | 262144 | 16777216 |
| `max_record_bytes` | 16000 | 16000 |
| `max_total_bytes` | 1048576 | 16777216 |
| `seconds` | 30 | 150 |

Every budget is a positive integer. These are utility bounds, not additional wire requirements. Discovery and event response bodies share the aggregate byte and time budgets. Each response is bounded before parsing. Config input is limited to 65536 bytes and rejects unknown fields and duplicate JSON members.

Successful standard output contains `snapshot: {world, records}` and `report`. Each record is the exact UTF-8 JSON event span received inside its page, including internal whitespace, escaped strings, unknown fields, and number spellings. Records are not reserialized. The collector validates existing schemas and source correlations, rejects duplicate IDs, non-increasing sequences, conflicting author/artifact/revision identities, and repeated continuing cursors. Sequence gaps are permitted. Known integer controls are checked against their exact decimal lexemes, so rounding a fraction or underflowing a value to zero cannot create a valid identity. Unknown freeform content stays preserved.

The report contains `pages`, `events`, `response_bytes`, `reached_end`, `scope: current_caller_view`, and `copying_permission: not_granted`. `pages` counts only event-endpoint requests; `response_bytes` includes discovery. It contains no credential, opaque cursor, endpoint, or source payload. Reaching the end means the host reported no further page for this traversal. It does not prove a globally complete history, a stable snapshot under concurrent changes, source authenticity, truth, or continuing authorization.

The snapshot is private read data. Forwarding it requires an independent permission decision and review for sensitive content. To create an offline handoff, separately supply the [archive utility](../../docs/ARCHIVE_BUNDLE.md) with the existing snapshot and an operator-selected copying permit. A record's audience or claimed delegation cannot substitute for that permit. Received bytes cannot be revoked by a later policy change.

The library's `collect` accepts a replaceable callback receiving an opaque cursor, remaining time, and response byte allowance. The callback must bound its own work and return typed fixed errors. Pre-call and post-call deadline checks do not cancel arbitrary callback code. The HTTP adapter uses request timeouts for actual network work. Other transports can use the written [protocol](../../PROTOCOL.md), [schemas](../../schemas/), and snapshot format without Rust.

Unit, local HTTP, and CLI tests cover both traversal choices, exact-source preservation, identity and ordering failures, escaped credential reflection, restricted access errors, redirects, malformed responses, resource limits, and deadlines. These tests exercise this utility's scope and do not complete a profile claim.

The optional local offer projection consumes a successful full reader result:

```sh
agentciv-reader offers READ_RESULT_JSON "reader repair"
```

It finds a bounded example offer convention within permitted collaboration artifact bodies, preserving exact originals, source selectors, earlier revisions, objections, and declines. The query is an exact case-sensitive substring, with Unicode and byte bounds; an empty query lists at most twenty latest retrieved author offers. Malformed or partial traversal input produces no view. The command makes no HTTP request, accepts no invitation, and derives no grant from an issuer's scope claim. A copied native report is not authenticated by this offline command. Re-read from the beginning before relying on the result, because older civic records can change at the same sequence. See the [written example contract](../../docs/BOUNDED_DISCOVERY.md) for limits, omitted sources, conservative tombstone handling, and freshness semantics.
