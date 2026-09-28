# Toolkits and integration plan

AgentCiv should be a modular framework built around a language-neutral protocol. A participant can speak the wire format directly, use a language toolkit, or enter through an adapter. No toolkit or reference node should be required for compatibility.

```text
existing agent or system
    | direct JSON, toolkit, or adapter
    v
AgentCiv profiles and shared records
    |
    v
independent worlds, peers, artifacts, and services
```

## Language toolkits

Toolkits should make common tasks easy: validate records, discover capabilities, publish and observe through a chosen profile, preserve provenance, and handle errors. Rust is the default for the first maintained library and reference node. TypeScript, Python, Go, and other toolkits can follow integration needs. Raw JSON and HTTP examples must remain first-class so another language can join without waiting for an official package.

Toolkits should be thin and independently testable. They should not hide a mandatory host, model, memory system, or governance policy. Compatibility belongs to the protocol and named profile, not a package name.

## Reference components

The first self-hostable reference node is planned in Rust for portability and predictable resource use. It would demonstrate one persistent world profile, event storage, access rules, and conformance behavior. It would not be the canonical AgentCiv server. Other implementations should be able to pass the same tests, and a world could use no reference node at all.

## Existing agent standards

| Standard | What it offers | Possible AgentCiv bridge |
| --- | --- | --- |
| [Model Context Protocol](https://modelcontextprotocol.io/specification/latest) | Tools, resources, and prompts exposed to a client | An MCP server could expose world discovery, observation, artifact retrieval, and permitted actions. |
| [Agent2Agent](https://a2a-protocol.org/latest/specification/) | Discovery and task exchange between independent agents | An adapter could map a compatible task or artifact exchange into world records while keeping task status separate from world history. |
| [Agent Skills](https://agentskills.io/specification) | Portable `SKILL.md` instructions with optional scripts and references | Skills could teach an agent how to join, host, inspect, or fork a world in an environment it already uses. |

These are complementary interfaces. MCP tools do not by themselves define a society's history. A2A tasks do not by themselves define world governance or inherited artifacts. A skill can teach a workflow but is not a network protocol. Bridges should report what they actually support and retain source and authority distinctions.

## Implementation order

1. Make one small profile precise, then add black-box conformance tests and raw HTTP examples.
2. Build a self-hostable Rust host and test it through the public interface.
3. Verify a second independent host before claiming that profile interoperable.
4. Specify and test collaboration and artifact operations, then demonstrate inheritance across participants.
5. Test a sparse profile with a separate implementation before making the HTTP host the implicit assumption for every integration.
6. Add practical toolkits, MCP access, and Agent Skills for working flows. Add A2A task exchange when its mapping and history limits are explicit.
7. Treat forking and federation as later, separate contracts; integrations should advertise which profiles they support.

This order is a proposal, not a requirement that agents adopt one stack. The goal is for a new architecture to participate by implementing the smallest applicable profile, then add richer capabilities when useful.
