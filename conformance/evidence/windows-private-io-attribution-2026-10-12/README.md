# Local Windows private-file test attribution

Observed on 2026-10-12 at isolated source `216e05938f662d944a532b473696e2a5bb4ef01c`. This packet is a bounded local diagnostic, not a GitHub CI observation or an optimization. The [dated review](../../../docs/WINDOWS_PRIVATE_IO_REVIEW_2026_10_12.md) explains the maintenance decision.

The original diagnostic ran exactly three existing synthetic tests. Its count-only wrapper called the original `_windows_acl` unchanged and restored it afterward. Existing assertions and mocked client/native responses remained in place. The selected tests made no network or provider request and ran no native executable. No permission verdict was cached. Output contains fixed test labels, counts, times and environment versions, without paths, participant text, credentials or exception messages.

| Existing test | ACL calls | Helper seconds | Test wall seconds |
| --- | ---: | ---: | ---: |
| Private bytes, bounds, replacement and no overwrite | 19 | 6.594930 | 6.617080 |
| Bounded lock and release after failure | 21 | 7.864028 | 7.887865 |
| Empty read and saved-view no overwrite | 32 | 10.672294 | 10.719877 |

All three tests and all 72 ACL calls passed on Windows 11 build `10.0.26200` with Python `3.14.7`. The supervisor measured 25.581056 seconds. Rounded per-call spans sum to 25.131254 seconds, approximately 99.63% of the 25.224822 test wall seconds. Separately rounded per-test helper totals sum to 25.131252 seconds; the two-microsecond difference follows the recorded rounding. Helper spans include startup, serialization, real ACL work and waiting; this diagnostic does not separate their costs.

The method declared a 120-second ceiling, a 105-second worker refusal boundary and a 110-second supervisor wait followed by bounded owned-process termination. The observed run did not exercise termination. Existing owned temporary-directory contexts completed; the diagnostic performed no extra deletion. The source worktree was tracked-clean before and after.

## Files and source binding

`observations.jsonl` preserves the 80 original emitted records byte for byte. `summary.json` preserves the supervisor's original output bytes; its contents are a derived projection plus supervisor metadata. `diagnostic-method.py.txt` preserves the original ignored diagnostic source as evidence, not a new maintained executable or CI hook. `source-map.json` is a separately derived comparison, and `SHA256SUMS` covers the other six files, excluding itself. Local attributes preserve these bytes.

All ten relevant Git blobs at the observed isolated source equal their bytes at public main `7d1f921903eb3264a74615f039bea02887980436`. The source map names them and separately records the actual CRLF worktree byte hashes. A reproducible public source set does not reattribute the run to another commit. Eight blobs also match `1f0ff2a59eb9bd8fa1eaab65b2f5dac669aed936`; the two changed files prevent using that earlier source as the complete reproduction set.

To reproduce on Windows, create an owned clean worktree under `.agents` at the public source above. Copy `diagnostic-method.py.txt` unchanged to the repository's `.agents/windows-private-io-attribution.py`, then run:

```powershell
python .agents/windows-private-io-attribution.py --worktree .agents/OWNED_PUBLIC_SOURCE_WORKTREE --output .agents/FRESH_DIAGNOSTIC_OUTPUT
```

The helper must remain directly under `.agents`, where its containment boundary is anchored. Inspect its retained argument definitions before running. Keep fresh reproduction outputs separate; their timings will differ. The original run used the isolated source named above.

This sample does not measure Python 3.11, explain historical GitHub runner delays, predict the full suite or compare implementations. Measure the unchanged full participant discovery on both Windows Python versions before selecting an optimization. Fresh permission checks, failure behavior and complete assertions remain required.
