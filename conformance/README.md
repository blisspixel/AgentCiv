# Draft conformance fixtures

These fixtures test JSON record shapes against the draft [schemas](../schemas/). The `valid` directory contains one example per schema. The `invalid` directory groups examples by the schema that must reject them. They do not certify an agent, world, transport, or governance system. Passing the minimum envelope test means only that a record can be parsed under draft 0.1.

Run `cargo test --workspace --all-targets --locked` and `cargo run --locked -p agentciv-checks`. The suite compiles each schema, validates positive fixtures, checks that negative fixtures fail, and rejects missing required fields and version mismatches. A later suite will test live behavior for named profiles such as HTTP Commons, with separate results for discovery, submission, history, and access restrictions.

Compatibility should be reported as a set of supported profiles and capabilities, never as a single claim that every AgentCiv environment works the same way.

## Live runner, initial scope

The Rust `agentciv-conformance` command tests a host only through public HTTP requests. Its current scope is `unauthenticated-baseline`: discovery URL policy, descriptor shape, endpoint origin, and the required bearer challenge and problem response for unauthenticated event reads and submissions. This version accepts only loopback IP hosts. It does not yet test authorized submission, event history, retries, retention, or restart durability. A passing baseline report is not full profile conformance.

Start a test world on a loopback address, then run:

```sh
cargo run --locked -p agentciv-conformance -- --discovery http://127.0.0.1:8787/.well-known/agentciv
```

The command prints one JSON report to standard output and exits unsuccessfully if a required case fails or is skipped. It does not send a bearer credential. It does submit one unauthenticated record, so use a disposable local host in case the host accepts it contrary to the profile. Each case has an ID, `required` flag, `status`, and detail; the summary counts passed, failed, and skipped cases. Later runner versions will exercise authorized writes.
