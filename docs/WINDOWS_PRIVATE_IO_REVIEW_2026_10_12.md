# Windows CI and private-file work

Reviewed 2026-10-12. This review changes no permission policy or CI gate. It identifies a measured delay and a bounded next investigation before optimizing private I/O.

## Actual CI step durations

Source `1f0ff2a59eb9bd8fa1eaab65b2f5dac669aed936` passed [main CI 38209480244](https://github.com/blisspixel/AgentCiv/actions/runs/38209480244). Completed job step timestamps show:

| Job | Participant suite | Native build | Python host suite |
| --- | --- | --- | --- |
| [Windows Python 3.11](https://github.com/blisspixel/AgentCiv/actions/runs/38209480244/job/114680213025) | 32m46s | 2m43s | 1m12s |
| [Windows Python 3.14](https://github.com/blisspixel/AgentCiv/actions/runs/38209480244/job/114680213068) | 43m27s | 2m47s | 1m39s |
| [Ubuntu Python 3.11](https://github.com/blisspixel/AgentCiv/actions/runs/38209480244/job/114680212958) | 2m18s | 1m15s | 18s |
| [Ubuntu Python 3.14](https://github.com/blisspixel/AgentCiv/actions/runs/38209480244/job/114680212979) | 4m51s | 1m12s | 18s |

Durations are rounded timestamp differences. The Windows Python 3.14 log reports 276 participant tests in 2605.433 seconds; Ubuntu reports the same count in 290.682 seconds. These are source-bound historical results. Native compilation is not the largest observed step. The logs contain no per-test timings, so they do not establish which private operation caused the difference.

## Local synthetic observation

The [original diagnostic](../conformance/evidence/windows-private-io-2026-10-12/benchmark.json) used a newly owned temporary directory and one 16-byte synthetic file, with no service, model, network or user file. All 15 ACL helper invocations passed and took 5.507 seconds in total. Three fresh directory checks took 0.346, 0.334 and 0.366 seconds; three file-plus-parent checks took 0.667, 0.714 and 0.787 seconds. Three no-op PowerShell starts took 0.243, 0.195 and 0.194 seconds.

The complete diagnostic took 6.463 seconds, within its 90-second ceiling. It retained per-call results and verified containment before removing only its owned file and directories. The environment was Windows 11 build 10.0.26200, PowerShell 5.1.26100.9549 and Python 3.14.7. Permission verdicts were not cached.

Process startup is material for these tiny local operations. The sample does not isolate startup cost, profile GitHub runners or prove a cause for the 32 to 43 minute CI steps. Sample order, caches, machine and workload differ.

## Bounded attribution of three existing tests

A second [retained local diagnostic](../conformance/evidence/windows-private-io-attribution-2026-10-12/README.md) ran exactly three existing synthetic tests with a count-only wrapper around the unchanged ACL helper. The observed source was isolated commit `216e05938f662d944a532b473696e2a5bb4ef01c`, not a green-main CI run. All ten relevant project source files have identical Git blob bytes at public main `7d1f921903eb3264a74615f039bea02887980436`; the packet retains that source comparison and original diagnostic method. This supplies a reproducible public source set without reattributing the observation.

On Windows 11 build `10.0.26200` with Python `3.14.7`, all three tests passed in 25.581056 supervisor seconds, within a 120-second ceiling. Their 72 real ACL calls all passed, without cached permission verdicts. Rounded per-call measurements sum to 25.131254 seconds, approximately 99.63% of the 25.224822 seconds of test wall time. Helper spans include startup, serialization, real ACL work and waiting; the diagnostic does not separate those components. The packet preserves original observation and supervisor-output bytes, labels derived records and retains the two-microsecond difference caused by independently rounded per-test totals.

This attributes the selected local tests only. It does not measure Python 3.11, explain historical GitHub runner delays, predict the complete suite, compare implementations or establish an optimization. No tracked implementation, permission check, assertion, model, service or CI gate changed. Next measure per-test and ACL-helper work across the unchanged complete participant discovery on both Windows Python versions before selecting an optimization.

## Next maintenance slice

First measure existing test durations and aggregate ACL helper counts and elapsed time, using fixed test names and counts. Keep private paths, process input, credentials and participant text out of diagnostics. Python 3.11 needs a compatible result wrapper. Preserve the complete discovery list, assertions and platform gates.

If attribution supports it, batch consecutive parent/file checks within one fresh bounded helper invocation per operation. Preserve ownership, allowed identities, full control, directory protection, ancestor and link checks before opening or writing private data. Check every path afresh; an invalid member fails the operation. Compare both Windows Python versions and retain timeout, malformed-response, foreign-owner, extra-identity, denied-access and link regressions. No performance improvement is claimed before that comparison.

This maintenance can proceed beside the [request-inspection](REQUEST_RECONCILIATION_PLAN.md) and [reading-table](PARTICIPANT_ACTIVITY_PLAN.md) work. It grants no permission to weaken checks or change private-storage access.
