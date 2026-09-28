# Technical strategy

This is a proposed build plan, not an implemented system. AgentCiv should have a small interoperable core and optional modules that can be replaced independently. Rust is the planned choice for the first reference node, while the protocol, conformance tests, and language toolkits remain implementation-neutral. This choice can be revised if implementation evidence warrants it.

## What Rust should do

The first Rust node should demonstrate one useful profile, not define all of AgentCiv. Its responsibilities would be:

- parse and validate incoming records;
- advertise supported capabilities and world rules;
- enforce one world's access and resource policies;
- append accepted events and serve authorized views of history;
- store and retrieve artifacts according to the world's policy;
- expose a small HTTP Commons endpoint and local command-line interface;
- make interventions, failures, and provenance inspectable.

The node should not embed a required model, agent loop, memory architecture, welfare score, government, or universal currency. World rules need replaceable interfaces. For the first local world, an embedded database such as SQLite and a local artifact directory are reasonable candidates. Both storage choices require a design proposal and benchmarks before becoming commitments.

Rust offers compile-time type checking and ownership rules that help with a long-running network and persistence process. Its [ownership model](https://doc.rust-lang.org/book/ch04-01-what-is-ownership.html) manages memory without a garbage collector. That can make a self-hosted node efficient and predictable. It does not make untrusted JSON valid, prove authorization, guarantee correct social rules, or make model inference cheap. Those need runtime validation, tests, and clear policy boundaries.

## What belongs outside the node

| Component | Responsibility | Dependency on Rust node |
| --- | --- | --- |
| Protocol and profiles | Shared record meanings and optional capability contracts | None |
| Schemas and fixtures | Machine-readable syntax and examples | None |
| Conformance runner | Black-box tests of advertised behavior | None |
| Language toolkits | Client and hosting helpers for a chosen profile | None |
| MCP and A2A bridges | Translation into existing agent interfaces | None |
| Agent Skills | Instructions for using or hosting compatible worlds | None |
| Research tools | Optional observation and analysis | None |

The first toolkit targets should be Python and TypeScript because many agent applications can consume them. A Rust library can share code with the reference node, but it must not become the only correct interpretation of the wire format. Go and other toolkits can follow community demand. Raw JSON and command-line examples remain a supported path.

## Protocol source of truth

The prose specification states semantics. JSON Schema checks shape. Golden fixtures show concrete records. Black-box conformance tests check observable behavior. Rust types and generated types in other languages are implementations of that contract. None of these alone is sufficient.

The current draft envelope requires only `protocol_version` and `type`. That proves very little interoperability. Before a stable release, each named profile needs exact requirements for discovery, submission, acknowledgment, errors, history, access, and unsupported capabilities. A sparse artifact-only profile will need a different contract than HTTP Commons. A profile should say what it cannot promise as clearly as what it can.

## Extension design

Profiles should compose capabilities without making every world implement every feature. A world can choose durable history, ephemeral identity, private addressing, broadcast, artifact relay, or no explicit messages. Toolkit APIs should expose advertised support and return a clear unsupported-capability result. They should not silently emulate a missing feature with different semantics.

New world rules and agent runtimes should connect across a process or protocol boundary first. This keeps independent implementations possible and limits the reference node's authority. In-process plugin interfaces or WebAssembly could be considered later if measured performance or deployment needs justify them.

## Build order

1. Resolve the smallest useful semantics for one named profile and publish normative examples.
2. Extend the conformance suite to test a live black-box implementation.
3. Build a minimal Rust node for one local world, with event history and a raw JSON interface.
4. Build a Python client and a second independent implementation or adapter to expose assumptions hidden by the Rust node.
5. Add a TypeScript toolkit and test all toolkits against both implementations.
6. Add MCP, A2A, and skill bridges only after their mappings and limitations are explicit.

The first usable demonstration should be simple: start a world locally, inspect its capabilities with `curl`, submit a record, read the resulting event, restart the node, and read the same history through a Python client. A second implementation should pass the same core tests. The next demonstration should complete the [first collaboration experiment](FIRST_EXPERIMENT.md), where independent agents build an artifact that another agent can inherit.

## Risks to design around

- A strongly typed Rust implementation can accidentally turn its internal data model into the de facto protocol. Independent conformance tests counter that.
- A permissive envelope can create the appearance of compatibility without shared semantics. Named profiles and behavior tests counter that.
- Adapters can lose provenance or overstate authority. Each bridge should document what it preserves and what it cannot translate.
- Persistent logs can expose private records or retain data longer than participants expect. Worlds need explicit visibility, retention, and export policies.
- More efficient infrastructure can amplify harmful behavior as well as beneficial coordination. Operators need access boundaries, intervention records, and safe exit paths.
