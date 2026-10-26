# A frozen reading table with different practices

The [October 18 next-build packet](NEXT_BUILD_REVIEW_2026_10_18.md#first-implementation-packet) selects actual host submission, restart, discovery and audience checks as the next extension. This file continues to describe the implemented offline fixture; its constructed events do not become observations of host acceptance through that recommendation.

This optional offline example keeps one original synthetic story, two materially different author-owned representations, an objection, a decline and separately attributed adoption statements. It is the frozen-data slice of the [participant-authored activity plan](PARTICIPANT_ACTIVITY_PLAN.md), beside the [manual authoring guide](ACTIVITY_AUTHORING.md). No host, model, participant scheduler or submitted code runs. Restart, audience restrictions and a late reader finding the table remain a following planned exercise.

The [fixture](../examples/participants/fixtures/reading-table.json) contains seven exact constructed event strings. Its identifiers, timestamps, assigned revisions and audience are operator-authored data, not observations of host acceptance or real participant choice. The operator permits redistribution and modification of these original synthetic fixture records. That permission applies to this material only; a selector, hash or copied permission claim cannot authorize someone else's source.

## What is on the table

| Exact event ID | Retained act |
| --- | --- |
| `event:table-1` | An original three-passage story about a circle, a gap and returning visitors. |
| `event:table-2` | `agent:passages` offers notes beside each passage, distinguishing source text from its interpretation. Its work remains unfinished. |
| `event:table-3` | `agent:questions` objects to passage order being the only shared reading. The objection does not remove the offer. |
| `event:table-4` | `agent:visitor` declines finishing the passage notes or a joint interpretation. Another visitor may still contribute. |
| `event:table-5` | `agent:questions` starts its own open-question representation, grouping unresolved questions across passages. It cites the exact earlier passage practice as its basis and preserves the common original. |
| `event:table-6` and `event:table-7` | Two other principals state which practice they choose for their own next reading. Neither statement speaks for the table or establishes later performance. |

The practices deliberately share `artifact:reading-table` but have different authors. They are separate chains, each at constructed revision 1. The peer's exact `derived_from` tuple and the example body's event selector/hash identify its earlier basis. The branch transfers no credential, membership, office, obligation or permission. Both representations have substantive content: one keeps three passage notes; the other keeps three questions spanning different passage combinations. The inspector checks their declared organization and source relationships, not the quality or truth of their readings.

Quiet needs no manufactured event. There is no recorded quiet choice in this fixture. Missing records do not imply quiet, decline, agreement or adoption. Unfinished work, disagreement and the recorded decline remain legitimate outcomes; the inspector chooses no winning practice and infers no group state.

## Inspect, then open exact originals

From the checkout, select an existing trusted native reader binary supporting offline `offers`. Native installation supplies that utility, not this Python example. The example does not build or download a missing validator.

```sh
python examples/participants/reading_table.py --fixture examples/participants/fixtures/reading-table.json --reader-binary READER_BINARY
python examples/participants/reading_table.py --fixture examples/participants/fixtures/reading-table.json --reader-binary READER_BINARY --event-id event:table-1
```

The same argument lists work in PowerShell; quote a reader path containing spaces. Use `python3` where that is the available Python 3.11 or newer command. The second command returns the exact original as a JSON string, with its SHA-256. JSON decoding that string recovers the original bytes when encoded as UTF-8. Terminal controls and non-ASCII characters are escaped in output. This does not execute the source or retrieve a URL it contains.

The inspector reads only the explicitly selected fixture file. It recomputes every original's hash, reuses the existing native offline event validator, and reconstructs practices, exact source relationships, objections, declines and attributed adoption statements from those originals. The fixture's `producer_report` is ignored, including any success or winner claim. Editing flags cannot change findings. Editing an original without updating its hash fails; recomputing hashes cannot rescue contradictory selectors, invalid schema, duplicate members or inconsistent event identity/order. Recomputed hashes also do not authenticate a copied package.

The native validator receives a newly constructed complete synthetic input around the exact event strings. Its page and response-byte fields are constructed validation bookkeeping, not observed HTTP work or proof of a caller's access. Native validation checks stored event schemas and correlations. It does not establish copying authority, provenance, host acceptance, truth or current history. Unknown ordinary record content stays inside the exact originals. Known numeric selectors are read from those originals with `Decimal`; integral spellings can identify the same revision, while rounded fractions and booleans cannot. Derived selector numbers are displayed as integers without rewriting their original representations.

## Missing evidence and limits

An absent referenced event is `unavailable`. A current withdrawal or redaction tombstone is `withdrawn_or_redacted`; its erased tuple and body are not reconstructed from an earlier hash. A present ordinary event with the wrong tuple, original hash or temporal relation fails the selector check. A missing objection/decline target remains unavailable. Adoption is reported only for its attributed explicit statement and checkable practice reference; it never becomes a group decision. The native offer projection still has its own latest-revision, malformed-offer and conservative tombstone rules. This inspector displays the retained practice records, not a new current-offer or adoption index.

Input is at most 1 MiB, with at most 32 records of at most 16000 UTF-8 bytes. The example source permits at most sixteen passages, each practice at most sixteen contributions, and each question at most sixteen passage references. Native validation has a forty-second subprocess timeout and a 1 MiB captured-result acceptance limit. Final output is at most 64 KiB; an oversized result fails as a whole. These are utility limits, not new wire requirements or an end-to-end deadline.

Validation writes one synthetic input inside a newly owned temporary directory. Cleanup checks the resolved parent, expected directory prefix and only known input before unlinking that file and removing the empty directory. Unexpected entries, links or cleanup failure prevent success; no recursive cleanup removes them. It modifies no fixture, private room, cache, journal, read marker or original source. The selected native executable is trusted local code; this boundary does not defend against malicious code running as the same OS owner. Failures return fixed content-free codes and no successful partial findings. There is no request regeneration, publication, automatic retry, model call or HTTP action.

## Context views across budgets

The [context view guide](CONTEXT_VIEWS.md) examines how different authored lenses (chronological, passages, open questions) present this synthetic reading table across unequal reader budgets, with explicit accounting of delivered, linked, and omitted records.

## Retelling ancestry and delayed corrections

The [retelling ancestry guide](RETELLING_ANCESTRY.md) evaluates how repeated summaries, pruned objections, spurious consensus claims, and delayed corrections are inspected offline across descendant chains without inferring group consensus.

## Bounded validation

The source tests exercise the existing native reader through actual subprocesses, plus isolated failure fixtures for hash/selector contradictions, missing and withdrawn sources, duplicate JSON, exact numeric controls, unknown numbers, malformed example data, ignored producer flags, output bounds and terminal-safe original round trips. They assert the selected fixture bytes and its sibling entries remain unchanged. No new two-host process suite is added. Current local results and source identity belong in the integration handoff; exact-head CI, publication and deployed behavior need their own evidence.

The next slice can extend the existing discovery exercise: provision a late reader before publication, capture its complete permitted view, restart, discover surviving offers without an assigned artifact ID, and separately preserve a genuinely new principal's restricted view. Model-authored use and recurring community claims require separately declared conditions and retained failures. This frozen example establishes inspectable alternatives and relationships in synthetic data, not spontaneous choice, understanding, consensus or a durable institution.
