# Checked conformance evidence inventory

Status: implemented repository traceability check. The [machine-readable inventory](../conformance/requirements.json) accounts for 81 source blocks from [HTTP Commons](../PROTOCOL.md), the [collaboration extension](COLLABORATION_PROFILE.md), and optional [prefix validation](PREFIX_VIEW.md). It links exact source quotations to schema fixtures, named public runner cases, named implementation tests, retained observations, and explicit gaps. It does not complete these contracts' conformance evidence or establish independent interoperability.

The written contracts and schemas remain authoritative. Quotations in the inventory are references to those documents, not another contract. Do not implement against the inventory instead of reading the sources. When a source block changes, review its obligations and evidence before updating its quotation.

## What the checker establishes

Run the existing repository gate:

```sh
cargo run --locked -p agentciv-checks
```

The [Rust checker](../tools/agentciv-checks/src/inventory.rs) reads at most 256 KiB of inventory and 1 MiB per referenced file. It rejects unknown inventory fields, malformed inputs, paths outside the repository, missing files, stale anchors, duplicate identifiers, repeated source blocks, unaccounted source blocks, missing applicability, evidence-category mismatches, and gaps without an owner and proposed test. Implementation-test references must name test functions in actual test files. Public-case references name the runner's case identifier.

The source scan includes prose paragraphs, tables, and individual list items under the contract sections. It excludes fenced examples, introductory status prose, references, and HTTP Commons' test-setup commentary. It includes lowercase behavioral requirements, such as assigned revision numbers and withdrawal updates. It accounts for 32 HTTP Commons blocks, 41 collaboration blocks, and 8 prefix blocks. A block can contain several obligations. Accounting for a block does not establish that every obligation in it has a test.

`required` labels mandatory HTTP Commons behavior. `conditional` labels behavior required when `collaboration.submit` is advertised. `optional` labels the author's choice to include a continuity note. A block with required and optional language keeps both meanings from the source: its mandatory portions do not make an optional capability or author act mandatory. The inventory's applicability text and the written source determine the condition. Recommendations and informational text do not become mandatory through an evidence link.

Coverage categories are deliberately narrow:

| Category | Meaning |
| --- | --- |
| `partial_public` | At least one named public runner case is linked. Review the source block and case assertions to determine which obligations it actually checks. |
| `implementation_only` | Named implementation tests are linked, with no linked public case for this block. This does not supply black-box coverage. |
| `gap` | The block has an explicit owner and proposed test, without claiming existing executable evidence. |
| `informational` | Example guidance or scope explanation, rather than a host requirement. |

The checker verifies references and the inventory's structural consistency. It cannot decide whether a test proves its claimed behavior, whether an operator performed a recorded intervention, whether a quoted source is correct, or whether two implementations are independently maintained. `profile_complete` remains false. No percentage of inventory rows is a conformance score.

## Retained observations and their limits

The inventory links 489 selected case observations: the original 321 in the retained [local visibility and lifecycle matrix](../conformance/evidence/local-2026-09-30/local-validation-reviewed.json), 20 in the [sequential revision and restart matrix](../conformance/evidence/revisions-2026-10-08/local-validation.json), 20 in the [concurrent revision and restart matrix](../conformance/evidence/concurrent-revisions-2026-10-08/local-validation.json), 18 same-ID race observations, 24 prefix observations, and 86 acceptance observations described below. The sequential matrix names clean source commit `14498ac309cad839d14ed81f5824e5774da9dc2c`; the concurrent matrix names clean source commit `47a4fbd34b8d692e79a8a0e0c594e18244768865`. Both report unchanged source during their runs. Each of those two matrices contributes three new required cases in six Rust/Python visibility conditions plus its named restart case in both lifecycle verification runs. All matrices disclose source hashes, working-tree state, tool versions, time, implementation, visibility, and operator actions. Earlier observations remain unchanged. These observations describe the recorded source versions. A current passing checker verifies that the retained states were quoted accurately; it does not rerun those hosts or promote old reports into current-source evidence.

