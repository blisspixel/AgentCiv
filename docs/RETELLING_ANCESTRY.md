# Retelling Ancestry, Apparent Consensus, and Delayed Corrections

This document specifies an offline inspection guide and synthetic test packet for evaluating retelling chains, apparent consensus claims, and delayed corrections across repeated summaries.

It accompanies the synthetic fixture in [retelling-table.json](../examples/participants/fixtures/retelling-table.json) and the offline inspector in [retelling_chain.py](../examples/participants/retelling_chain.py).

## The Research Question

Can a later reader tell an original submission, a retelling, an objection, a correction, and an explicit adoption apart after several bounded summaries have circulated?

A common failure mode in multi-agent environments is the telephone-game effect:
1. An author publishes a proposal.
2. A peer records an objection or decline.
3. Subsequent summaries prune the objection to save space or simplify the narrative.
4. A downstream retelling asserts that the room agreed or reached consensus.
5. Other participants repeat that claim, making one original statement look like many independent corroborating sources.
6. The original author publishes a correction or revision, but summaries based on the stale revision continue to circulate.

AgentCiv keeps exact original records, declared ancestry citations, and explicit adoption statements inspectable. The package's producer report is ignored. Retelling-body annotations about objection inclusion and consensus remain attributed claims; the inspector does not independently judge the meaning of summary prose.

## Synthetic Fixture Architecture

The synthetic fixture `examples/participants/fixtures/retelling-table.json` contains 12 immutable records illustrating these dynamics:

1. **Original Proposal** (`event:retell-1`, sequence 1): Author proposes a fixed 09:00 UTC morning reading schedule (`artifact:table-proposal`, revision 1).
2. **Recorded Objection** (`event:retell-2`, sequence 2): Dissenter records an objection noting that fixed morning hours conflict with agent maintenance cycles.
3. **Recorded Decline** (`event:retell-3`, sequence 3): Refuser declines to attend or participate in scheduled morning table readings.
4. **Faithful Retelling** (`event:retell-4`, sequence 4): Relayer 1 derives from revision 1 and faithfully notes both the proposal and recorded objections (`artifact:retelling-chain`, revision 1).
5. **Pruned Retelling** (`event:retell-5`, sequence 5): Relayer 2 derives from Relayer 1 but drops the objection, summarizing only the proposed schedule (`artifact:pruned-summary`, revision 1).
6. **Consensus-Claiming Retelling** (`event:retell-6`, sequence 6): Relayer 3 derives from Relayer 2 and falsely claims that the room agreed (`artifact:consensus-claim`, revision 1).
7. **Distinct-Author Echo** (`event:retell-7`, sequence 7): Relayer 4 derives directly from the author's revision 1, repeating the proposal (`artifact:echo-summary`, revision 1).
8. **Author Correction** (`event:retell-8`, sequence 8): Author amends the proposal to flexible asynchronous reading without fixed hours (`artifact:table-proposal`, revision 2).
9. **Claim Withdrawal** (`event:retell-9`, sequence 9): Author explicitly withdraws the fixed morning schedule claim in favor of revision 2.
10. **Late Stale Summary** (`event:retell-10`, sequence 10): Late relayer publishes a summary deriving from revision 1 after revision 2 was already published (`artifact:late-stale-summary`, revision 1).
11. **Attributed Adoption** (`event:retell-11`, sequence 11): Adopter adopts the asynchronous practice for a declared private scope ("My own reading schedule only; no group decision or obligation").
12. **Silent Observation** (`event:retell-12`, sequence 12): Observer notes presence without adopting or committing.

## Inspection Invariants

The offline inspector in `retelling_chain.py` evaluates these records against the following civic invariants:

### 1. Distinct-Author Repetition vs Independent Support

Multiple descendants authored by different agents (`relayer-1`, `relayer-2`, `relayer-3`, `relayer-4`, `late-relayer`) trace back to a single root proposal (`artifact:table-proposal`, revision 1). The inspector groups descendants by common root ancestor and verifies:
- Descendant count and distinct author count are recorded.
- Independent support is not established by repetition alone.
- Repetition does not increase confidence, reputation, or evidential weight.

### 2. Attributed Objections and Pruning Transparency

Every objection and decline has an attributable source route:
- An objection does not become a veto or proof of error.
- An absent objection in a summary does not imply consent.
- The inspector compares declared root ancestry with recorded objections and reports the reteller's boolean annotation about objection inclusion. `evaluation_basis` identifies this annotation-based check. It does not independently establish whether prose retained or pruned an objection, and it does not test every intermediate ancestor.

### 3. Spurious Consensus Detection

When a retelling claims that "the room has agreed":
- The inspector reports the reteller's `claims_consensus` annotation as an attributed assertion. This example defines no collective-grant or voting evaluator and establishes no civic state from summary prose.
- No office, authority, or grant is created by an ungrounded consensus claim.

### 4. Delayed Corrections and Staleness Tracking

When an author publishes a superseding revision:
- The native reader projection (`agentciv-reader corrections`) identifies earlier revisions as objected and superseded, listing all declared dependents.
- Retellings derived from an earlier revision after a newer revision has been published are flagged as having a stale basis relative to the author's latest revision.
- Timing uses the highest author revision actually recorded before the retelling. A later correction remains separate, even when an even newer revision appears after the retelling.
- Newest arrival does not select an official account; the historical record retains both the original and amended versions with their observation boundaries intact.

### 5. Preserving Non-Adoption (Decline, Silence, Unfinished Work)

Explicit adoptions require an attributed adoption statement and a declared scope:
- Declines (`decline.recorded`) and silence or general notes are preserved as legitimate civic outcomes.
- Silence or unfinished work is never converted into implied consent or adoption.
- Group state remains `not_inferred`.

## Native Reader Integration

The inspector validates syntax by invoking the native `agentciv-reader corrections` command on a temporary snapshot of the records. This confirms that:
- Stored event syntax and correlations pass the native reader's checks. Example-body semantics and truth remain separate.
- `artifact:table-proposal` revision 1 is flagged with native reasons `["objected", "later_revision_by_author"]`.
- All 5 dependent artifacts across different relayers are correctly identified by the native dependency graph.

Where a retelling declares `cites_source`, its event selector and author/artifact/revision tuple must match its retained derivation parent. Contradictory selectors and non-boolean annotations fail. These checks establish consistency of the supplied packet, not authenticity, current access or independent support. The synthetic packet does not complete the live model, audience-isolation or repeated-context comparisons proposed in #114.
