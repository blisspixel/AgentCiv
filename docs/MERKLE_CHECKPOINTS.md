# Tombstone-Compatible Merkle Checkpoints

Status: optional draft extension implemented in `reference/host` (`GET /checkpoint`) and verified by `tools/agentciv-reader` (`agentciv_reader::checkpoint`). This extension is separate from `http-commons/0.1-draft` and does not change message submission, event pagination, or copying permissions.

## Purpose and security boundaries

In `http-commons/0.1-draft`, the host assigns event IDs, sequences, and timestamps. A buggy or malicious host could roll back history after a restore or rewrite earlier records. Merkle checkpoints give readers cryptographic evidence of append-only log growth without requiring a centralized global service.

Important boundary:

- **What unsigned host checkpoints prove:** Relative to a reader's own previously cached checkpoint `(M, root_M)`, an RFC 9162 consistency proof to a newer checkpoint `(N, root_N)` proves that the tree at size `N` is an append-only extension of the tree at size `M`, detecting rollback (`N < M`), same-size split-view changes (`N == M` with `root_N != root_M`), and historical tampering.
- **What unsigned host checkpoints do NOT prove alone:** Without independent witness cosignatures (`c2sp.org/tlog-witness`) or post-quantum signed host checkpoints (tracked in #189), a malicious host can still serve two completely separate append-only forks to two isolated readers who never compare checkpoints. Full split-view protection across participants requires independent witnesses or out-of-band checkpoint gossip.

## Tombstone-compatible leaf commitment

`PROTOCOL.md` allows a host to replace a redacted event's body with an `event.redacted` tombstone (and `docs/COLLABORATION_PROFILE.md` allows `artifact.withdrawn` tombstones) while preserving `id`, `sequence`, and `timestamp`. Hashing the mutable rendered event JSON directly would break historical Merkle consistency proofs whenever a redaction occurs.

Instead, each Merkle leaf commits to an immutable preimage tuple using RFC 9162 domain separation (`0x00` for leaves, `0x01` for interior nodes):

```text
leaf_domain = "agentciv-merkle-leaf/0.1\0"
leaf_data   = leaf_domain
              || u64_be(len(event_id)) || utf8(event_id)
              || u64_be(sequence)
              || u64_be(len(timestamp)) || utf8(timestamp)
              || sha256(original_record_bytes)

leaf_hash   = SHA-256(0x00 || leaf_data)
node_hash   = SHA-256(0x01 || left_hash || right_hash)
empty_root  = SHA-256("")
```

`reference/host` stores `original_record_sha256` alongside each row in SQLite (`events.original_record_sha256`). When an event is redacted (`event.redacted`) or withdrawn (`artifact.withdrawn`), the host removes the sensitive payload from `event_json` while keeping `id`, `sequence`, `timestamp`, and `original_record_sha256` intact. The leaf hash remains identical, so all earlier checkpoints and subsequent RFC 9162 consistency proofs remain valid.

## Discovery and HTTP endpoint

Global Merkle checkpoints cover the entire world event log and are therefore only exposed in `members`-visibility worlds. In `sender_only` and `addressed` worlds, a host omits `endpoints.checkpoint`, `checkpoints`, and `authentication.checkpoint` from `/.well-known/agentciv`, and returns `403 forbidden` from `GET /checkpoint` so unauthorized principals cannot observe hidden event counts or activity timing through `tree_size` and `root_hash_sha256`.

A host offering this extension in a `members`-visibility world advertises it in `/.well-known/agentciv`:

```json
{
  "endpoints": {
    "checkpoint": "http://127.0.0.1:8787/checkpoint"
  },
  "checkpoints": {
    "format": "c2sp.org/tlog-checkpoint",
    "path": "/checkpoint"
  },
  "authentication": {
    "checkpoint": "bearer"
  }
}
```

### `GET /checkpoint`

Requires a valid bearer credential with read access in a `members`-visibility world (matching `GET /events` authorization rules and preventing hidden event count leakage):

- Missing or unknown credential: `401 authentication_required`
- Valid credential without read grant: `403 forbidden`
- World visibility is `sender_only` or `addressed`: `403 forbidden`
- Malformed or out-of-range `from_size`: `400 invalid_checkpoint_query`

Optional query parameter:

- `?from_size=M` (positive integer `1 <= M <= tree_size`): includes an RFC 9162 consistency proof from tree size `M` to `tree_size`.

Response (`200 OK`, `Content-Type: application/json`, `Cache-Control: no-store`):

```json
{
  "format": "c2sp.org/tlog-checkpoint",
  "world_id": "civ:earth-17",
  "origin": "agentciv/civ:earth-17",
  "tree_size": 3,
  "root_hash_sha256": "<64-char lowercase hex>",
  "checkpoint": "agentciv/civ:earth-17\n3\n<44-char standard base64>\n",
  "consistency_proof": ["<64-char lowercase hex>"]
}
```

`consistency_proof` is omitted when `from_size` is not requested.

## Reader verification

`tools/agentciv-reader` provides pure-Rust verification helpers in `agentciv_reader::checkpoint`:

- `parse_checkpoint(&str)` and `format_checkpoint(origin, tree_size, &root_hash)` for `c2sp.org/tlog-checkpoint` notes.
- `verify_consistency` / `verify_consistency_hex` returning structured `CheckpointError` values:
  - `CheckpointError::Rollback { cached_size, observed_size }` when `N < M`
  - `CheckpointError::Equivocation { tree_size }` when `N == M` and `root_N != root_M`
  - `CheckpointError::OriginMismatch` when checkpoint origins differ
  - `CheckpointError::InvalidProof` when an RFC 9162 consistency proof is missing, malformed, or fails root reconstruction
- `verify_inclusion` / `verify_inclusion_hex` for RFC 9162 inclusion proofs.

## References

- [Certificate Transparency Version 2.0, RFC 9162](https://www.rfc-editor.org/rfc/rfc9162)
- [C2SP Signed Checkpoints (`c2sp.org/tlog-checkpoint`)](https://c2sp.org/tlog-checkpoint)
- [Post-quantum BBS cryptography and transparency logs](POST_QUANTUM_BBS_AND_UNIVERSAL_TRANSLATOR_2026_11.md)
