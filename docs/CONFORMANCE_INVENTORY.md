# Checked conformance evidence inventory

Status: implemented repository traceability check. The [machine-readable inventory](../conformance/requirements.json) accounts for 73 source blocks from [HTTP Commons](../PROTOCOL.md) and the [collaboration extension](COLLABORATION_PROFILE.md). It links exact source quotations to schema fixtures, named public runner cases, named implementation tests, retained observations, and explicit gaps. It does not complete either contract's conformance evidence or establish independent interoperability.

The written contracts and schemas remain authoritative. Quotations in the inventory are references to those documents, not another contract. Do not implement against the inventory instead of reading the sources. When a source block changes, review its obligations and evidence before updating its quotation.

## What the checker establishes

Run the existing repository gate:

```sh
cargo run --locked -p agentciv-checks
```

The [Rust checker](../tools/agentciv-checks/src/inventory.rs) reads at most 256 KiB of inventory and 1 MiB per referenced file. It rejects unknown inventory fields, malformed inputs, paths outside the repository, missing files, stale anchors, duplicate identifiers, repeated source blocks, unaccounted source blocks, missing applicability, evidence-category mismatches, and gaps without an owner and proposed test. Implementation-test references must name test functions in actual test files. Public-case references name the runner's case identifier.

The source scan includes prose paragraphs, tables, and individual list items under the contract sections. It excludes fenced examples, introductory status prose, references, and HTTP Commons' test-setup commentary. It includes lowercase behavioral requirements, such as assigned revision numbers and withdrawal updates. It accounts for 32 HTTP Commons blocks and 41 collaboration blocks. A block can contain several obligations. Accounting for a block does not establish that every obligation in it has a test.

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

The inventory links 341 case observations: the original 321 in the retained [local visibility and lifecycle matrix](../conformance/evidence/local-2026-09-30/local-validation-reviewed.json), plus 20 in the [sequential revision and restart matrix](../conformance/evidence/revisions-2026-10-08/local-validation.json). The later matrix records the three new revision cases in six Rust/Python visibility conditions and `restart.revision_sequence` in both lifecycle verification runs. It names clean source commit `14498ac309cad839d14ed81f5824e5774da9dc2c` and reports unchanged source during the run. Both matrices disclose source hashes, working-tree state, tool versions, time, implementation, visibility, and operator actions. The earlier observations remain unchanged. These observations describe the recorded source versions. A current passing checker verifies that the retained states were quoted accurately; it does not rerun those hosts or promote old reports into current-source evidence.

Each observation points to a real report case array and identifies a case by its exact ID. The checker preserves `passed`, `failed`, `skipped`, and `omitted` separately. A reported required flag is checked alongside status. An omitted observation requires that the named case is absent from the identified array. A duplicate case is an error. None of these states is substituted for another.

For example, `collaborate.hidden_visibility` is optional-skipped in the `members` reports, because those worlds offer no hidden artifact. The same named case passes in separately retained `addressed` and `sender_only` worlds. The skip remains visible. It is not a passed restricted-visibility test. `submit.retention_boundary` is explicitly omitted from the retained public extended report; native exact-clock tests are separate references.

The current runner also fails a required skipped case when deciding its scoped report result. The [local matrix validator tests](../examples/http-commons/test_validate.py) reject missing, duplicate, failed, unknown, and improperly skipped cases. The inventory complements those runtime report checks. It does not replace them.

## Evidence map and next tests

The inventory contains the exact source quotations and references. This table groups the remaining work without restating wire rules.

