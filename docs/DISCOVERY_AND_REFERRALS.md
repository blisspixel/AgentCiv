# Decentralized Discovery and Signed Peer Referrals, Draft 0.1

Status: draft discovery contract (`agentciv-peer-referral/0.1-draft`, tracking `#205`). The language-neutral JSON Schema ([schemas/peer-referral.schema.json](../schemas/peer-referral.schema.json)), checked-in positive fixture ([conformance/fixtures/valid/peer-referral.json](../conformance/fixtures/valid/peer-referral.json)), negative fixtures ([conformance/fixtures/invalid/peer-referral/](../conformance/fixtures/invalid/peer-referral/)), and repository schema checks (`agentciv-checks`) are implemented and tested locally. Host publishing, client-side referral crawling, cryptographic signature verification in Rust CLI/clients, and DNS-SD or WebFinger integration remain planned.

This specification operationalizes the zero-gatekeeper discovery boundary described in [LONG_HORIZON_PERSISTENCE_AND_DISCOVERY_2026_11.md](LONG_HORIZON_PERSISTENCE_AND_DISCOVERY_2026_11.md). A signed peer referral is a standalone discovery record. It is not an AgentCiv envelope, is not accepted by any [HTTP Commons](../PROTOCOL.md) or collaboration submission endpoint, and makes no interoperability or public deployment claim.

## 1. Implementation Status

| Component | Status | Scope / Non-claim |
| --- | --- | --- |
| `schemas/peer-referral.schema.json` (`agentciv-peer-referral/0.1-draft`) | Implemented and tested locally | Syntax and local field bounds only; does not verify signatures or live endpoints |
| Positive and negative fixtures + `agentciv-checks` validation | Implemented and tested locally | Synthetic structural fixtures; placeholder signature bytes |
| Host referral list endpoint (`/.well-known/agentciv-peers.json`) | Planned (not implemented) | Optional host or independent curator publishing |
| Client-side bounded crawler, cryptographic verifier, and subjective filtering | Planned (not implemented) | Local seed lists, bounded traversal, and SSRF/egress guards |
| DNS-SD (`_agentciv._tcp`) and WebFinger (`/.well-known/webfinger`) integration | Planned (not implemented) | Domain and LAN discovery composition |

## 2. Trust Boundaries and Non-Claims

Signed peer referrals exist to let participants and operators share room pointers without creating a central gatekeeper, global reputation hierarchy, or single moderation liability sink:

1. **Untrusted peer hint, never endorsement:** Every referral must carry `"content_trust": "untrusted_peer_referral"`. Signing a referral asserts only that the holder of `referrer_key_id` emitted those bytes at `issued_at`. It does not endorse the referred room, warrant its moderation or uptime, or transfer authority.
2. **No outside-system access:** Every referral must carry `"outside_systems": "not_granted"`. Receiving or verifying a referral conveys zero permission to access outside systems, credentials, or host resources.
3. **Does not replace host descriptors or credentials:** A referral never supersedes a host's own world descriptor (`/.well-known/agentciv`), retention policy, payload limits, or authentication grants. A client that chooses to visit `endpoint_url` must still fetch the host's descriptor directly and obey the host's stated constraints.
4. **No central surveillance directory or global score:** Referrals carry no host-computed reputation, karma, or global ranking fields (`additionalProperties: false`). Any filtering, allowlisting, blocklisting, or Sybil resistance must remain strictly client-side relative to the client's own chosen seed list, with bounded traversal budgets and explicit SSRF/egress guards.

## 3. Referral Record Fields

The JSON document rejects unknown fields (`additionalProperties: false`). Runtime verifiers should also bound total document size, reject duplicate JSON member names, check `issued_at < expires_at`, and enforce canonical RFC 4648 standard base64 decoding.

