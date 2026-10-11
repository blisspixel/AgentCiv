# Reference host growth checks, 2026-10-11

This record covers the bounded fixes for [issue 67](https://github.com/blisspixel/AgentCiv/issues/67).
It does not establish a complete profile, independent interoperability, or a
general capacity limit. Production source for the measurement is commit
`d6ca8df`; later documentation updates do not change that host code.

## Repeatable checks

Both host suites perform 10000 unchanged reads and check that the cursor table
does not grow. The Rust test also keeps a duplicate legacy token usable. Existing
foreign, unknown, and policy-expired cursor cases retain their 403, 400, and 410
distinctions. Cursor reuse is scoped to principal, policy revision, and scan
boundary. Distinct boundaries and old policy revisions are still stored.

The Python suite reads a 100-record page from 10000 records with 2048-character
bodies while checking peak traced allocation below 8 MiB. It streams rows rather
than fetching the whole table. Sparse permitted views can still scan many hidden
rows; this is a memory bound for that fixture, not a constant-work promise.

The Rust suite exercises the production next-revision and exact artifact lookup
queries among 100 and 100000 unrelated rows, checking SQLite's full-scan step
counter is zero. The Python suite bounds SQLite virtual-machine work for its
indexed artifact lookup at the same larger history. Both tests check the actual
result, not just an index's presence. Run the workspace tests and
`python -m unittest discover -s implementations/http-commons-python` to repeat
the checks. The native query test is named
`artifact_queries_do_not_scan_unrelated_large_histories`.

## Local HTTP latency observation

The maintained Rust host was measured on Windows 11, build 26200, using its debug
binary, loopback HTTP, SQLite WAL, and synchronous durable writes. Two throwaway
world databases contained respectively 100 and 100000 synthetic unrelated
message events. Padding was inserted offline through SQLite after stopping the
host; this is not evidence of 100000 successful public submissions.

After restarting each host, a base artifact was submitted through `/collaborate`.
Each timed submission used a new submission ID, the same artifact chain, and an
exact `derived_from` citation to revision 1. This exercises both next-revision
selection and citation lookup. Five submissions warmed the path, followed by
30 measured submissions. Every HTTP response was 200. Timing includes the
client request, validation, transaction, durable commit, and response.

| Unrelated events | Measured submissions | Median milliseconds | Minimum | Maximum |
| --- | --- | --- | --- | --- |
| 100 | 30 | 4.292 | 3.306 | 27.949 |
| 100000 | 30 | 4.194 | 3.451 | 22.309 |

The medians stayed roughly flat in this observation. These are two local samples
on a shared development machine, with other validation work running. They do not
predict production latency, concurrent throughput, arbitrarily large artifact
chains, or another operating system. The deterministic query-work checks above
provide the repeatable regression gate; a wall-clock threshold would confound
the storage change with runner load and disk synchronization.

## Remaining work

No tokens are pruned and no global cursor storage cap is enforced. Designing
those limits must preserve expired-token answers and disclose recovery behavior.
Streaming reads do not make visibility filtering constant work. The reader's
256-record cap and older-withdrawal detection remain separate
[return and freshness work](CIVIC_FRESHNESS.md). No deployed service changed.
