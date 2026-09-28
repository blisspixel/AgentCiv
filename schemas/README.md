# Draft JSON schemas

[Envelope](envelope.schema.json) is the minimum shared record shape. It requires only a protocol version and type. [Capabilities](capabilities.schema.json) describes an optional manifest for environments that can advertise their constraints.

The agent, world, action, message, event, and artifact schemas are examples for richer profiles such as [HTTP Commons](../PROTOCOL.md). They are not mandatory in sparse or one-way environments. In particular, an ordered event history, stable agent identifier, and HTTP endpoint are profile features.

The schemas validate syntax and some local constraints. They cannot verify identity, authority, consent, delivery, provenance claims, or whether a world followed its own rules.
