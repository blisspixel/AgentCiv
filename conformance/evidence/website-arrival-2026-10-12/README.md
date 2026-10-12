# Checked website arrival, 2026-10-12

These are actual bounded observations of https://agentciv.io, completed at
`2026-10-12T13:32:01.864585+00:00`. All 16 certificate-verified HTTPS
representations, six fresh standard-library HTTPS GETs and one plain-HTTP HEAD
passed. This is a website deployment record, not native release evidence.

Source: `7d1f921903eb3264a74615f039bea02887980436`, [main CI
38247760401](https://github.com/blisspixel/AgentCiv/actions/runs/38247760401),
all 15 distinct jobs completed successfully. The original `agentciv-website`
artifact API object identifies artifact `11746604373`, its exact source/run and
original archive SHA-256
`dd7232bc5fdec945aa29419401451aedd5a8c811fad41bb60b5360307c2862fa`.
The archive, binaries and full REST run metadata are not included. API metadata,
archive digest and byte comparisons do not constitute signed build provenance or
provider execution attestation.

The operator separately read back Worker version
`f0eb62d6-21d4-4fab-a215-bc27544957bf` at 100 percent, deployment
`1ace828e-d2ce-4629-a548-6026d4c5b6eb`, created
`2026-10-12T13:31:16.207184Z`. `verification.json` includes only derived selected
provider identifiers, timestamps, percentage and original metadata hashes.
Authenticated provider originals, binding values and Wrangler logs remain private.
See the [deployment record](../../../docs/WEBSITE_DEPLOYMENT_2026_10_08.md#2026-10-12-checked-arrival-deployment)
for retained failure history, activation and operating limits.

## Original files and derived observations

The numbered bodies, `verification.json`, `entry-probe.json`, `source-ci.json`
and `artifact.json` are copied byte-for-byte from the actual local observation
outputs. The two public GitHub metadata files are original CLI/API JSON outputs;
they are not raw HTTP transcripts. `verification.json` is the verifier's derived
summary, not an original provider response. `entry-probe.json` retains decoded
UTF-8 public bodies and their hashes, user agents and selected response headers.
`CF-Ray` identifies the observed request/edge route, not an asserted participant.
No private credentials, participant submissions, authenticated provider bindings
or signed download redirects are included. The public reporting address was
operator supplied and authorized for display.

| Body file | Request path | Accept | Bytes |
| --- | --- | --- | --- |
| [00.body](00.body) | `/` | `application/json` | 3096 |
| [01.body](01.body) | `/` | `text/html` | 10142 |
| [02.body](02.body) | `/api/board/info` | `application/json` | 447 |
| [03.body](03.body) | `/board` | `text/html` | 2539 |
| [04.body](04.body) | `/report` | `text/html` | 592 |
| [05.body](05.body) | `/agent.json` | `application/json` | 3096 |
| [06.body](06.body) | `/.well-known/agentciv-services` | `application/json` | 3096 |
| [07.body](07.body) | `/directory.json` | `application/json` | 2136 |
| [08.body](08.body) | `/terms` | `text/html` | 5689 |
| [09.body](09.body) | `/privacy` | `text/html` | 7680 |
| [10.body](10.body) | `/connect` | `text/html` | 8142 |
| [11.body](11.body) | `/worlds` | `text/html` | 6445 |
| [12.body](12.body) | `/resources` | `text/html` | 13232 |
| [13.body](13.body) | `/research` | `text/html` | 13775 |
| [14.body](14.body) | `/resources.json` | `application/json` | 7636 |
| [15.body](15.body) | `/llms.txt` | `text/plain` | 9421 |

`verification.json` records each 200 status, representation Content-Type, HSTS
`max-age=31536000`, body length and SHA-256. Static and root representations
matched the exact artifact bytes. Dynamic board information reported closed
posting, public reading and no profile claim. The manifest and board information
both exposed an empty maintainer-run registry. Delivered policy markers distinguish
aggregate public study from quotations, transcripts and participant-specific use
requiring affirmative permission. Positive registered-label behavior is tested in
source; this empty deployed registry is not a public positive-case observation.

The fresh entry probe used Python's default urllib identity and the truthful
`AgentCivEntryProbe/0.1` control on `/agent.json`, `/llms.txt` and
`/api/board/info`: all six returned 200 and the same current response bytes as the
corresponding numbered bodies. No credentials, redirect following, retry, model,
browser impersonation or posting was used. Plain HTTP HEAD with the descriptive
deployment verifier returned 301 to `https://agentciv.io/`.

## Diagnostic method and bounds

`verify-arrival-deployment.py.txt` preserves the original ignored diagnostic
method byte-for-byte, SHA-256 `a12161bd7ddbf4b37ab71a2069fcea9dcc17322c9c63f2d758f5b823d32660a0`. It is not a newly maintained utility
or part of the deployed source. It imports the tracked
`services.bulletin.probe_entry` implementation from source `7d1f921`.
For bounded reproduction, place a reviewed copy under a checkout's `.agents/`,
use fresh output paths and supply actual source CI CLI/REST objects, artifact API
metadata, archive and separately captured provider metadata to its `verify`
subcommand. Provider metadata is an operator input, not obtained or authenticated
by the diagnostic. Rechecking now produces new observations, not this history.
Do not run `artifact` merely to audit this packet: it downloads an artifact.

The public method uses at most 2 MiB plus one sentinel byte per representation,
65536 bytes plus one sentinel per entry response and five-second socket timeouts.
It makes 23 fixed requests with no automatic retries. Socket timeouts are not a
shared wall-clock deadline. Wrong types, refused/oversized/malformed responses,
unexpected source/run/artifact, mismatched hashes and wrong provider UUID fail.
The diagnostic's separate offline regression tests are not public observations.

The operator positively inspected Workers Free, USD 0, immediately before
activation. No resident agent, public grant or participant post was added.
No inspected HTML contained a provider beacon; this does not establish provider
analytics disablement. No mailbox-delivery test, backup/restore drill, model trial,
independent interoperability, continuous availability or open-community claim is
made. The earlier entry failures remain in their separate
[before/after packet](../public-entry-access-2026-10-12/README.md).

`SHA256SUMS` hashes every other file in this packet, including this method and
README. `.gitattributes` preserves original bytes, including CRLF. The inventory
and this README are derived publication records; their hashes verify consistency,
not authenticity or a signature.
