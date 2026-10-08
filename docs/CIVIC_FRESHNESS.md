# Civic history freshness decision

Status: design proposal after the bounded discovery and durable stopping slices. No new endpoint, validator, snapshot guarantee, or incremental catch-up contract is implemented by this document.

The next runtime adapter needs to know which permitted history it prepared. Today, the native reader traverses bounded current-caller pages, and the discovery exercise revalidates older originals. Neither traversal is an atomic snapshot. Civic withdrawal can replace an older event at the same sequence, so a newer-events cursor cannot establish that earlier evidence remains available. The bulletin's separate change feed does not change this boundary.

## Options and recommendation

| Option | Benefit | Boundary and cost |
| --- | --- | --- |
| Bounded full revalidation, available now | Reuses existing hosts and reader; exposes current permitted originals and older tombstones | Multiple pages can observe different moments; repeat reads cannot prove permanent stability or provide reliable incremental catch-up |
| Optional caller-scoped view validation, proposed next | Lets a bounded traversal detect that its permitted view changed between pages and before reliance | Requires a written extension, authenticated checks, access-aware validators, expiration, bounded restart behavior, and explicit failure handling in both hosts and the public runner |
| Ordered civic change interface, deferred | Could support incremental catch-up over revisions, withdrawals, and access changes | Needs separate ordering, initial synchronization, pruning, missing-boundary, restore, and permission-change semantics; a numeric global counter can disclose restricted activity |

Recommend designing and testing the second option against the existing reader-repair and discovery fixtures before implementing a general change feed. Keep full revalidation as the conservative baseline. An unsupported extension, expired boundary, changed view, or unavailable history must yield a bounded restart from the origin or an explicit inability to establish freshness. A stale successful view must not replace that failure.

[HTTP entity tags](https://www.rfc-editor.org/rfc/rfc9110.html#section-8.8.3) validate a selected representation. Inference for this design: an ordinary page ETag alone cannot certify that several separately fetched pages form one unchanged permitted history. The resource and caller scope must be specified. [SQLite isolation](https://www.sqlite.org/isolation.html) describes transaction snapshots inside a database; it does not create a snapshot across independent HTTP requests without an additional application contract.

## Required decisions before code

Define the view's world, authenticated principal, selection, ordering, access policy, expiry, and recovery epoch. Decide whether a changed view invalidates traversal or retains an explicit read session. A retained session cannot continue serving restricted bytes after an access change or withdrawal contrary to the stated host policy. Current authorization is checked on every request; possession of a validator is no grant.

Do not expose global counters, hidden record counts, or validator changes caused only by inaccessible activity. Review a caller-specific digest of permitted records against a bounded server-side session. Include retention expiry and older-record replacement in invalidation. Disclose the work required to compute each representation, session storage limits, and what happens on host restart or restoration. Do not infer rollback detection from a restored counter.

A final successful validation is an observation at a stated time. It does not prevent the source from changing afterwards, authenticate its claims, authorize copying, or make HTTP history checks atomic with an external process spawn. The runtime stop gate remains a separate authority boundary. An adapter must identify its accepted observation window rather than promise permanent freshness.

## First reviewable packet

Write optional extension semantics and positive/negative fixtures first, with no changes to the current mandatory profile. Then implement a bounded view-check operation in both hosts and add named public cases. Extend the native reader and one explicitly authorized runtime adapter only after those checks establish their boundary.

Exercise an older withdrawal between pages, a corrected source after traversal, an append during traversal, retention expiry, lost cursors, restart, restored state, credential rotation, revoked read access, visibility changes, and hidden-only activity. Verify that restricted views disclose neither original bytes nor a hidden activity signal through counts or validators. Preserve original failed traversals and configured retry budgets. Continue comparing the existing full-revalidation baseline separately.

This packet advances discovery and return without choosing a participant's purpose. A newcomer can inspect an offer, question it, decline, leave, or choose another activity. Persistent dispatch remains a later decision with its own authorization and stopping evidence.
