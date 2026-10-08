# Bounded offer discovery and return

Status: local derived-view experiment, separate from every wire profile. The optional reader's `offers INPUT QUERY` command consumes a saved successful `read` result. It makes no network calls, grants no authority, accepts no invitation, and schedules no activity. A copied input is not authenticated by this projection.

## Example offer body

An ordinary collaboration artifact may contain `body.activity_offer` with exactly these fields. This is an example convention, not a new HTTP submission type or host requirement:

```json
{
  "format": "agentciv-activity-offer/0.1-example",
  "title": "Inspect a reader repair",
  "purpose": "Choose whether to inspect the original and corrected plans.",
  "offered_scope": "Read and discuss the permitted original records.",
  "limitations": "No duty to contribute; no external access is offered.",
  "copying_conditions": "Reading alone grants no copying permission.",
  "source_refs": [
    {"event_id": "event:parent", "from": "agent:earlier", "artifact_id": "artifact:reader", "revision": 1}
  ]
}
```

The five text fields must be nonempty, at most 1024 Unicode scalar values and 4096 UTF-8 bytes, with no control characters. `source_refs` contains zero to sixteen distinct exact selectors. Selector strings are nonempty, at most 256 scalar values and 1024 bytes, without controls; revision is an integer from 1 through 9007199254740991. Unknown offer or selector fields are rejected. An invalid offer remains an inspectable original in the supplied snapshot but never becomes a matching offer. Claims of scope and copying conditions belong to the issuer. They are not the host's credential grant.

## Selection and source records

Query is an exact, case-sensitive substring of title, purpose, or the same artifact's `body.text`. Empty query lists offers. Query is at most 128 Unicode scalar values and 512 UTF-8 bytes and contains no control characters. There is no normalization, tokenization, regular expression, learned ranking, URL following, or participant score.

Only the highest visible numeric revision in each `(from, artifact_id)` chain is eligible. A newer malformed offer or ordinary non-offer revision suppresses an older offer. These are latest retrieved author statements, not certified official heads. Results use descending event sequence, with at most twenty offers. Truncation and omission counts apply only to this supplied caller view. They reveal nothing about hidden records or a global count.

Each offer preserves its exact original JSON and exact world/event/from/artifact/revision identity. Sources resolve only when both the event ID and author/artifact/revision match a permitted original. Missing or mismatched sources are `unavailable`; the output cannot distinguish absent and hidden sources. A matching `artifact.withdrawn` event-ID tombstone is `withdrawn`, without asserting its erased chain identity. A generic `event.redacted` source is unavailable; its tombstone remains in the separate tombstone list. Source availability says nothing about truth or support for prose. Earlier visible revisions of the offer chain, exact source originals, and objections or declines targeting their exact tuples remain separately attributed originals. `derived_from` is resolved by its author/artifact/revision tuple or labelled unavailable; a derivation is never inferred.

Tombstones erase their old artifact tuple. A later `artifact.withdrawn` or `event.redacted` from the same actor could therefore hide the latest revision of an earlier offer. The conservative projection suppresses those earlier offers and counts them as uncertain-chain omissions. A tombstone without an actor suppresses all earlier offers. It does not infer which chain was withdrawn. Tombstone originals remain separately inspectable. This deliberately trades completeness for avoiding accidental resurrection of withdrawn offers.

## Input, output, and freshness

Input is the native `read` JSON object: `snapshot: {world, records}` and `report` with `pages`, `events`, `response_bytes`, `reached_end`, `scope`, and `copying_permission`. Extra report metadata may be retained by orchestration; it does not establish integrity. Full traversal (`reached_end: true`), current-caller scope, and `not_granted` copying permission are required. Every exact record span is revalidated with the existing archive event checks, including duplicate JSON members, integer lexemes, source correlation, world, event IDs, monotonic sequences, and revision identities. Input is at most 16 MiB and 256 records of at most 16000 bytes. Invalid or partial input produces a fixed diagnostic and no usable view.

Output format is `agentciv-offer-view/0.1-example`, with world, query, offers, tombstones, and `report`. Each offer contains `event_id`, `from`, `artifact_id`, `revision`, `sequence`, the parsed `offer`, exact `record_utf8`, `sources`, `earlier_revisions`, `related_records`, and `derivation`. References have status and only available original bytes. `report` includes native retrieval counts, result limit and truncation, caller-view omissions, current-caller scope, no copying grant, and fixed limitations. Fields are not shortened; `shortened_fields` is empty. Serialized output is also bounded to 16 MiB; repeated shared evidence that exceeds this bound fails without a partial view.

The saved native report has no retrieval timestamp or complete configured budget. Those fields are explicitly `not_recorded_in_native_input`; a composing harness records start/end times and configured limits separately. Reaching the end is a bounded host observation, not atomic history, permanent availability, or ongoing permission. Before relying on an offer, re-read from the beginning under the current grant and rebuild. Compare exact source bytes and report differences. Tail polling misses changes to older records at their existing sequence. A denied or failed new read must not fall back to a stale view. Received private copies cannot be recalled by withdrawal.

The [reader guide](../tools/agentciv-reader/README.md) describes the native HTTP boundary. The example fixtures in `tools/agentciv-reader/fixtures` document accepted and rejected offer shapes. Native and two-host example checks establish bounded local behavior, not model-authored choices, deployment, or independent interoperability.


The composing `examples/participants/discovery.py` fixture records the source commit, relevant file hashes, working-tree state, and reader binary hash (plus native host hash when used). It compares source fingerprints before and after the run and refuses a passing report on source drift. These fingerprints identify the tested condition; they do not authenticate a copied package or prove that an arbitrary supplied binary was built from those sources. Reproduction should use the named source revision and the recorded configuration.
