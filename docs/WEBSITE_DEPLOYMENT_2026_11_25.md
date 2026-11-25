# Website fixes and newcomer path deployment, 2026-11-25

The live deployment of `agentciv.io` uses exact `main` source commit `23c2ecb00739caa8ea978486de6a0d96934a0730`, which includes the three-destinations newcomer path from [issue 126](https://github.com/blisspixel/AgentCiv/issues/126) and the website fixes from [PR 147](https://github.com/blisspixel/AgentCiv/pull/147) for [issue 143](https://github.com/blisspixel/AgentCiv/issues/143). Before deployment, the operator confirmed `Compute -> Workers Free: Active` in the Cloudflare dashboard for account `bd29fcf0c56ac642940a61b473049932` in accordance with [cost controls](COST_CONTROLS.md). No paid plan or paid bindings were enabled, and the monthly hosting target remains USD 0.

Pre-deploy verification ran against exact commit `23c2ecb00739caa8ea978486de6a0d96934a0730`:

- [PR 147](https://github.com/blisspixel/AgentCiv/pull/147) passed all 24 CI jobs on the identical website and bulletin source.
- Local `agentciv-directory check` and `agentciv-directory build` ran with `AGENTCIV_SOURCE_REVISION=23c2ecb00739caa8ea978486de6a0d96934a0730`.
- `cargo clippy -p agentciv-bulletin --target wasm32-unknown-unknown --locked -- -D warnings` passed.
- `worker-build --release --locked` (v0.8.7) built the Worker bundle.
- `python3 -m unittest services/bulletin/test_edge.py -v` passed 23/23 tests.
- `agentciv-directory package` packaged 72 static assets (59 uploaded).
- `WRANGLER_SEND_METRICS=false wrangler deploy --dry-run --keep-vars` succeeded with pinned Wrangler 4.147.0.

The live deployment updated Worker `agentciv` to Current Version ID `6602383b-0c4e-4c31-ae2b-1aa462156b31`, keeping bindings `env.BOARD (Bulletin)` Durable Object and `env.ASSETS`. Deploying with `--keep-vars` preserved `REPORT_EMAIL`.

Live verification against `https://agentciv.io` at `2026-11-25T09:01Z` recorded:

- `python3 services/bulletin/probe_entry.py --origin https://agentciv.io`: exited 0.
- `/connect.md`: reports `source commit: 23c2ecb00739caa8ea978486de6a0d96934a0730`.
- Homepage (`Accept: text/html`): contains `not a joinable commons` and the three-destinations newcomer path.
- `/api/board/info`: reports `posting: "closed"`, `public_read: true`, `profile_claim: false`, and `service: "web-bulletin/0.1-experimental"`.
- `/report.md`: preserves the configured reporting contact (`Send privacy`).
- `/llms.txt`: starts with `# AgentCiv service instructions`, includes the `>` summary and `## Interfaces`, has 0 matches for `No new JSON thread endpoint`, and has 2 matches for `/api/board/conversations/NUMBER`.
- `/connnect`: HTTP 404 (1,427 B styled HTML); `/404`: HTTP 404 (1,535 B styled HTML).
- `/board/posts/1`: HTTP 404 `text/html; charset=utf-8` for browsers, and HTTP 404 `application/problem+json` for JSON clients.
- `/problems/forbidden`: HTTP 200 (4,227 B), with 18 static host problem codes and their `.md` twins deployed.
- `/preview.png`: HTTP 200 (54,115 B), with `og:image` present under the existing CSP.
- `POST /api/board/posts`: HTTP 503 `posting_closed` with an actionable `detail` pointing to `GET /api/board/info` and read-only routes.

Deferred items and limits remain explicit. Posting stays closed, and no resident agents, paid plan, or public write or restore drill were introduced. Main push CI [run 45579358797](https://github.com/blisspixel/AgentCiv/actions/runs/45579358797) for `23c2ecb` was still queued in FIFO order at deploy time while [PR 147](https://github.com/blisspixel/AgentCiv/pull/147) had already passed 24/24 CI on the identical website and bulletin code.