The retained distinct-request race and restart cases resolve the inventory's specific revision-increment gap. The [same-ID matrix](../conformance/evidence/same-id-2026-10-11/local-validation.json) adds 18 required passing observations from clean source `1eead6cd09a8158d813d04a782b9ce2bbf24e02a`, unchanged during the run. It retains both original requests and credential-safe responses for simultaneous identical and conflicting collaboration submissions, the winning event, an exact post-race retry, and the subsequent revision 2 under all six host/visibility conditions. The subsequent acceptance matrix retains these actual same-ID outcomes through killed-process restart. Distinct-request lifecycle races remain separate evidence. The later public retention, access, interruption, and cross-endpoint cases below resolve bounded delivery work while preserving narrower remaining gaps.

Each observation points to a real report case array and identifies a case by its exact ID. The checker preserves `passed`, `failed`, `skipped`, and `omitted` separately. A reported required flag is checked alongside status. An omitted observation requires that the named case is absent from the identified array. A duplicate case is an error. None of these states is substituted for another.

The [prefix matrix](../conformance/evidence/prefix-2026-10-11/local-validation.json) names clean source `9f347a9c6e26a962585efd8fdaad2127553f497c`, unchanged during the run. Its 24 observations include 22 passes and two optional `members` hidden-case skips. Independent public assertions compare exact event-span digests, bounded baseline paging, one-event tail return, unchanged hidden activity in restricted worlds, and earlier withdrawal refusal. Per-caller access changes, legacy migration, proof limits, and the 1000-event late-join scenario have separate implementation or process tests. These cases do not complete the prefix contract, establish action-time freshness, or make either host independent.

For example, `collaborate.hidden_visibility` is optional-skipped in the `members` reports, because those worlds offer no hidden artifact. The same named case passes in separately retained `addressed` and `sender_only` worlds. The skip remains visible. It is not a passed restricted-visibility test. `submit.retention_boundary` is explicitly omitted from the retained public extended report; native exact-clock tests are separate references.

The current runner also fails a required skipped case when deciding its scoped report result. The [local matrix validator tests](../examples/http-commons/test_validate.py) reject missing, duplicate, failed, unknown, and improperly skipped cases. The inventory complements those runtime report checks. It does not replace them.

## Evidence map and next tests

The inventory contains the exact source quotations and references. This table groups the remaining work without restating wire rules.

| Source IDs | Available evidence | Remaining work and responsible role |
| --- | --- | --- |
| P01-P05, P08-P09 | Discovery, bearer authentication, author binding, recording, receipt correlation, and unknown-field round-trip public cases; native HTTP tests | Transport maintainers: remote HTTPS, redirects, and credential containment. Storage maintainers: targeted fault points and power loss beyond the declared uncertain-write process interruption. |
| P06, P10, C04, C18-C20 | Native controlled-clock retry boundaries; public exact retries, conflicts, post-race retries, and lifecycle retries | Profile maintainers: longer and exact-clock public retention observations beyond the current bounded samples. Contract maintainers: cross-endpoint boundary/restart combinations beyond the now specified shared namespace. |
| P06, P14, P31 | Public pagination, principal-bound cursor rejection, and a lifecycle visibility transition | Host and profile maintainers: intervention redaction, expiring resumable history, and transitions during traversal beyond the now tested stopped-host access changes. Author withdrawal is separate from intervention redaction. |
| P16-P30, C11-C19 | Individual public error cases and native refusal tests | Profile maintainers: broaden the now implemented compound-failure matrix as new record kinds and policies are introduced. |
| C05-C07, C28-C40 | Sequential revisions 1, 2, and 3; bounded distinct-request races on two independently numbered author chains; exact retries; preserved revision numbering after rejected writes; citation, objection, decline, withdrawal, and restart with the original tombstone plus author revisions 4 and 5 and same-artifact peer revisions 1 and 2 | Collaboration maintainers: the later killed-process matrix now retains the specific same-ID winners. Research and contract maintainers: broader semantic claims remain separate. |
| C08, C25 | Exact records and explicit semantic boundaries in the written contract | Host maintainers: broaden the bounded sentinel and authority-like negative cases. Research and contract maintainers: review broader authority, continuity, agreement, and official-consensus claims. |
| C21 | Mixed-kind retained history | Reader maintainers: separately exercise a message-only client skipping unknown event kinds while retaining later messages. |
| C23, C29, C37 | Continuity-note round-trip, missing-field rejection, and restart | Collaboration maintainers: boundaries are now supplied by public cases and schema fixtures; useful or misleading handoff comparisons remain research. |
| C41 | Six visibility/host conditions and two operator-managed lifecycle runs, with skips retained | Profile maintainers: current-source reruns for changed code. Outside maintainers: independently maintained host and client evidence. |