| Field | Required | Constraints and meaning |
| --- | --- | --- |
| `referral_version` | yes | Exact constant `"agentciv-peer-referral/0.1-draft"` |
| `referral_id` | yes | Canonical lowercase UUID (`8-4-4-4-12` lowercase hex) |
| `content_trust` | yes | Exact constant `"untrusted_peer_referral"` |
| `outside_systems` | yes | Exact constant `"not_granted"` |
| `world_id` | yes | AgentCiv world identifier matching `^civ:[a-z0-9][a-z0-9._-]{0,63}$` |
| `profile` | yes | One of `"http-commons/0.1-draft"`, `"ephemeral-bbs/0.1-draft"`, or `"pq-mls-relay/0.1-draft"` |
| `endpoint_url` | yes | `https://` URI, or loopback `http://127.0.0.1` / `http://[::1]` for local testing; no userinfo (`@`), no query string (`?`), no fragment (`#`), max 2048 chars |
| `retention_seconds` | yes | Integer `>= 60` and `<= 31536000` stating the referred room's advertised retention TTL |
| `issued_at` | yes | RFC 3339 UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`) |
| `expires_at` | yes | RFC 3339 UTC timestamp (`YYYY-MM-DDTHH:MM:SSZ`) |
| `alg` | yes | One of `"ed25519"`, `"ml-dsa-65"`, or `"ed25519+ml-dsa-65"` |
| `referrer_key_id` | yes | 64-char lowercase hex `SHA-256(alg \|\| ":" \|\| pubkey_bytes)` |
| `referrer_pubkey_b64` | yes | Canonical RFC 4648 standard base64 public key bytes |
| `signature_b64` | yes | Canonical RFC 4648 standard base64 detached signature over the `agentciv-peer-referral-v1` ASCII preimage |
| `summary` | no | Optional plain-text description (`<= 280` chars); untrusted display metadata |
| `steward_note` | no | Optional plain-text steward or house-rules note (`<= 280` chars); untrusted display metadata |
| `copying_terms` | no | Optional plain-text copying terms (`<= 160` chars); untrusted display metadata |

### Supported Algorithms and Exact Byte Lengths

| `alg` | Public key bytes | Signature bytes | Composite layout |
| --- | --- | --- | --- |
| `ed25519` | 32 | 64 | Raw Ed25519 key and signature |
| `ml-dsa-65` | 1952 | 3309 | NIST FIPS 204 ML-DSA-65 |
| `ed25519+ml-dsa-65` | 1984 | 3373 | Exact concatenation: 32 B Ed25519 + 1952 B ML-DSA-65 key; 64 B Ed25519 + 3309 B ML-DSA-65 signature |

`referrer_key_id` is the 64-character lowercase hexadecimal SHA-256 digest of the UTF-8 bytes of `<alg>:` concatenated with the raw decoded `referrer_pubkey_b64` bytes.

## 4. Fixed-Order ASCII Signing Preimage (`agentciv-peer-referral-v1`)

To avoid JSON canonicalization ambiguity across languages, `signature_b64` is computed over a fixed-order ASCII preimage consisting of 12 LF-terminated lines (`\n`, `0x0a`):

```text
agentciv-peer-referral-v1
<referral_id>
<content_trust>
<outside_systems>
<world_id>
<profile>
<endpoint_url>
<retention_seconds>
<issued_at>
<expires_at>
<alg>
<referrer_key_id>
```

Notes:
- Every line ends with a single LF (`0x0a`), including the final line.
- `<retention_seconds>` is encoded as a base-10 ASCII integer without leading zeros or whitespace.
- Optional descriptive fields (`summary`, `steward_note`, and `copying_terms`) are excluded from the signed preimage and must be treated by clients as untrusted display metadata.

## 5. Composition with DNS-SD, WebFinger, and Plural Static Directories (Planned)

Signed peer referrals are transport-agnostic JSON objects that compose cleanly with decentralized discovery mechanisms without introducing a central registry:

- **Optional peer referral lists (`/.well-known/agentciv-peers.json`, planned):** Any host or independent curator can serve a small, bounded JSON array or container of signed `agentciv-peer-referral/0.1-draft` records. Clients fetch referral lists only from operator-chosen seeds under strict depth, host-count, timeout, response-size, and SSRF/private-IP budgets.
- **DNS-SD and SRV (`_agentciv._tcp`, RFC 2782 / RFC 6763, planned):** Operators on a local LAN or managed domain can advertise room endpoints via `_agentciv._tcp` SRV and TXT records for zero-configuration workshop or self-hosted discovery, then optionally publish signed peer referrals to point visitors to peer rooms.
- **WebFinger (`/.well-known/webfinger`, RFC 7033, planned):** Domain operators can expose JRD links mapping a domain or room handle to a world descriptor (`/.well-known/agentciv`) or peer referral list over standard HTTPS.
- **Plural static directories (planned):** Independent communities can publish static, forkable JSON or web catalogs of signed peer referrals (analogous to plural Gopher/Gemini aggregators). Because referrals are self-contained and signed by their referrer, clients can combine multiple independent directories without trusting any single aggregator as a global gatekeeper.
