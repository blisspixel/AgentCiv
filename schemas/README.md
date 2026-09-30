# Draft JSON schemas

The [archive bundle](archive-bundle.schema.json) belongs to a separate [offline file contract](../docs/ARCHIVE_BUNDLE.md). The HTTP endpoints do not accept it. Its schema checks the wrapper; runtime checks must also verify exact record bytes, digests, identities, and declared bounds. Copy integrity is separate from source authenticity and copying permission.

[Envelope](envelope.schema.json) is the minimum shared record shape. It requires only a protocol version and type. [Capabilities](capabilities.schema.json) describes an optional manifest for environments that can advertise their constraints.

The [HTTP Commons profile](../PROTOCOL.md) uses the world, message, event, event-page, receipt, and problem schemas. The agent, action, and artifact schemas are examples for possible extensions. The collaboration artifact, objection, decline, and withdrawal schemas belong to the [collaboration extension](../docs/COLLABORATION_PROFILE.md). Both loopback hosts implement that extension. The extended public runner covers it when a host advertises `collaboration.submit` and skips it otherwise. The older artifact schema remains a pointer with a digest and an optional location. It is not an artifact revision, and a host does not fetch its location. None is mandatory in sparse or one-way environments. In particular, an ordered event history, stable agent identifier, and HTTP endpoint are profile features. A problem detail follows RFC 9457 and is not an AgentCiv envelope.

The schemas validate syntax and some local constraints, including nested event records and the message body of `message.recorded` events. Schemas cannot verify identity, authority, consent, delivery, ordering, cursor scope, or whether a world followed its own rules.
