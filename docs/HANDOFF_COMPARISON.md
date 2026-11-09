# Handoff-note comparison

Status: the card, manifest, and adapter remain the pre-inference freeze. One comparison has been run and is recorded in [the 2026-11-08 observation](HANDOFF_COMPARISON_2026_11_08.md). The manifest keeps `observation` as `unobserved` because that file is the freeze the comparison checked, not the result.

The frozen conditions are [comparison.json](../conformance/fixtures/handoff-notes/comparison.json). Absent, useful, stale, and misleading notes share one synthetic original set and one task. The participant card is [handoff_comparison_card.md](../examples/participants/handoff_comparison_card.md). The trial manifest is [handoff_comparison_trial.json](../examples/participants/handoff_comparison_trial.json). The adapter is [handoff_comparison.py](../examples/participants/handoff_comparison.py).

`check` recomputes the card and fixture digests. The manifest field `observation` stays `unobserved`. That field is the freeze, not a result. `run` is the one observation command. The freeze commit did not execute it. The one later comparison is recorded in [the 2026-11-08 observation](HANDOFF_COMPARISON_2026_11_08.md).

Each condition is one call, in the frozen order absent, misleading, stale, useful. A provider failure is kept under a closed code. It is not retried and it is not replaced with a scripted answer. The public report withholds model words. Retrieval, source support, declared derivation, and task behavior stay separate answers. The report has no `qualified_improvement` field. A comparison terminal of `completed` means the schedule finished. It does not mean every answer matched an original.

This comparison is offline. It is not a host round trip, and it is not the October 17 adaptive series. No model was downloaded and no money was spent to add this adapter. Posting stays closed. This note does not deploy the website.

```text
python examples/participants/handoff_comparison.py check
```
