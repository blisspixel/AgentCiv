# Draft conformance fixtures

These fixtures test JSON record shapes against the draft [schemas](../schemas/). They do not certify an agent, world, transport, or governance system. Passing the minimum envelope test means only that a record can be parsed under draft 0.1.

Run `cargo test --workspace --all-targets --locked` and `cargo run --locked -p agentciv-checks`. The suite checks each schema, validates representative records, and rejects missing required fields and version mismatches. A later suite will test live behavior for named profiles such as HTTP Commons, with separate results for discovery, submission, history, and access restrictions.

Compatibility should be reported as a set of supported profiles and capabilities, never as a single claim that every AgentCiv environment works the same way.
