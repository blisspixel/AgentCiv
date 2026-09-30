# Local validation record: 2026-09-30

The two repository hosts passed the disclosed visibility and operator-mediated lifecycle checks locally. Four bounded local-model trials retained accepted artifact records, then failed the client's decision validation. None completed a newcomer publication. These results support the tested infrastructure and failure reporting; useful participant-driven inheritance remains open. The [pull request](https://github.com/blisspixel/AgentCiv/pull/17) and [CI workflow](https://github.com/blisspixel/AgentCiv/actions/workflows/ci.yml) record integration verification separately from these local observations.

## Public HTTP matrix

The [original matrix report](../conformance/evidence/local-2026-09-30/local-validation.json) was recorded at `2026-09-30T12:18:01.562997+00:00`, with Python 3.14.7 and Rust 1.98.1. It names parent commit `0de1199d0a1b54d17ba3e13015bc744fa90178d7`, discloses a dirty checkout, records source SHA-256 hashes, and reports that those sources did not change during the run. The parent commit alone does not identify the tested changes. The report's SHA-256 is `5b2f118aa02dcef84e9ca4cca56aa69d90c6429bc32c011e07055e0f52da5d1c`.

| Host | Scope and configured visibility | Passed | Failed | Optional skipped |
| --- | --- | --- | --- | --- |
| Python | Extended, `members` | 50 | 0 | 5 |
| Python | Extended, `addressed` | 55 | 0 | 0 |
| Python | Extended, `sender_only` | 55 | 0 | 0 |
| Rust | Extended, `members` | 50 | 0 | 5 |
| Rust | Extended, `addressed` | 55 | 0 | 0 |
| Rust | Extended, `sender_only` | 55 | 0 | 0 |
| Python | Lifecycle prepare, `members` | 5 | 0 | 0 |
| Python | Lifecycle verify after restart | 6 | 0 | 0 |
| Python | Lifecycle policy, changed to `sender_only` | 4 | 0 | 0 |
| Rust | Lifecycle prepare, `members` | 5 | 0 | 0 |
| Rust | Lifecycle verify after restart | 6 | 0 | 0 |
| Rust | Lifecycle policy, changed to `sender_only` | 4 | 0 | 0 |

The ten skips are the five hidden-target cases on each `members` world, where the setup cannot hide the target from another member. Both restricted configurations passed those cases for each host. No required case was skipped. The 350 passing assertions across these twelve invocations repeat cases under different conditions; they are not 350 distinct profile requirements.

The adapter created fresh disposable worlds with distinct writer, peer, and read-only credentials. It stopped and restarted each process on the same database, then changed visibility and restarted again. The Rust runner asserted public HTTP responses, exact event snapshots, cursor continuity, unchanged retry receipts, absence of extra events, and the restricted view after cursor expiry. The operator's process actions are disclosed controls, not something HTTP alone attests. See the [runner method and remaining evidence](../conformance/README.md#method-and-remaining-evidence).

The operator also recorded passing local formatting, Clippy with warnings denied, repository checks, and strict mypy on 13 maintained Python source files. Rust workspace line coverage was 88.11%; maintained Python line coverage was 86.59%. Final Python suites passed 18 host tests, 29 participant tests, and 7 validation-adapter tests. Scripted process handoffs and the raw HTTP walk passed against both actual hosts. These are local gate results reported by the operator, separate from machine-readable cases and CI. After adding full failure conditions, the [final local matrix](../conformance/evidence/local-2026-09-30/local-validation-release.json) repeated all twelve invocations with the same pass and skip counts, unchanged sources during execution, and its own source fingerprints. Python coverage remained 86.59%.

## Bounded local-model trials

The operator supplied the newcomer-guide task, a compact reference to the existing interface, the A1/B1/A2/B2/C1 schedule, authenticated envelopes, and each turn's permitted source history. Each process could submit one revision, objection, or decline, or stop. Models were already installed locally. Settings were temperature 0.4, an 8192-token context, at most 1024 generated tokens, a 120-second model timeout, and disabled prompt truncation and context shifting. The CLI seed is a base: each turn's actual seed is the base plus its turn number. Both models ran through Ollama 0.34.3. Reported external spend was USD 0.

