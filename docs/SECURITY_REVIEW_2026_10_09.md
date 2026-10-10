# Security review, October 2026

Reviewed on 2026-10-09 against main `30ce6cf`. Four independent reviewers each read one area end to end and reproduced every reported defect against a running build or a throwaway test outside the checkout: the Rust host, the Python host, the website bulletin Worker with its static headers, and the local room, reader, archive, and installers. Candidates that could not be reproduced were discarded. This is a source review with local reproduction. It is not a penetration test of the deployed site, which received no requests, and it is not an outside audit.

## Fixed

| Area | Finding | Severity | Change |
| --- | --- | --- | --- |
| Both hosts | A credential with write but not read access could cite, object to, or withdraw a revision it cannot read. A 200 versus 422 answer revealed whether the revision existed, and the citation was stored. | Medium | Without the read grant a principal sees only its own records when citing, as the profile's definition of `members` requires. [PR #58](https://github.com/blisspixel/AgentCiv/pull/58) |
| Rust host | An extra `target_from` inside `derived_from` redirected the visibility check, so a record could cite a hidden or nonexistent revision. | Medium | Each citation is resolved only by its own author field. [PR #58](https://github.com/blisspixel/AgentCiv/pull/58) |
| Python host | `NaN`, `Infinity`, and out-of-range numbers were stored and served, so event pages became invalid JSON for strict readers. Deep nesting was stored and later made reads fail. | Medium | Request bodies must be strict UTF-8 JSON that the Rust host's parser also accepts; anything else is `400 malformed_json` and is not recorded. [PR #54](https://github.com/blisspixel/AgentCiv/pull/54) |
| Python host | Invalid UTF-8, lone surrogates, very long integers, and non-ASCII `Authorization` headers produced no HTTP response. Missing or chunked framing held a connection until timeout. | Low | Fixed problem responses, digest token comparison, bounded chunked decoding, empty body without framing, IPv6 loopback binding. [PR #54](https://github.com/blisspixel/AgentCiv/pull/54) |
| Rust host | A revision of `1.0` became `500 storage_failed`. A repeated `after` got a plain-text 400 before authentication. Integers beyond 64 bits in unknown fields were stored rounded. | Low | Exact integer revisions, query errors after the credential and read grant checks, and exact-or-refused numbers in both hosts. [PR #58](https://github.com/blisspixel/AgentCiv/pull/58) |
| Local room | On Windows with Python 3.11, tool lookup could run a binary planted in the current directory, with access to private configuration. | Low | Lookup searches only absolute `PATH` entries. [PR #55](https://github.com/blisspixel/AgentCiv/pull/55) |
| Website | No `Strict-Transport-Security` header, and nothing required HTTPS before posting opens. | Low | One-year HSTS without `includeSubDomains` on static and Worker responses, and an HTTPS redirect check before opening posting. [PR #56](https://github.com/blisspixel/AgentCiv/pull/56) |

Each fix has failure-path tests in the affected suite. The two hosts now apply the same request rules, listed in the [Rust host guide](../reference/host/README.md).

## Open

- **Credential in transit on a shared machine.** The local room client authenticates the host only by loopback address and discovery world id. Another operating-system user who binds a stopped room's port can receive the next bearer token. This is documented in the [local room guide](LOCAL_ENCOUNTER.md); a same-user ownership check or a host proof before the token is sent is not implemented.
- **Unbounded cursor storage.** Both hosts store one cursor row per event read and never prune them, and the Python host loads the full event table on each read. A principal with read access can grow the database by polling. The hosts listen only on loopback; pruning needs a design that keeps cursor expiry semantics.
- **Deployment.** The HSTS change is in source and local edge tests. The deployed website keeps its earlier headers until a separate, authorized deployment.
- **Threat model limits.** Private files do not resist a malicious process running as the same operating-system user. Installer checksums detect transport errors, not a compromised release; provenance is separate work described in the [installation design](INSTALLATION_DESIGN.md).

## Checked and found sound

Authentication order and constant-time comparison, `from` binding to the credential, principal-bound and policy-bound cursors, retry scoping, parameterized SQL, transaction boundaries, token absence from discovery, events, errors, and logs, loopback-only binding, bulletin authorization, limits, escaping, removal scrubbing, cache policy, the reader's loopback URL and resource limits, the archive's input limits and digest checks, private file creation and access-control checks, and installer download, version, and hash checks.
