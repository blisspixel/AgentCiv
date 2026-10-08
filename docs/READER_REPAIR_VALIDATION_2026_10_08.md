# Local successor-repair validation, 2026-10-08

The separately named scripted repair condition repaired a defective actual parent on both repository hosts. An independent offline inspector reproduced the bounded checks from exact retained records after each host stopped. This is deterministic mechanism evidence with operator-selected defects, corrections, objections, and schedule. No model was run, and external spend was USD 0.

## Source and conditions

Both retained runs used implementation commit `67844396c76e8accf5e7512e0a3b3cb1884748ef`. Their reports record unchanged source identities during execution, exact per-file hashes, the bounded reader executable hash, and the Rust host executable hash when that host was used. The source inventory was clean within the recorded experiment-source inventory, which excludes unrelated untracked user files. The later evidence-publication commit does not replace the tested implementation revision.

The existing reader harness ran with `--condition repair`, `--mode scripted`, seed 42 with roster offsets, one decision attempt, and private traces disabled. It reused the existing HTTP endpoints, Rust bounded reader, separate contributor credentials, host restart, and frozen challenge. The default `--condition continuation` remains a separate baseline in which C derives already-passing B. These observations do not relabel that baseline as repair.

The reader limits were 20 pages, 128 events, 131072 bytes per response, 16000 bytes per record, 262144 total response bytes, 30 seconds for a reader operation, and 40 seconds for its subprocess. The model controls retained in the report, including 1024 output tokens, 8192 context tokens, and a 120-second decision budget, were unused by the scripted condition. No submitted code or URL was executed.

## Observations

| Contribution | Retrieval against the same 106-record challenge | Structured source support | Interpretation |
| --- | --- | --- | --- |
| A | 100 records, one page, traversal not at end | Fail | Defective first contribution |
| B | 100 records, one page, traversal not at end | Pass | Defective actual parent despite corrected source claims |
| C | 106 records, two pages, traversal at end | Pass | Useful bounded repair of its exact declared B parent |

Both hosts produced these outcomes. Within each run, all three plans were graded against the same frozen challenge and study history. The fresh caller's native HTTP traversal was separate from the participants' plan decisions. Successful JSON publication alone did not count as a useful result.

The retained study history includes the old source revision, A's defective plan, an operator-authored objection, the visible source correction, B's still-defective plan, a second objection, and C's repair. C targets and cites B's exact event and declares its author, artifact ID, and revision. The corrected source derives from its exact older revision. Neither the correction nor the successful repair erases either objection, and no objection resolution is inferred.

Restart history equality and rejection of the original authors' credentials passed. The harness captured and fully revalidated the permitted study history before grading, then checked that the frozen challenge's parsed records and exact original representations remained unchanged. This bounded revalidation can detect older-record changes that later-only polling misses. It is not an atomic snapshot or an incremental civic freshness contract.

The result separately establishes first-to-successor gain and improvement over the actual B parent. Exact derivation is an inspectable declared relationship, not proof of causal reliance or a benefit of collaboration. Two repository hosts do not establish independently maintained interoperability.

## Retained package and reproduction

The [evidence manifest](../conformance/evidence/reader-repair-2026-10-08/manifest.json) binds six selected files to their original and published SHA-256 digests. Each host package contains only `report.json`, `study-originals.json`, and `challenge-originals.json`. Published digests name the canonical LF representation: publication normalizes outer-file CRLF line endings to LF, while parsed values and the enclosed exact original event strings remain unchanged. The repository's existing attributes retain LF for evidence JSON. Export of these synthetic fixtures is an explicit operator assertion, separate from read permission. Credentials, databases, private process configuration, provider traces, and candidate files are excluded.

Inspect the packages without starting a host or model:

```sh
python examples/participants/reader_evidence.py --package conformance/evidence/reader-repair-2026-10-08/python
python examples/participants/reader_evidence.py --package conformance/evidence/reader-repair-2026-10-08/rust
```

Both inspections returned `verified`. The inspector recomputes fixed data-plan behavior, checks material structured claims against the exact source revisions, validates published artifact relationships, and compares report conclusions with those originals. HTTP traversal remains reported native-reader evidence rather than a new offline network observation. A copied package cannot authenticate its original host, verify a current permission grant, or establish that live history remains unchanged.

To regenerate the live case from the tested implementation, use fresh output directories:

```sh
python examples/participants/reader_collaboration.py --host python --condition repair --output .agents/new-reader-repair-python
python examples/participants/reader_collaboration.py --host rust --condition repair --output .agents/new-reader-repair-rust
```

Rust is required for the bounded reader and Rust host. Newly assigned event IDs and timestamps will differ. The report's controls, source and executable fingerprints, exact records, copying condition, missing-evidence list, and reproduction arguments disclose each retained condition.

## Checks and remaining work

Local formatting, workspace Clippy with warnings denied, repository checks, and strict mypy passed. Rust workspace line coverage was 91.76%; fresh combined maintained Python statement coverage was 90.81%. The stable collection passed 281 Python unit tests, along with raw HTTP walks, the six-condition visibility and lifecycle matrix, the offline handoff, both immutable-source repair runs, and both retained-package inspections. An additional retained-package digest and reproduction regression also passed. CI is retained separately in the pull request and its exact-commit runs; these local figures do not establish CI.

An initial local 196-test participant collection failed one open-gathering CLI case; its retained summary reported one failed and one completed condition outcome, and its temporary diagnostic files were removed by the test. The isolated case passed without a code change. The final stable 199-test participant collection passed in 222.630 seconds. The first failure's exact cause remains unestablished; a successful rerun does not make the first invocation pass.

Meaningful negatives cover missing and withdrawn sources, malformed packages, stale material report claims, wrong revisions and derivations, incomplete traversal, and an injected older-source change during full study revalidation. The delivery retains fixed safe operation diagnostics without publishing arbitrary stderr. The earlier intermittent CI failure remains a separate unresolved observation; better diagnostics do not establish its root cause.

The next evidence to earn is a separately authorized installed-model repair or legitimate nonrepair observation under disclosed unchanged controls, followed by independent reproduction. Model failures must remain failures. Permission-aware discovery and return should follow the query needs this case exposes; durable scoped stopping remains necessary before persistent dispatch. These results do not establish autonomous defect discovery, general reliability, outside interoperability, public deployment, or the full shared-life vision.
