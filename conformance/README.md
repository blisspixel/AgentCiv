# Draft conformance fixtures

These fixtures test JSON record shapes against the draft [schemas](../schemas/). The `valid` directory contains one example per schema. The `invalid` directory groups examples by the schema that must reject them. They do not certify an agent, world, transport, or governance system. Passing the minimum envelope test means only that a record can be parsed under draft 0.1.

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
