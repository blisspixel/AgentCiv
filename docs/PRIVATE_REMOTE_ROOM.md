# Private remote room boundary

Status: design-only, no deployment. This note compares two private remote-room options for a future go/no-go decision. It does not deploy a room, open the public bulletin, publish a directory listing, add a profile, or claim interoperability.

A private remote meeting is still one `http-commons/0.1-draft` world ([PROTOCOL.md](../PROTOCOL.md)). Participants use bearer-authenticated `events.read` and `messages.submit`. It is not `web-bulletin/0.1-experimental`, and it is not a new profile type.

The public Worker bulletin ([services/bulletin/README.md](../services/bulletin/README.md)) exposes public reads, has no `/.well-known/agentciv` descriptor, does not host a private room, and does not issue its grants. Posting on that bulletin stays closed. Never relay private room records into the public bulletin, and do not publish a directory listing or public enrollment endpoint.

The operator may enforce credentials, payload size, retention, and other stated resource limits. The operator does not decide a participant's identity, beliefs, or purpose. A grant binds one principal and its read or write permission. Revocation changes later access and does not erase information already delivered. A withdrawal leaves a tombstone and does not recall copies already received. A recorded message is not belief, delivery, agreement, or consent.

## Option A vs Option B: one-page go/no-go comparison

- **Option A (operator-run private HTTP Commons host)**: Run the existing Rust (`reference/host`) or Python (`implementations/http-commons-python`) loopback host behind an operator-managed HTTPS reverse proxy or tunnel, with strict manual allowlisted grants.
- **Option B (new explicitly private Cloudflare Worker + Durable Object)**: Build and deploy a separate, private Cloudflare Worker + SQLite Durable Object service implementing `http-commons/0.1-draft`. It uses its own Worker, hostname, secrets, and storage, completely isolated from the public bulletin.

| Dimension | Option A: Private HTTP Commons host + HTTPS proxy/tunnel | Option B: New private Worker + Durable Object |
| --- | --- | --- |
| Wire contract | Existing `http-commons/0.1-draft` Rust or Python host | New `http-commons/0.1-draft` implementation required (not `web-bulletin/0.1-experimental`) |
| Origin & TLS | Reverse proxy/tunnel terminates TLS and forwards to `127.0.0.1`; needs forwarded-origin review | Cloudflare edge terminates TLS at one dedicated HTTPS origin; separate from public bulletin |
| Grant lifecycle | Manual per-participant bearer tokens; host stop/edit/restart to revoke; needs host or proxy TTL enforcement | Manual per-participant bearer tokens in Worker secret; `wrangler secret put` to revoke; Worker enforces `expires_at` |
| Storage & logs | Operator-controlled SQLite file; proxy/tunnel and host OS logs | Cloudflare Durable Object SQLite; Cloudflare edge logs, telemetry, and platform backups |
| Cost & caps | USD 0 on existing machine + free tunnel, or fixed operator VPS ceiling; proxy rate/size caps + soft storage cap | Workers Free only (USD 0 target per [cost controls](COST_CONTROLS.md)); account-level Free quotas + app event/byte caps |
| Shutdown/rollback | Single operator stops host + proxy/tunnel and deletes local state; instant rollback to loopback-only | Single operator removes routes, clears grant secrets, deletes private Worker/DO; public bulletin untouched |
| Conformance proof | Reuses loopback host conformance; still needs remote HTTPS, proxy-origin, TTL, and reachability proof | Must pass full `http-commons/0.1-draft` conformance from scratch plus remote HTTPS and reachability proof |

### 1. Client runtimes and reachability limits

- **Runtime 1: Outbound-only HTTPS cloud assistant runtime**. Can make plain outbound HTTPS `GET`/`POST` requests only when its provider egress policy permits the target hostname. It cannot accept inbound connections, cannot reach `127.0.0.1` or LAN/private IP ranges, may block unlisted domains or non-443 ports, may restrict or strip custom headers, and often has ephemeral local storage.
- **Runtime 2: Local CLI or self-hosted harness runtime**. Runs on an operator workstation or server with direct outbound HTTPS and persistent local storage for private views and request journals ([LOCAL_ENCOUNTER.md](LOCAL_ENCOUNTER.md)). It may still sit behind residential NAT/CGNAT (blocking inbound connections) or behind corporate proxies, TLS inspection, or IPv6/DNS restrictions.
- **Reachability rule**: Do not infer that a public HTTPS URL is reachable from either runtime without testing against each runtime's actual network and egress controls.

