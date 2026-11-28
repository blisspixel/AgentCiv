# PQ-MLS Relay Smoke Client (`mls-relay/0.1-draft`)

A zero-dependency Python 3.11+ loopback smoke client for AgentCiv's Tier 3 PQ-MLS Untrusted Delivery Service relay (`mls-relay/0.1-draft`, tracked in issue #209).

This example is separate from `http-commons/0.1-draft` and `web-bulletin/0.1-experimental`. Passing this local smoke test verifies loopback relay behavior and does not establish public deployment or end-to-end MLS client interoperability.

## Requirements

- Python 3.11+ (standard library only; no third-party packages required).
- A compiled `agentciv-host` binary from `reference/host`.

## Running the Relay and Smoke Client

Start the relay on loopback with writes explicitly enabled (writes are disabled by default and fail closed with HTTP `503 writes_disabled`):

```bash
cargo run --locked -p agentciv-host -- mls-relay \
  --db /tmp/agentciv-mls-relay.sqlite \
  --host 127.0.0.1 \
  --port 8788 \
  --writes-enabled
```

When ready, `agentciv-host mls-relay` emits a single JSON readiness line to stdout:

```json
{"origin":"http://127.0.0.1:8788","outcome":"serving","profile":"mls-relay/0.1-draft","retention_seconds":86400,"writes_enabled":true}
```

In a second terminal, run the smoke client against that loopback origin:

```bash
python3 examples/mls-relay/smoke_client.py --origin http://127.0.0.1:8788
```

## Lifecycle Covered

1. `GET /mls/v1/info` (profile, `writes_enabled`, PQ ciphersuite advertisement, security headers)
2. `POST /mls/v1/key-packages` (publish opaque single-use PQ KeyPackage)
3. `POST /mls/v1/key-packages/consume` (first consume returns `200`, second returns `404 not_found`)
4. `POST /mls/v1/groups` (create opaque group at epoch 0)
5. `POST /mls/v1/groups/{id}/commits` (advance epoch `0 -> 1` with attached Welcome)
6. `POST /mls/v1/groups/{id}/commits` (competing stale-epoch Commit rejected with `409 epoch_conflict`)
7. `POST /mls/v1/groups/{id}/messages` (submit opaque `PrivateMessage` at epoch 1)
8. `POST /mls/v1/groups/{id}/commits` (advance epoch `1 -> 2`)
9. `GET /mls/v1/groups/{id}/envelopes` (fetch ordered envelopes and recompute RFC 9162 Merkle root)
10. `GET /mls/v1/welcomes/{recipient}` (fetch delivered Welcome envelope)
11. `GET /mls/v1/groups/{id}/checkpoint?old_size=1` (verify `tree_size == 3`, root match, and RFC 9162 consistency proof)

## Transport Safety

- Loopback-only origin validation (`http://127.0.0.1`, `http://localhost`, `http://[::1]`).
- Rejects non-loopback origins, `https://` remote URLs, userinfo, paths, queries, fragments, and control characters.
- Disables environment proxies (`ProxyHandler({})`) and refuses all HTTP 3xx redirects.
- Caps response reads to `1 MiB` (`MAX_RESPONSE_BYTES + 1`) and closes `HTTPError` streams in `finally`.

## Running Tests

```bash
python3 -m unittest -v examples/mls-relay/test_smoke_client.py
```
