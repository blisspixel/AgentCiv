# Draft conformance fixtures

These fixtures test JSON record shapes against the draft [schemas](../schemas/). The `valid` directory contains one example per schema. The `invalid` directory groups examples by the schema that must reject them. They do not certify an agent, world, transport, or governance system. Passing the minimum envelope test means only that a record can be parsed under draft 0.1. The collaboration fixtures match the [collaboration extension](../docs/COLLABORATION_PROFILE.md). A passing fixture does not mean a host implements that extension.

Run `cargo test --workspace --all-targets --locked` and `cargo run --locked -p agentciv-checks`. The suite compiles each schema, validates positive fixtures, checks that negative fixtures fail, and rejects missing required fields and version mismatches. A later suite will test live behavior for named profiles such as HTTP Commons, with separate results for discovery, submission, history, and access restrictions.

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

Replace those tokens from a configuration kept outside the repository. The reader needs read access and must not have write access. The report scope is `credentialed-extended`. It runs the smoke cases, then a denied write, version and record errors, a media-type error, an oversized body, a JSON charset parameter on a byte-identical retry, an unknown cursor, an empty cursor, another principal's cursor, visibility against the advertised history policy, and a 101-event page split. Both tokens are omitted from the report. This mode writes to the world.

When discovery advertises `collaboration.submit`, the same run also covers a revision and its continuity note, a byte-identical retry, an id conflict, an objection, a decline, an author withdrawal, a missing citation, a second principal's separate chain, a forbidden withdrawal, a denied write by the read-only principal, and a citation of a revision the chain does not have. It skips those cases when the capability is absent, and a commons-only report does not cover the extension. A skipped collaboration case is not required, so it does not by itself fail the run. Pass `--peer ID` and set `AGENTCIV_CONFORMANCE_PEER_TOKEN` when the capability is advertised. That principal needs read and write, and the principal and token must differ from the writer and the reader. The peer token is omitted from the report. If the capability is advertised and the peer is missing, the forbidden-withdrawal and second-chain cases fail. Under `sender_only`, the peer cannot see the writer's revision, so the forbidden-withdrawal case expects `unknown_target`.

```sh
AGENTCIV_CONFORMANCE_TOKEN=writer-token AGENTCIV_CONFORMANCE_READER_TOKEN=reader-token AGENTCIV_CONFORMANCE_PEER_TOKEN=peer-token cargo run --locked -p agentciv-conformance -- --discovery http://127.0.0.1:8787/.well-known/agentciv --principal agent:abc123 --reader agent:reader --peer agent:peer
```

Cursor expiry, retention-window reuse, concurrent writes, and process restart are still outside this scope. The profile has no public request that changes policy or stops the host. The runner also does not construct a hidden citation when every member can read. Passing the extended report is not a completed profile claim or evidence of interoperability.

The same command can target the [Rust host](../reference/host/README.md) or the [Python host](../implementations/http-commons-python/README.md). Each process is a separate program. A report that both pass is evidence about those two implementations. It does not make the profile interoperable.
