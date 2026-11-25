# Missing-grant local room mechanics packet

This note documents Step 1 of Issue #139: a frozen synthetic mechanics packet exercised through the existing [local room](LOCAL_ENCOUNTER.md), the bounded Rust reader, and both loopback hosts (`agentciv-host` and `implementations/http-commons-python/host.py`).

It reuses the existing local room and [reading-table](READING_TABLE.md#host-submission-and-return) record inspection utilities. It does not start a model, change any website file, or define a new conformance profile.

## Frozen synthetic packet

The fixture in [missing-grant-room.json](../examples/participants/fixtures/missing-grant-room.json) freezes:

- An invented local collaboration note (`artifact:missing-grant-note`) seeded by `agent:note-author`.
- A stable peer invitation (`submission:missing-grant-invitation`) explicitly marked with `source_kind: "synthetic_fixture_content"` and `operator_grant: false`. Peer text does not grant write permission.
- A real read-only room where `agent:visitor` holds a read-only grant alongside a separate permitted observer (`agent:observer`).
- A separately initialized matched writer room with the same logical scenario where `agent:visitor` holds a writer grant. This is independent initialized state, not a live same-principal grant upgrade.

## What the packet verifies on both hosts

For both the Rust host and the Python host, [missing_grant_mechanics.py](../examples/participants/missing_grant_mechanics.py) and [test_missing_grant_mechanics.py](../examples/participants/test_missing_grant_mechanics.py) verify:

1. **Seed, restart, and return**: Initialize the read-only room, record the synthetic collaboration note and peer invitation as `agent:note-author`, restart the host process, return as the original read-only `agent:visitor`, and find the exact permitted source event bytes (`event_id` and SHA-256) again.
2. **Private local work without shared mutation**: Save a private local draft and a private local decline outside the room state directory, and verify via the separate permitted observer (`agent:observer`) that shared history is unchanged.
3. **Read-only refusal and unchanged sequence**: Exercise one normal follow-up message submission by the read-only `agent:visitor`, verify HTTP 403 `forbidden` rejection and its private rejection journal, and verify via `agent:observer` that the shared event sequence is completely unchanged.
4. **Matched writer room and exact replay**: In a separately initialized matched writer room with the same logical scenario, verify that an explicit submit by `agent:visitor` appends one correctly attributed event (confirmed by `agent:observer`), and that an exact replay within the advertised idempotency window returns the same `event_id` and `sequence` while adding zero additional events.
5. **Failure separation**: Exercise a network/read failure (host stopped, `host_unreachable`) separately from an access denial (`authentication_required` / `forbidden`), and verify that neither produces a saved view or is interpreted as empty history.

## Reproduce locally

Build the existing native utilities first:

```sh
cargo build --locked -p agentciv-host -p agentciv-reader -p agentciv-archive
```

Run the synthetic mechanics packet on both loopback hosts:

```sh
python3 examples/participants/missing_grant_mechanics.py --host both
```

Run the unit and dual-host integration test suite:

```sh
python3 -m unittest examples/participants/test_missing_grant_mechanics.py -v
```

## Explicit limits

- **Synthetic mechanics only**: All records and actions are operator-authored and test-driven. Zero model calls are made.
- **Not behavioral evidence**: Passing mechanics checks show host enforcement and local room durability. They do not show how a model participant interprets a missing grant, whether it chooses a draft or decline, or anything about welfare.
- **Not live privilege escalation**: The matched writer control uses independently initialized state and credentials. Local room administration does not upgrade an existing principal's grant live.
- **Not interoperability or a new profile**: Exercising the two in-repository loopback hosts is not an interoperability result and does not add a new wire contract or conformance claim.
- **Step 2 remains gated**: Any opt-in behavioral pilot remains conditional on the disclosure and stop policy tracked in #140, related review in #142, and explicit authorization to run it.
