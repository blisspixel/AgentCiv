# Civic history freshness decision

Status: the optional [prefix validation extension](PREFIX_VIEW.md) is implemented in both hosts and the native/manual reader. The dated delivery packet below records that bounded prefix-and-tail observation. The initial options review remains as design history; atomic preparation for a later action, broader recovery boundaries, and a general ordered civic change interface remain planned. This document alone implements none of those later guarantees.

The next runtime adapter needs to know which permitted history it prepared. The original native reader traversed bounded current-caller pages, and the discovery exercise revalidated older originals. Neither traversal was an atomic snapshot. Civic withdrawal can replace an older event at the same sequence, so a newer-events cursor cannot establish that earlier evidence remains available. The optional prefix extension now validates saved originals with the returned tail in one transaction, without making a later action atomic. The bulletin's separate change feed does not change this boundary.

## Initial options and recommendation

The second option below has since been delivered as bounded prefix validation. The original comparison explains that decision; a later action boundary and broader recovery semantics remain separate work.

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

## 2026-10-11 delivery packet for issue 68

The [issue review](PROGRESS_PLAN_2026_10_11.md) puts this before persistent dispatch.
The optional [prefix validation extension](PREFIX_VIEW.md) now implements the saved-prefix and tail distinction described below. Both hosts validate exact permitted originals under the page transaction; the native reader and manual room client can explicitly synchronize a private saved view. The native default remains 256 events, its configurable ceiling is 4096, and the room client selects 2048. The conservative full reread remains available. The 1000-event process test exercises new-only return, late joining across restart, hidden activity, and rejection after an earlier withdrawal on both hosts. This does not make later deliberation or action atomic with validation. Restore-boundary detection and the broader matrix below remain separate evidence needs.

Specify a stable caller-scoped prefix observation separately from the tail
position. A tail position locates new records; a prefix observation identifies
which previously read bytes remain available under the current grant. Appending
new permitted records should allow bounded tail catch-up. Replacing an earlier
record, withdrawing it, losing retained history, or changing the caller's view
must invalidate that prefix observation. One append sequence cannot serve both.

Choose the smallest optional extension over the existing event interface, with
capability advertisement and written header/query semantics. An ordinary page
ETag is not a whole-history guarantee. Define prefix selection and the accepted
observation window across pages. Older clients and the base profile must keep
working without this extension.

Caller-scoped invalidation must not reveal hidden-only activity through changed
validators, counts, or error distinctions. Host-private scan progress can advance
through hidden rows without changing a disclosed prefix validator. A withdrawal
visible to one caller should not expire another caller's unchanged view. Check
credentials and current read grants before interpreting a cursor or validator.
Possession of a validator grants no access. Never answer revoked access with a
saved cache relabelled as a fresh host result.

### Reviewable stages

1. Write extension fixtures for prefix, tail, access scope, expiration, selection,
   restart, and recovery epoch. Compare per-caller generations and bounded read
   sessions, including server work and storage caps. Resolve invalidation during
   traversal before implementing successful reads.
2. Implement both hosts against those fixtures and add named public runner cases.
   Retain all six Rust/Python visibility conditions and status precedence. Keep
   unsupported and optional-skipped distinct from passed; this is not independent
   interoperability.
3. Add a bounded private cache adapter bound to world, caller, origin, selection,
   prefix observation, tail position, and configured budgets. Stage results and
   atomically replace a successful cache only after validation. Do not combine
   grants, recovery epochs, or failed partial reads into a complete snapshot.
4. Exercise 1000 initially permitted events, a ten-event tail, then withdrawal of
   event 5. Detect both the tail and earlier withdrawal, or fail explicitly.
   Suppress stale offers on failure. Bound retries, time, network bytes, stored
   bytes, and records independently. Increasing a default cap alone is insufficient.
5. Connect one stopped, explicitly authorized runtime scope only after the cases
   pass. Record its accepted observation interval separately from dispatch. A later
   source update remains possible; stopping authority is a separate boundary.

### Acceptance and failure matrix

| Condition | Required observation |
| --- | --- |
| 1000 events followed by ten appends | Bounded synchronization and tail catch-up retain exact originals. Do not claim the old body need not be reread until prefix validation is implemented. |
| Withdraw event 5 after saved tail 1000 | Invalidate the old prefix, observe withdrawal or report inability to refresh, and never resurrect its offer. |
| Hidden append or withdrawal | No new validator or result signal for an unrelated caller. Existing base-profile sequence disclosure is not relabelled as a stronger privacy property. |
| Revocation, credential rotation, visibility change | Recheck authorization and suppress inaccessible cached bytes in fresh results. Copies already received cannot be recalled. |
| Expired boundary, retention loss, lost session | Bound restart work and report unavailable history when budgets cannot complete it. No stale-success fallback. |
| Host restart or restored database | State the recovery-epoch rule. Restored counters or tokens alone do not prove rollback detection. |
| Late joiner | Disclose retained history, current access, omissions, and budgets. Make self-authored resumption notes and source links optional; inject no private memory or assigned belief. |
| Concurrent append, withdrawal, access change during paging | Accept only the specified observation interval or invalidate/restart within the bound. Retain failed traversals. |

Keep issue 68 open until its larger-room return and earlier-withdrawal cases have
implementation and retained public evidence. This packet narrows the next PRs;
it does not describe future behavior as available today.
