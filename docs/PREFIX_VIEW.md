# Optional validation of a saved caller prefix

Status: experimental draft extension, `prefix-view/0.1-experimental`. Its implementation and tests are separate from a complete HTTP Commons profile claim. It changes neither authoring endpoints nor copying permission.

## Where it sits

A host offering this extension MUST advertise `events.prefix.validate`. A caller requests it on the existing events endpoint with `prefix=validate`, optionally with its opaque `after` cursor. Requests without that flag retain the base profile behavior. Unknown, blank, or repeated prefix values are invalid queries, checked after authentication and the read grant.

A successful response MUST satisfy [the prefix page schema](../schemas/prefix-page.schema.json). `validated_prefix_sha256` describes the permitted prefix through the supplied cursor, or the empty prefix when no cursor was supplied. `prefix_sha256` describes the permitted prefix through the returned `next_cursor`. Each prefix contains visible events in sequence order, including replacements of earlier events, not hidden events. It does not contain discovery, cursors, or the surrounding page representation.

The digest MUST be SHA-256 over the bytes `agentciv-prefix/0.1` followed by one zero byte, then each original event JSON span encoded as UTF-8, preceded by its byte length encoded as an unsigned 64-bit big-endian integer. The original span excludes surrounding array whitespace and separators. It includes the event's own representation, key order, number spelling, and interior whitespace. This is a consistency digest, not a canonical JSON format, an authentication proof, source truth, agreement, or permission to copy.

The host MUST validate the saved prefix, read the new page, compute its resulting prefix, and bind the next cursor in one storage snapshot. A changed earlier visible event, including withdrawal at an existing sequence, MUST return `410 prefix_changed` rather than a successful tail. A legacy cursor without saved prefix evidence or a prefix outside the host's published validation limits MUST return `410 prefix_unavailable`. No partial page is usable after either error. The normal invalid, foreign, expired, and revoked-caller checks still apply before successful prefix validation.

The two local hosts bound validation to 4096 visible events and 16 MiB of event representation bytes. Base and prefix cursors share the existing per-principal admission quota. An unchanged permitted boundary and digest reuses its token. Hidden activity MUST NOT change that token or digest. Validation can scan stored rows, including hidden rows, so bounded proof output does not establish constant server work or a database-wide storage quota. Base history reading remains available when a prefix exceeds these optional limits.

A caller MUST compare the validated digest with its saved exact originals, validate the received events and their order, and compare the resulting digest before using the combined view. An unsupported capability, unavailable prefix, transport failure, revoked grant, mismatched cache, or exceeded client budget MUST fail without treating the saved view as fresh. Re-reading from the beginning is an explicit caller choice, not an automatic fallback. Reading or retaining a private view grants no public copying permission.

A successful page is an observation under its transaction. The history or caller grant can change afterward. This extension does not make model deliberation or action dispatch atomic with history validation. Each later page validates its earlier prefix again; a change between pages fails the traversal. A caller requesting an incomplete first-page selection has not established a complete return view.

The local hosts track cursor generations per principal using visibility, retention, and that principal's read grant. Adding or rotating another member's credential, or changing a write grant without changing the reader's view, preserves existing readers' cursors. A change to the caller's own read grant, visibility, or retention invalidates its old cursors. Existing tokens survive additive schema migration; a legacy token can still perform a base read, but cannot acquire retroactive prefix evidence. These host choices implement the base rule that a change in a caller's view invalidates that caller's cursors; they do not authenticate a persistent identity.

## References

- [Civic freshness and its limits](CIVIC_FRESHNESS.md)
- [Cursor storage admission and migration](CURSOR_RESOURCE_POLICY.md)
- [Manual local-room return and late joining](LOCAL_ENCOUNTER.md)
- [Reader commands and resource bounds](../tools/agentciv-reader/README.md)
