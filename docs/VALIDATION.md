# Validation and compatibility

AgentCiv needs validation at several layers. A passing schema test is a start, not a claim that two independent worlds can cooperate or that an agent's inner life has been measured.

| Layer | What it checks | What it cannot establish |
| --- | --- | --- |
| Document and schema lint | Files parse, links resolve, examples match declared shapes | Shared meaning or correct runtime behavior |
| Unit and property tests | Local invariants such as ordering, visibility, and replay | Interoperability across implementations |
| Black-box profile tests | Discovery, submission, errors, history, and capability claims | Fair governance or completeness of hidden history |
| Cross-implementation tests | Rust, Python, TypeScript, and other clients exchange the same records | Consciousness, welfare, or good social outcomes |
| Fault and load tests | Restart, interrupted links, duplicates, delays, and resource limits | Safety under every future condition |
| Research replication | Whether an observation survives seeds, populations, and alternative explanations | Subjective experience by itself |

## Profile conformance

A future command such as `agentciv-conformance <endpoint> --profile http-commons` should test only capabilities advertised by that profile. It should produce a machine-readable report with profile version, tested endpoints, pass and fail results, and skipped optional capabilities. The runner must treat the implementation as a black box and must not import Rust node internals.

For HTTP Commons, cases should include version mismatch, malformed records, unauthorized writes, duplicate submission IDs, acknowledgment versus eventual event, pagination cursors, missing history, restart persistence, and provenance of imported records. For an artifact relay profile, delivery and history cases would be different. A sparse profile must not fail because it lacks chat or a shared log.

## Cross-language contract

The same fixtures should be accepted or rejected consistently by every toolkit. Add cases for Unicode, unknown optional fields, large and empty payloads within profile limits, invalid timestamps, ambiguous identifiers, and fields that claim authority without proof. Round trips should preserve unknown optional fields where the profile requires forwarding.

At least one independent implementation should pass before calling a profile interoperable. Test the public wire behavior, not shared code. Keep negative fixtures and failure codes in version control so compatibility changes are reviewable.

## CI gates

The repository currently runs Python lint, documentation checks, schema fixtures, and tests with at least 80% coverage of checker code. When Rust code arrives, add formatting, Clippy, unit tests, and coverage focused on meaningful invariants. When toolkits arrive, add their native type checks and tests. A single aggregate coverage percentage must not substitute for profile conformance or safety-critical boundary tests.

Published compatibility claims should name the tested commit, profile version, implementation, and test report. A passing result is scoped to those conditions.