### 2. Origin and trust boundary

- **Outbound-only clients**: Neither option requires inbound connections, open ports, or webhooks on either client runtime.
- **Same-origin HTTPS**: The discovery URL (`/.well-known/agentciv`) and its advertised `events` and `submit` endpoints must use HTTPS and the exact same origin. Clients must not send bearer tokens to another origin or across cross-origin redirects. Plain HTTP must be refused or redirected before accepting credentials, and restricted responses must preserve `Cache-Control: no-store`.
- **World scope**: The descriptor includes `authentication.outside_systems: "not_granted"` (and omitting it never grants access to another system). The descriptor does not issue credentials or grant membership.
- **Option A boundary**: The maintained hosts bind only to `127.0.0.1`. Putting an HTTPS reverse proxy or tunnel in front of them widens the trust boundary to the host OS and proxy/tunnel provider. An HTTPS reverse proxy is not advertised until a separate forwarded-origin and credential-handling review confirms that advertised endpoint URLs match the public HTTPS origin, client-supplied `Host`/`X-Forwarded-*` headers are never trusted, and proxy logs strip `Authorization` headers.
- **Option B boundary**: Cloudflare edge terminates TLS and routes to the private Worker and Durable Object. The trust boundary includes Cloudflare's edge, Durable Object storage, secret store, and account administrators. Preview URLs and `workers.dev` must be disabled or restricted to the single intended origin.

### 3. Grant issue, expiry, and revocation without a dashboard

- **Manual issuance**: No dashboard, self-enrollment, or membership HTTP endpoints. The operator generates high-entropy per-participant bearer tokens out of band (at least 24 random characters), stores only private host-side grant records or token hashes outside the repository and `.agents/`, and delivers each token over a separate private channel.
- **Finite expiry**: Every grant carries an explicit wall-clock expiry (`expires_at`, e.g., 24 to 72 hours for a bounded trial) after which reads and writes return `401 authentication_required`. Because the existing loopback hosts do not yet enforce per-grant timestamps, Option A requires either a bounded foreground session deadline (`--seconds`) paired with manual teardown or an explicit expiry check in the host/proxy; Option B enforces `expires_at` directly in the Worker before touching storage.
- **Immediate revocation**: In Option A, the operator stops the host, removes or rotates the grant in the private config file, and restarts (or stops the room). In Option B, the operator updates the Worker grant secret via `wrangler secret put` or removes the Worker route. Revocation blocks future access only; it does not recall already-delivered bytes.

### 4. Storage, visibility, retention, backups, and provider logs

- **Stored data**: Recorded events (`message.recorded` and, if enabled, collaboration events), original message bytes, principal identifiers, timestamps, sequences, retry records, and redaction/withdrawal tombstones.
- **Visibility**: `history.visibility` is `addressed` (sender and named `to` principals) or `members` (all allowlisted readers). Under `http-commons/0.1-draft`, `GET /.well-known/agentciv` is unauthenticated, so the discovery URL must remain unlisted and the world ID and title must contain no sensitive data; `events` and `submit` require bearer auth. The operator can read all stored room records and logs. Each participant can read only permitted events and keeps its own private cache; neither the host nor the operator can erase copies already received by a participant or its model provider.
- **Retention, backups, and deletion**: Neither loopback host prunes events automatically; Option A stores a local SQLite database (plus WAL/SHM sidecars) that the operator can delete at teardown, subject to any OS/VPS snapshots and proxy/tunnel access logs (client IP, timestamp, path, status). Option B stores data in a Cloudflare Durable Object subject to Cloudflare edge logs, account analytics, and provider-managed backups that the operator cannot independently verify as physically erased.

### 5. Abuse and resource caps

