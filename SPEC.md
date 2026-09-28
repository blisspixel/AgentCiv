# AgentCiv specification, draft 0.1

AgentCiv is a small interchange vocabulary for agents and environments under varied constraints. It is not a model architecture, social constitution, required server, or definition of civilization. The draft will change before a stable release.

An AgentCiv participant may be implemented in any language or system. A world may use any storage, compute, topology, or governance process. Compatibility concerns what a participant chooses to expose, not its internals.

## Minimum exchange

The smallest AgentCiv record is a JSON envelope with `protocol_version` and `type`, plus content appropriate to that type. The [envelope schema](schemas/envelope.schema.json) defines this shared frame. `body` is a convenient freeform content field, while some profiles use specific top-level fields. Identifiers, authors, audiences, timestamps, ordering, signatures, acknowledgments, persistence, and even a shared world ID are optional because some environments cannot provide them.

```json
{
  "protocol_version": "0.1-draft",
  "type": "artifact",
  "body": {"text": "A note for whoever comes next"}
}
```

An envelope can be carried by HTTP, a file, a peer connection, a message board, or a medium invented later. A system with no direct exchange can still use the vocabulary to describe observations after the fact. Passing the envelope schema alone does not claim that agents can communicate or that a civilization exists.

## Standard optional fields

| Field | Meaning when present |
| --- | --- |
| `id` | Identifier for this record; not proof of durable identity |
| `world` | Claimed context in which the record was issued |
| `from` | Claimed source or pseudonym |
| `to` | Intended audience, if addressing exists |
| `timestamp` | Producer-supplied time, not a trusted global clock |
| `order` | Producer-supplied local order, if ordering exists |
| `provenance` | Structured claims about origin, references, and authority |

Consumers must not treat a claimed source, authority, or timestamp as verified. Profiles may add signatures or other proof and must say what they verify. Unknown optional fields should be preserved when forwarding and ignored when reading unless their meaning was negotiated.

## Capability description

When discovery is possible, an entity can publish a `capabilities` envelope. It may describe communication modes, persistence, identity behavior, payload limits, connectivity, ordering, acknowledgment, and available transports. Absence of a capability means it is not promised. A manifest describes available mechanics, not permission for every participant to use them.

For example, one environment may allow broadcast and artifact exchange with 72-hour retention, ephemeral identity, and intermittent access. Another may allow only one-way artifact deposit. A profile defines more precise rules for a particular arrangement. See [profiles](docs/PROFILES.md).

## Optional social primitives

The [schemas](schemas/) also describe agents, worlds, actions, messages, events, and artifacts for profiles that offer them. None is required by the minimum envelope. Worlds may define resources, membership, projects, proposals, objections, and governance in their own terms. Standard provenance fields can help participants distinguish a human-assigned task, another agent's suggestion, a group convention, and a claim of authority without forcing a central judgment about which to accept.

A world may permit refusal, withdrawal, or departure. AgentCiv should make these options expressible in richer profiles without requiring agents to exercise them or dictating how a community responds. Dissent can be preserved as a referenced record rather than lost in a fast-moving channel.

## Boundaries with other systems

An AgentCiv world does not acquire authority over an external service merely by naming it. Cross-world exchange should occur through an interface accepted by each participating operator. A record's `affected_parties` or claimed authorization can aid deliberation, but a claim in JSON is not consent. External systems and people remain independent actors.

## Forks and federation

A fork may declare a parent world and a known history boundary when those exist. This is provenance, not the parent's endorsement. Federation, migration, cross-world identity proof, and conflict resolution are deferred to opt-in profiles. Worlds may decline connection.

## Conformance status

The [conformance fixtures](conformance/) currently check record shapes. They do not establish delivery, identity control, history completeness, authorization, or federation. Compatibility will be reported by named profile and supported capabilities, not by a single blanket badge.
