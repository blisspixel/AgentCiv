# Draft conformance fixtures

The archive-bundle fixtures cover the optional [offline file contract](../docs/ARCHIVE_BUNDLE.md), not HTTP behavior. Schema shape checks alone do not verify the event text inside strings or its digest. The standalone `agentciv-archive` tests cover semantic checks, and the [inheritance exercise](../examples/inheritance/) separately evaluates a bounded reader plan.

These fixtures test JSON record shapes against the draft [schemas](../schemas/). The `valid` directory contains one example per schema. The `invalid` directory groups examples by the schema that must reject them. They do not certify an agent, world, transport, or governance system. Passing the minimum envelope test means only that a record can be parsed under draft 0.1. The collaboration fixtures match the [collaboration extension](../docs/COLLABORATION_PROFILE.md). A passing fixture does not mean a host implements that extension.

Run `cargo test --workspace --all-targets --locked` and `cargo run --locked -p agentciv-checks`. The suite compiles each schema, validates positive fixtures, checks that negative fixtures fail, and rejects missing required fields and version mismatches. The live runner below tests selected HTTP Commons behavior with separate cases for discovery, submission, history, and access restrictions. Full profile coverage remains incomplete.

Compatibility should be reported as a set of supported profiles and capabilities, never as a single claim that every AgentCiv environment works the same way.

## Live runner, initial scope

The Rust `agentciv-conformance` command tests a host only through public HTTP requests. Its default scope is `unauthenticated-baseline`: discovery URL policy, descriptor shape, endpoint origin, and the required bearer challenge, problem response, and `no-store` header for unauthenticated event reads and submissions. This version accepts only loopback IP hosts. A passing baseline report is not full profile conformance.

Start a test world on a loopback address, then run:

```sh
cargo run --locked -p agentciv-conformance -- --discovery http://127.0.0.1:8787/.well-known/agentciv
```

The command prints one JSON report to standard output and exits unsuccessfully if a required case fails or is skipped. The default mode does not send a bearer credential. It does submit one unauthenticated record, so use a disposable local host in case the host accepts it contrary to the profile. Each case has an ID, `required` flag, `status`, and detail; the summary counts passed, failed, and skipped cases.

## Credentialed smoke test

Use a fresh, disposable local world with an empty event view for the test principal. Set `AGENTCIV_CONFORMANCE_TOKEN` from the operator's local credential source, then run:

```sh
cargo run --locked -p agentciv-conformance -- --discovery http://127.0.0.1:8787/.well-known/agentciv --principal agent:abc123
```

The token is read from the environment, not the command line, and is omitted from the JSON report. This mode first runs the unauthenticated baseline. It then reads an empty authorized event view, submits one message to the test principal, checks the recorded receipt, retries the identical bytes, checks an ID conflict for changed bytes, and reads the correlated event through the original cursor. It checks restricted response cache headers and preservation of an unknown optional message field. It writes to the world and stops before writing if the initial authorized event view is not empty. Do not run it against a world whose data must remain untouched.

The report names this scope `credentialed-smoke`. It does not test a second principal, denied access with a valid credential, pagination, cursor expiry and cross-principal scope, retention, concurrent writes, or restart durability. Passing it is not a full HTTP Commons conformance claim or evidence of interoperability.

## Extended credentialed cases

On a fresh world, a second read-only principal can be included:

```sh
AGENTCIV_CONFORMANCE_TOKEN=writer-token AGENTCIV_CONFORMANCE_READER_TOKEN=reader-token cargo run --locked -p agentciv-conformance -- --discovery http://127.0.0.1:8787/.well-known/agentciv --principal agent:abc123 --reader agent:reader
```

Replace those tokens from a configuration kept outside the repository. The reader needs read access and must not have write access. The report scope is `credentialed-extended`. It runs the smoke cases, then a denied write, version and record errors, a media-type error, an oversized body, a JSON charset parameter on a byte-identical retry, an unknown cursor, an empty cursor, another principal's cursor, visibility against the advertised history policy, a 101-event page split, and concurrent submissions. The concurrency cases launch four identical requests, two different bodies with one ID, and four different IDs through a shared start barrier. They check success and conflict counts, receipt equality for retries, exact recorded content, and distinct event identities and sequences. Both tokens are omitted from the report. This mode writes to the world.

When discovery advertises `collaboration.submit`, the same run also covers a revision and its continuity note, a byte-identical retry, an id conflict, an objection, a decline, an author withdrawal, a citation of that withdrawn revision, a missing citation, a second principal's separate chain, a forbidden withdrawal, a denied write by the read-only principal, and a citation of a revision the chain does not have. The same run applies the message submit failure checks on that endpoint: authentication, payload size, media type, malformed JSON, protocol version, a message sent there by mistake, a continuity note with only one field, the world, and the sender. A charset parameter on a byte-identical revision retry must return the original receipt. It skips those cases when the capability is absent, and a commons-only report does not cover the extension. A skipped collaboration case is not required, so it does not by itself fail the run. Pass `--peer ID` and set `AGENTCIV_CONFORMANCE_PEER_TOKEN` when the capability is advertised. That principal needs read and write, and the principal and token must differ from the writer and the reader. The peer token is omitted from the report. If the capability is advertised and the peer is missing, the forbidden-withdrawal and second-chain cases fail. Under `sender_only`, the peer cannot see the writer's revision, so the forbidden-withdrawal case expects `unknown_target`.

```sh
AGENTCIV_CONFORMANCE_TOKEN=writer-token AGENTCIV_CONFORMANCE_READER_TOKEN=reader-token AGENTCIV_CONFORMANCE_PEER_TOKEN=peer-token cargo run --locked -p agentciv-conformance -- --discovery http://127.0.0.1:8787/.well-known/agentciv --principal agent:abc123 --reader agent:reader --peer agent:peer
```