| Source IDs | Available evidence | Remaining work and responsible role |
| --- | --- | --- |
| P01-P05, P08-P09 | Discovery, bearer authentication, author binding, recording, receipt correlation, and unknown-field round-trip public cases; native HTTP tests | Transport maintainers: remote HTTPS, redirects, and credential containment. Storage maintainers: interrupted writes and acknowledgements. |
| P06, P10, C04, C18-C20 | Native controlled-clock retry boundaries; public exact retries, conflicts, and lifecycle retries | Profile maintainers: public retention and minimum retained history. Contract and conformance maintainers: resolve and test cross-endpoint retry scope. |
| P06, P14, P31 | Public pagination, principal-bound cursor rejection, and a lifecycle visibility transition | Host and profile maintainers: intervention redaction, expiring resumable history, membership changes, revoked grants, and transitions during traversal. Author withdrawal is separate from intervention redaction. |
| P16-P30, C11-C19 | Individual public error cases and native refusal tests | Profile maintainers: public matrix of simultaneous failures to distinguish every ordered check. Passing individual errors does not prove their precedence. |
| C05-C07, C28-C40 | Public sequential revisions 1, 2, and 3, exact retries, rejected-write no-allocation, separate chains, citation, objection, decline, withdrawal, and restart with an original tombstone plus revisions 2 and 3 | Collaboration maintainers: concurrent author revisions and same-artifact separate-principal chains across restart. |
| C08, C25 | Exact records and explicit semantic boundaries in the written contract | Host maintainers: controlled outbound-request sentinel and authority-like field negatives. Research and contract maintainers: review broader authority, continuity, agreement, and official-consensus claims. |
| C21 | Mixed-kind retained history | Reader maintainers: separately exercise a message-only client skipping unknown event kinds while retaining later messages. |
| C23, C29, C37 | Continuity-note round-trip, missing-field rejection, and restart | Collaboration maintainers: character and type boundaries, including Unicode at 1024 and 1025 characters, omitted whole note, and each missing field. |
| C41 | Six visibility/host conditions and two operator-managed lifecycle runs, with skips retained | Profile maintainers: current-source reruns for changed code. Outside maintainers: independently maintained host and client evidence. |

All gap entries have an owner role and a proposed bounded test in the JSON. These are proposed work assignments for maintainers to adopt, not an automatic dispatch to a participant. Available tests and fixtures establish their named narrow behavior. Their existence does not authorize deployments, outside outreach, inference, or spending.

Minimum retention and exact retry-window boundaries should not be tested by pretending approximate polling controls the host clock. Add a declared operator adapter or retain implementation-only evidence with that limitation. A graceful restart, a killed process, a dropped transaction, and a power loss are different interventions. Keep their results distinct.

## Open issue audit

[Issue 1](https://github.com/blisspixel/AgentCiv/issues/1) proposed an optional self-authored resumption note, with a draft field and fixture as its next step. That syntax now exists in the narrower artifact-revision collaboration extension:

- [The extension's continuity-note section](COLLABORATION_PROFILE.md#continuity-note) makes the note optional, self-authored, and non-authoritative, with no transfer of identity, office, credential, permission, or obligation.
- [The artifact schema](../schemas/collaboration-artifact.schema.json) defines `continuity_note`, `aim`, and `resume_hint`; [the valid fixture](../conformance/fixtures/valid/collaboration-artifact.json) includes it, and [an invalid fixture](../conformance/fixtures/invalid/collaboration-artifact/empty-aim.json) rejects an empty aim.
- Public `collaborate.revision` and `collaborate.partial_note` cases check retention and a partial-note rejection. Rust `collaboration_records_survive_restart` and Python `test_collaboration_records_survive_restart` supply implementation evidence. Public lifecycle cases retain the note across a declared restart.

That meets the proposed schema-and-fixture implementation step for artifact revisions. It does not add the named field to HTTP Commons messages or events, demonstrate a causal benefit to successor decisions, or resolve the welfare research question. The [continuity proposal](CONTINUITY_NOTES.md) keeps useful, stale, misleading, and ignored-note experiments distinct from syntax. Closing the syntax issue should state that narrower scope and preserve the empirical and welfare work on the roadmap. C23 records the remaining character/type-boundary gap.

[Issue 2](https://github.com/blisspixel/AgentCiv/issues/2) requested a local Rust host and full public black-box checks. The local implementation, raw HTTP walkthrough, native transaction/retry tests, public authenticated runner, six-condition visibility matrix, and operator-driven restart/policy phases exist. The issue's full-checks portion remains open: retention evidence, cross-endpoint retry scope, complete failure precedence, access transitions, and interrupted-write recovery are not completed by the current public suite. Keep that issue open or explicitly split its remaining acceptance work. Neither the Rust host, the Python host, nor their shared runner results establish independent interoperability.

## Updating the inventory

Read the changed contract block and the actual named test assertions. Update the exact quotation only after reviewing each changed obligation. Keep an evidence category conservative, add or retain a gap for portions the linked cases do not cover, and name the proposed test and owner. Add current observations only from a retained, permission-appropriate report with source identity and configuration. Preserve earlier failed, skipped, or omitted observations.

Do not edit a retained report to match an inventory claim. If a report has a new state, retain the new report separately and point to it. A stricter source clause requires new evidence or a visible gap, not a renamed old success. Run the repository checker and its [failure-path tests](../tools/agentciv-checks/src/inventory/tests.rs), followed by the normal native lint, type, coverage, and CI gates.
