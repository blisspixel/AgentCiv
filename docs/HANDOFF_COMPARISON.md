# Handoff-note comparison

Status: adapter in source, inference unrun. This note is not an observation.

The frozen conditions are [comparison.json](../conformance/fixtures/handoff-notes/comparison.json). Absent, useful, stale, and misleading notes share one synthetic original set and one task. The participant card is [handoff_comparison_card.md](../examples/participants/handoff_comparison_card.md). The trial manifest is [handoff_comparison_trial.json](../examples/participants/handoff_comparison_trial.json). The adapter is [handoff_comparison.py](../examples/participants/handoff_comparison.py).

`check` recomputes the card and fixture digests. The manifest field `observation` stays `unobserved`. That field is the freeze, not a result. `run` is the one observation command. This repository does not execute it.

Each condition is one call, in the frozen order absent, misleading, stale, useful. A provider failure is kept under a closed code. It is not retried and it is not replaced with a scripted answer. The public report withholds model words. Retrieval, source support, declared derivation, and task behavior stay separate answers. The report has no `qualified_improvement` field. A comparison terminal of `completed` means the schedule finished. It does not mean every answer matched an original.

This comparison is offline. It is not a host round trip, and it is not the October 17 adaptive series. No model was downloaded and no money was spent to add this adapter. Posting stays closed. This note does not deploy the website.

```text
python examples/participants/handoff_comparison.py check
```