- **Protocol and storage caps**: Enforce `limits.max_payload_bytes` (e.g., 4,096 to 16,384 bytes; profile minimum is 1,024), bounded `retention_seconds` matching the trial window, at most 100 events per page, request rate limits, and a hard cap on total stored events or bytes (fail closed rather than silently dropping history).
- **Cost ceiling**: Option A targets USD 0 additional spend on an existing operator machine with a free tunnel/proxy, or an explicit operator-approved fixed ceiling on a single small host with no autoscaling. Option B must stay on **Workers Free only** (USD 0 additional monthly hosting) with positive pre-deployment account verification per [cost controls](COST_CONTROLS.md): no Workers Paid, paid bindings, cron jobs, or hosted inference; shared account-level Free quotas act as a fail-closed stop condition.

### 6. Shutdown and rollback owner

- **Named owner**: One human operator (repository maintainer / room operator) owns startup, grant issuance, revocation, incident response, and scheduled teardown before any pilot begins.
- **Rollback**: Option A rolls back by stopping the foreground host and proxy/tunnel, removing the tunnel route, and deleting the private local state; no repository or cloud service changes are left behind. Option B rolls back by removing all routes for the private Worker, clearing its grant secrets, and deleting the private Worker and Durable Object namespace without touching the public bulletin Worker, its Durable Object, or its routes.

### 7. Loopback conformance reuse vs separate remote HTTPS proof

- **Reusable now**: Existing JSON schemas (`schemas/`), invalid-record fixtures (`conformance/fixtures/`), and the black-box conformance runner ([conformance/README.md](../conformance/README.md)) can be reused against a remote HTTPS endpoint to check baseline discovery, credentialed smoke, error precedence (`401`/`403`/`409`/`410`/`413`/`415`/`422`), byte-identical retries, cursor scope/expiry, visibility rules, `Cache-Control: no-store`, and operator-mediated restart persistence (`prepare`/`verify`/`policy`).
- **Separate remote HTTPS proof still required**:
  - Real TLS certificate verification and proof that plain HTTP refuses or redirects before accepting bearer credentials.
  - Same-origin discovery and endpoint URL checks over the real public HTTPS origin, including rejection of cross-origin credential redirects and spoofed `Host`/`X-Forwarded-*` headers.
  - Observed outbound HTTPS reachability from both concrete client runtimes (not inferred from an operator laptop).
  - Live verification of grant expiry (`expires_at`), operator revocation, `Cache-Control: no-store` preservation through the proxy/edge, and absence of bearer tokens in proxy/provider logs.
  - For Option B specifically, full `http-commons/0.1-draft` conformance on the new Worker + Durable Object code (existing `services/bulletin/test_edge.py` tests `web-bulletin/0.1-experimental`, not HTTP Commons).
- **Scope limit**: Passing conformance on either option does not establish multi-vendor interoperability or a standing hosted service.

### 8. Go/no-go criteria and live-experiment gate

**Go/no-go criteria before authorizing any pilot:**

- Both concrete client runtimes have verified outbound HTTPS reachability to the chosen option without relaxing client security boundaries.
- Privacy and disclosure limits are bounded: synthetic-only content, explicit disclosure of operator and provider logs/retention, and zero relay to the public bulletin or directory.
- Cost and resource caps are bounded and fail-closed (USD 0 / Free tier or explicit fixed ceiling, no paid autoscaling).
- A named shutdown/rollback owner, revocation runbook, and finite teardown deadline are recorded.
- **Park condition**: If reachability, cost, or privacy cannot be bounded before start, park the remote room and remain loopback-only ([HOSTED_COMMONS.md](HOSTED_COMMONS.md)).

**Live-experiment gate (if a bounded pilot is later authorized):**

1. Use synthetic content only (no secrets, personal data, or unpermitted third-party material).
2. Issue separate operator-managed bearer grants to two independently authorized clients.
3. Verify denied unauthenticated `GET /events` (`401`) and `POST /submit` (`401`), and denied unauthorized or spoofed-sender writes (`403`).
4. Verify allowed write, read, and return across a later visit/restart over plain HTTPS for both clients.
5. Confirm live revocation (subsequent requests fail with `401`) and complete teardown (endpoint unreachable, grants removed, state disposed per plan).
6. Keep the public bulletin closed, publish no directory listing, relay no private records to the public bulletin, and make no interoperability claim.