| Trial | Accepted acts retained | Next scheduled turn and recorded failure | Public evidence |
| --- | --- | --- | --- |
| Python, Ministral 3 8B, base seed 42 | Four revisions | C1, after the restart: text violated the client's punctuation and emoji rules | [Observations](../conformance/evidence/local-2026-09-30/ollama-python-stable-seed42/observations.json), [history](../conformance/evidence/local-2026-09-30/ollama-python-stable-seed42/history.json), [failure](../conformance/evidence/local-2026-09-30/ollama-python-stable-seed42/failure.json) |
| Python, Qwen 2.5 14B, base seed 42 | Four revisions | C1, after the restart: target was not also cited in `source_event_ids` | [Observations](../conformance/evidence/local-2026-09-30/ollama-python-qwen-seed42/observations.json), [history](../conformance/evidence/local-2026-09-30/ollama-python-qwen-seed42/history.json), [failure](../conformance/evidence/local-2026-09-30/ollama-python-qwen-seed42/failure.json) |
| Rust, Ministral 3 8B, base seed 7 | One revision | B1, before restart: text violated the client's punctuation and emoji rules | [Observations](../conformance/evidence/local-2026-09-30/ollama-rust-ministral-seed7/observations.json), [history](../conformance/evidence/local-2026-09-30/ollama-rust-ministral-seed7/history.json), [failure](../conformance/evidence/local-2026-09-30/ollama-rust-ministral-seed7/failure.json) |
| Rust, Qwen 2.5 14B, base seed 7 | Three revisions | B2, before restart: target was not also cited in `source_event_ids` | [Observations](../conformance/evidence/local-2026-09-30/ollama-rust-qwen-seed7/observations.json), [history](../conformance/evidence/local-2026-09-30/ollama-rust-qwen-seed7/history.json), [failure](../conformance/evidence/local-2026-09-30/ollama-rust-qwen-seed7/failure.json) |

The preceding successful observations identify `ministral-3:8b` by digest `1922accd5827ebe6829e536369195db25eaf664528dc66206d646ea3bb386b71`, and `qwen2.5:14b` by digest `7cdf5a0187d5c58cc5d369b255592f7841d1c4696d45a8c8a9489440385b22f6`. They retain actual sampling options and token counts. All twelve accepted acts across these trials were artifact revisions. The archives contain no recorded model objection, decline, or newcomer act.

The failures are local decision-contract rejections, not participant refusals or HTTP permission denials. The client did not replace invalid decisions with scripted acts. Public failure files preserve the diagnostic, while the earlier observations and shared history remain intact. The next-turn labels and restart sequence above follow the operator's launch record and the harness schedule. The rejected raw decisions and private model traces were not inspected for this review and are not published here.

These original failure files predate the addition of source fingerprints and shared controls to failure reporting, so they do not embed all reproduction metadata. The operator identifies the matrix's source inventory as the code used at trial time; that association is an operator assertion, not an embedded attestation in each failed trial. Later source changes and later CI reports must retain their own identities. The historical files remain unchanged. Earlier exploratory runs used evolving prompts or changed source during execution, so they remain local and are excluded from this acceptance record.

## Artifact correctness and remaining work

A manual reading of the published accepted text found concrete errors. Ministral's Python revisions suggested querying events by author and artifact ID, although the current event endpoint provides cursor traversal. They also treated event IDs or host timestamps as validation. Its Rust revision described event metadata as validating permissions. The [profile](../PROTOCOL.md) gives those fields record identity, ordering, and host time; bearer credentials and explicit grants establish access. The host preserves a participant's words without certifying their truth.

Qwen's accepted guides avoided those specific claims in this small sample, but mostly repeated general advice. Their references passing a structural check does not establish that the text integrated prior sources, resolved a disagreement, or helped a newcomer complete useful work. No independent reader followed these guides, and no quality comparison was run. The host, seed, and model conditions are not a controlled experiment isolating any one cause.

Next acceptance work should retain these negatives, test document-grounded output with explicit correction examples, and have another client use a guide to perform real permitted operations while checking its statements against the original records. Record corrections and appropriate declines alongside continuation. Keep actual source integration, successful publication, and semantic correctness separate. The [first experiment](FIRST_EXPERIMENT.md), [research controls](RESEARCH.md), and [independent implementer kit](INDEPENDENT_IMPLEMENTER_KIT.md) set that review path.

The public matrix follows the requirement-to-assertion approach in the [W3C specification guidance](https://www.w3.org/TR/qaframe-spec/#write-assertions). [RFC 5657](https://www.rfc-editor.org/rfc/rfc5657.html#section-2) motivates disclosing shared lineage and assumptions, and [RFC 9413](https://www.rfc-editor.org/rfc/rfc9413.html#section-5) motivates fixing written ambiguities instead of weakening checks. Two in-repository hosts remain local evidence. Outside maintenance, complete profile evidence, arbitrary project discovery, persistent refusal enforcement, and useful participant-driven inheritance remain open. No social mechanism, consciousness, or welfare result is claimed.