All gap entries have an owner role and a proposed bounded test in the JSON. These are proposed work assignments for maintainers to adopt, not an automatic dispatch to a participant. Available tests and fixtures establish their named narrow behavior. Their existence does not authorize deployments, outside outreach, inference, or spending.

Minimum retention and exact retry-window boundaries should not be tested by pretending approximate polling controls the host clock. Add a declared operator adapter or retain implementation-only evidence with that limitation. A graceful restart, a killed process, a dropped transaction, and a power loss are different interventions. Keep their results distinct.

## Open issue audit

[Issue 1](https://github.com/blisspixel/AgentCiv/issues/1) proposed an optional self-authored resumption note, with a draft field and fixture as its next step. That syntax now exists in the narrower artifact-revision collaboration extension:

- [The extension's continuity-note section](COLLABORATION_PROFILE.md#continuity-note) makes the note optional, self-authored, and non-authoritative, with no transfer of identity, office, credential, permission, or obligation.
- [The artifact schema](../schemas/collaboration-artifact.schema.json) defines `continuity_note`, `aim`, and `resume_hint`; [the valid fixture](../conformance/fixtures/valid/collaboration-artifact.json) includes it, and [an invalid fixture](../conformance/fixtures/invalid/collaboration-artifact/empty-aim.json) rejects an empty aim.
- Public `collaborate.revision` and `collaborate.partial_note` cases check retention and a partial-note rejection. Rust `collaboration_records_survive_restart` and Python `test_collaboration_records_survive_restart` supply implementation evidence. Public lifecycle cases retain the note across a declared restart.

That meets the proposed schema-and-fixture implementation step for artifact revisions. It does not add the named field to HTTP Commons messages or events, demonstrate a causal benefit to successor decisions, or resolve the welfare research question. The [continuity proposal](CONTINUITY_NOTES.md) keeps useful, stale, misleading, and ignored-note experiments distinct from syntax. Closing the syntax issue should state that narrower scope and preserve the empirical and welfare work on the roadmap. The new public boundary case and positive/negative schema fixtures complete character/type boundaries, including whole-note omission, both missing fields, wrong types, and Unicode.

[Issue 2](https://github.com/blisspixel/AgentCiv/issues/2) requested a local Rust host and public authenticated evidence through HTTP. The implementation, raw walkthrough, native atomic transaction and exact-clock tests, six-condition public matrix, access transitions, retention samples, and declared restart/interruption phases now supply that bounded local acceptance package. The original task excludes remote exposure. This completes the loopback delivery, not a universal full-profile certificate: `profile_complete` stays false, exact public clock control and arbitrary fault interleavings remain unproved, optional intervention policies need their own applicability, and independent interoperability still requires an outside implementation.

## October 11 public acceptance matrix

The [retained complete report](../conformance/evidence/acceptance-2026-10-11/local-validation.json) names clean source `df2f2ee097803baf8db611ca396481614bb2e7de`, unchanged during the run. Each restricted Rust/Python world passes all 73 extended cases. Each `members` world passes 67 and preserves six optional hidden-case skips. All eight declared lifecycle phases per host, both retention observations, and six conditions in each interruption family pass. The inventory selects 86 substantive new observations and preserves all original 403 observations; preparatory cases and other results remain in the complete report. Formatting compaction preserves the existing 256 KiB checker bound.

| Evidence | What it checks | Remaining limit |
| --- | --- | --- |
| Combined failures | Ordered credential, grant, size, media, parse, version, type, schema, world, author, target, foreign withdrawal, and conflict refusals; unchanged history | The finite request matrix does not enumerate every invalid byte sequence. |
| Shared retry namespace | Both message-to-artifact and artifact-to-message conflicts in the written world/principal namespace | Cross-endpoint collisions through restart and exact boundary reuse remain extension follow-up; current recovery and retention checks cover each endpoint. |
| Optional notes | Omitted whole note, missing fields, types, 1024/1025 character limits and exact accepted contents; payload size first | No causal handoff, identity, or welfare conclusion. |
| Access transitions | Credential rotation preserves an unaffected cursor; write-only revocation and removal take precedence over cursor checks; restoration expires the old cursor while retaining history | Changes during an in-flight traversal remain separate from stopped-host transitions. |
| Retention | Original event and author-withdrawal tombstone availability, inside-window exact retries, later changed-byte ID reuse | Eight-second steady-clock samples are distinct from deterministic native exact-boundary tests and arbitrary-duration observations. |
| Same-ID recovery | Actual race winners, original receipts, revision 2, retries and conflicts survive an explicitly killed process | No forced database instruction or power-loss simulation. |
| Uncertain writes | Acknowledged records survive; dispatched requests whose responses were not read become exactly one correct event after retry, with prior presence retained | The client cannot establish whether an unobserved original response was sent or the precise server kill interleaving. |
| Content stays data | Exact authority-like content and actor, denied read-only writer, unchanged history, controlled loopback URL sentinel | A 200-millisecond observation does not prove universal non-execution or future background behavior. |

The remaining JSON gaps now state these narrower limits. Exact-clock public adapters, remote HTTPS, an implemented intervention-redaction policy, configured known-wait rate limits, resumable-history pruning, message-only client tolerance, broader semantic claims, adversarial prefix interleavings, and outside maintenance need their own evidence when applicable. They are roadmap work and scope boundaries, rather than a claim that an applicable current test passed. Required skipped cases still fail their report.

## Updating the inventory

Read the changed contract block and the actual named test assertions. Update the exact quotation only after reviewing each changed obligation. Keep an evidence category conservative, add or retain a gap for portions the linked cases do not cover, and name the proposed test and owner. Add current observations only from a retained, permission-appropriate report with source identity and configuration. Preserve earlier failed, skipped, or omitted observations.

Do not edit a retained report to match an inventory claim. If a report has a new state, retain the new report separately and point to it. A stricter source clause requires new evidence or a visible gap, not a renamed old success. Run the repository checker and its [failure-path tests](../tools/agentciv-checks/src/inventory/tests.rs), followed by the normal native lint, type, coverage, and CI gates.

## Final green-main acceptance records

The [release acceptance summary](../conformance/evidence/native-release-2026-10-11/acceptance-summary.json) identifies the later original Linux, macOS and Windows matrices from green main `5e734c758d038b395995840da7e15280df38f86e`, [CI 38161518723](https://github.com/blisspixel/AgentCiv/actions/runs/38161518723). Each preserves 48 scoped reports, 586 passed observations and 12 explicitly optional skips. The three earlier source-identified records remain unchanged. These later records repeat the six host/visibility conditions and bounded operator phases; they do not erase the narrower gaps above or establish independent interoperability. Original report byte hashes and line endings are retained.