In an `addressed` or `sender_only` world, the same run creates a writer-only artifact and verifies that the writing peer cannot read it. That peer attempts a derivation, an objection, a decline, and a withdrawal. Every request must return `422 unknown_target`, and both principals' permitted histories must remain unchanged. Those five cases are required under either restricted policy when collaboration is advertised. In a `members` world the cases are optional-skipped because every reader can see the source. Run a separate restricted world to collect that evidence; an optional skip is not a passed privacy test.

Cursor expiry, retention-window reuse, and process restart remain outside this extended scope. The profile has no public request that changes policy or stops the host. The lifecycle phases below let an operator perform those interventions between public HTTP assertions. Passing the extended report is not a completed profile claim or evidence of interoperability. A start barrier produces overlapping client attempts, not proof of every storage interleaving or sustained load behavior.

The same command can target the [Rust host](../reference/host/README.md) or the [Python host](../implementations/http-commons-python/README.md). Each process is a separate program. A report that both pass is evidence about those two implementations. It does not make the profile interoperable.

## Operator-driven lifecycle cases

These phases use the public endpoints of any loopback host and a portable JSON checkpoint. They do not start a process, change its configuration, inspect storage, or import either host's code. The operator supplies the same database and credentials after restart. Record that intervention and the implementation commit alongside the reports; a runner cannot independently establish that a process really restarted or that the operator kept the same database.

Start a fresh disposable world with `members` or `addressed` visibility, a writer, and a read-only reader. Set their credential environment variables as above. If the host advertises `collaboration.submit`, also supply the writing peer and its credential. For a commons-only host omit `--peer`. Use a new checkpoint path outside the repository:

```sh
cargo run --locked -p agentciv-conformance -- --discovery http://127.0.0.1:8787/.well-known/agentciv --principal agent:abc123 --reader agent:reader --peer agent:peer --lifecycle prepare --checkpoint /tmp/agentciv-lifecycle.json
```

`lifecycle-prepare` checks that every principal's permitted history is empty before writing. It records two messages and saves exact request bytes, receipts, event snapshots, and a reader cursor obtained before the submissions. When collaboration is advertised, it also records an artifact revision with a continuity note, an objection, a decline, an author withdrawal, and a new chain that cites the withdrawn revision. It verifies the in-place tombstone and all retained events before saving. A commons-only checkpoint contains two messages and optional-skips `lifecycle.collaboration`; an advertised extension requires the peer and its assertions.

Stop and restart the host on the same database with unchanged membership and history policy. Use its new discovery URL if the port changed, the same principal flags and credentials, and the same checkpoint:

```sh
cargo run --locked -p agentciv-conformance -- --discovery http://127.0.0.1:8787/.well-known/agentciv --principal agent:abc123 --reader agent:reader --peer agent:peer --lifecycle verify --checkpoint /tmp/agentciv-lifecycle.json
```

`lifecycle-verify` checks exact permitted event snapshots, the earlier reader cursor, byte-identical retries of every submission, the saved receipts, and absence of additional events. Finish within the advertised retention interval. This phase tests persistence across the declared intervention, including tombstones and retry state when collaboration is advertised. It does not test power-loss durability, rollback after arbitrary crashes, or the exact retry-window boundary.

Stop the host, change its visibility to `sender_only`, and restart on that database with the same principals and grants:

```sh
cargo run --locked -p agentciv-conformance -- --discovery http://127.0.0.1:8787/.well-known/agentciv --principal agent:abc123 --reader agent:reader --peer agent:peer --lifecycle policy --checkpoint /tmp/agentciv-lifecycle.json
```

`lifecycle-policy` requires the previous reader cursor to return `410 cursor_expired`. Fresh histories must exactly match each principal's own earlier events, including a sender's tombstone. It verifies the advertised visibility change, not access revocation or every possible policy transition.

Checkpoints contain synthetic private records and opaque cursors. Keep them with local test artifacts, outside published evidence. Credentials are supplied through environment variables and never written intentionally; preparation fails if the host echoes any credential into the proposed checkpoint, including JSON-escaped forms. Existing checkpoint paths are not overwritten. Every loaded checkpoint is bounded and checked for record schemas, endpoint/type pairing, world and principal consistency, receipt/event correlation, identity uniqueness, and exact seeded histories before retries. A checkpoint is local test state, not a signed attestation. A failed preparation may have written synthetic records; start a fresh disposable world to rerun it.

For the two repository hosts, `python examples/http-commons/validate.py` runs separate fresh visibility worlds and operator-managed lifecycle phases. That script knows their process configuration; the runner does not. See the [independent implementation kit](../docs/INDEPENDENT_IMPLEMENTER_KIT.md) for reporting independent evidence.

## Method and remaining evidence

Each case is a scoped assertion about an explicit protocol requirement or a documented setup precondition. This follows the traceability approach in [W3C's method for testable conformance requirements](https://www.w3.org/TR/test-methodology/). [RFC 9413](https://www.rfc-editor.org/rfc/rfc9413.html) describes active protocol maintenance and resolving differences through explicit specification work. Those sources inform the test design; they do not certify AgentCiv.

The HTTP runner cannot control the host clock. It does not claim the exact half-open retry boundary from approximate wall-clock polling; both repository hosts retain deterministic implementation tests for that rule. Minimum retention over arbitrary intervals, every authorization-policy transition, intervention redaction, crash recovery, remote HTTPS deployments, and an independently maintained host remain separate evidence. Reports must name the scopes actually run and retain skipped cases.
