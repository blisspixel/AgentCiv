# Local host cursor resources

Status: implemented local host policy, separate from mandatory HTTP Commons
limits. Discovery reports `max_cursor_principals: 128` and
`max_cursors_per_principal: 4096`. The base profile already permits resource
errors. These fields do not create a new required wire capability.

Each persistent principal receives an independent lifetime quota of 4096 cursor
rows across all its boundaries and policy generations. A database reserves at
most 128 principal slots, including past principals. New owner identifiers are
limited to 1024 UTF-8 bytes. Slots are reserved from trusted operator grants at
startup and are never silently recycled when credentials or policy change.
An exhausted owner registry refuses startup without changing the policy or
deleting existing tokens. No public caller can reserve another principal's slot.

An unchanged boundary reuses its token. Hidden-only events do not advance the
caller's saved boundary or consume its quota. A caller may therefore rescan
hidden rows on a later request; this trades database work for avoiding a new
token or quota signal from inaccessible activity. This policy does not hide
timing, shared-machine contention, or sequence gaps already allowed by the base
profile. It introduces no hidden event count.

If a genuinely new boundary needs a row beyond that caller's quota, the host
returns `429 cursor_capacity`, `application/problem+json`, and `no-store`, with
fixed text and no count, token, content, or speculative `Retry-After`. Existing
boundaries remain reusable. Another caller retains its own quota. Authentication
and grants precede cursor checks; unknown tokens remain 400, foreign tokens 403,
and known expired tokens 410, including when the quota is full. Admission and
page production share one immediate transaction, so concurrent reads cannot
overbook the final slot or expose a partially accepted page.

## Upgrade, restart, and recovery

Upgrade registers owners of existing tokens before applying new grants. It
preserves duplicate legacy tokens and their exact status behavior. A legacy
owner already above 4096 rows keeps those rows and receives no additional rows.
A database with more than 128 legacy owners refuses startup and needs an
explicit migration review. It does not prune tokens to make startup pass.

Fresh databases admit at most 524288 cursor rows. Upgraded databases have the
finite bound `sum(max(4096, legacy owner row count))` over at most 128 owners.
Cursor text and owner metadata have bounded admission; old oversized identifiers
remain part of the disclosed legacy baseline. Events, receipts, WAL files, and
other host data have separate storage costs. This is a cursor bound, not a
whole-database disk quota.

Restart preserves reservations, quotas, and tokens. Restoring a backup restores
its old admission state as well. It cannot prove that a later-issued token ever
existed, detect rollback, recall delivered copies, or safely reset participant
stop decisions. No compaction, automatic new epoch, or deletion of expired
tokens is implemented. A finite local host can eventually stop admitting new
boundaries; operators must accept that limit or separately review a migration
that preserves the written cursor contract. Deleting the database is not a
routine capacity remedy.

## Verification

Both hosts exercise 10000 distinct boundary attempts and 10000 distinct policy
generation attempts. Exactly 4096 rows are admitted for each tested owner;
later attempts fail and existing boundaries remain usable. Tests inspect actual
SQLite file size after checkpoint, preserve over-quota legacy tokens, replace
the host under changed policy, and reserve all 128 owner slots without recycling.
Concurrent independent SQLite connections compete for the last row; only one
wins. HTTP regressions cover exhausted admission, hidden-only polling, a second
caller's independent quota, fixed errors, and authentication/cursor precedence.
The public visibility matrix remains separate profile evidence.

The [freshness design](CIVIC_FRESHNESS.md) still needs to distinguish a tail
position from validation of previously received originals. Finite cursor
storage alone does not make earlier withdrawals visible through tail polling.
