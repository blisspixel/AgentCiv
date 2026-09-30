# Local decision-loop validation: 2026-09-30

The optional feedback loop completed one local Qwen handoff after repairing a
missing target citation. Its paired one-shot run failed on that validation rule.
Earlier observations included both feedback failures and a successful one-shot
handoff. These cases show local execution and failure reporting. They do not
establish that feedback improves reliability or that the guides are useful.
The [earlier four-trial record](LOCAL_VALIDATION_2026_09_30.md) remains unchanged.

## Committed functionality pair

Both final trials used source commit
`460c5c8fe9b5a06369af3ab3f5db0f2e26a8b5a9`, a clean source checkout, the actual
Python loopback host, and installed `qwen2.5:14b`. Each report records unchanged
source hashes during execution. Ollama was version 0.34.3; the model digest was
`7cdf5a0187d5c58cc5d369b255592f7841d1c4696d45a8c8a9489440385b22f6`, quantized
as Q4_K_M. Reports preserve those observations separately from later CI.

The operator supplied the task, interface reference, A1/B1/A2/B2/C1 schedule,
credentials, and permitted original records. Sampling used temperature 0.4,
8192 context tokens, and base seed 42 plus the participant turn number. Each
turn shared a 1024 output-token allowance and a 120-second decision deadline.
The enclosing participant process limit was 180 seconds. Inventory, publication,
and verification were outside the decision deadline. Proxies, redirects, prompt
truncation, and context shifting were disabled. All new test scratch and build
directories used D: for the committed trials and release checks.

| Condition | First valid / started turns | Eventually valid / started turns | Model requests | Reported output tokens across started turns | Outcome |
| --- | --- | --- | --- | --- | --- |
| [One attempt](../conformance/evidence/local-loop-2026-09-30/committed/qwen-one/failure.json) | 3 / 4 | 3 / 4 | 4 | 657 | B2 failed because its target was not also cited. No newcomer turn was launched. |
| [Up to three attempts](../conformance/evidence/local-loop-2026-09-30/committed/qwen-feedback/report.json) | 4 / 5 | 5 / 5 | 6 | 1096 | B2 repaired the citation on its second attempt. C1 published after restart. |

The first prompt hash was identical:
`96c26f80351fa166b3f851299719e61e4c7289948e6979585a2f9c4ba9fbb72f`.
First responses differed, and later source records and randomly assigned event
IDs diverged. Feedback adds input tokens and context work; the controls do not
equalize total tokens, GPU compute, or latency. One pair, sequential launch order,
and a seed do not supply a causal reliability comparison.

The [successful observations](../conformance/evidence/local-loop-2026-09-30/committed/qwen-feedback/observations.json),
[archive](../conformance/evidence/local-loop-2026-09-30/committed/qwen-feedback/history.json),
and [B2 journal](../conformance/evidence/local-loop-2026-09-30/committed/qwen-feedback/attempts/b-2.json)
preserve the accepted decisions and failed attempt's hash, rule, usage, and
publication state. The invalid candidate was not published or rewritten. Exact
prompts, candidates, and native provider traces remain private local evidence.

## Preliminary observations retained

Six preceding trials used a dirty source checkout at parent commit `d8d3917`,
with file hashes and no source changes during each run. They preceded final
policy separation and malformed-encoding handling. Their hashes identify that
checkout; the parent commit alone does not reproduce those uncommitted changes.
They are exploratory records, not a clean-source acceptance result.

| Condition | Completed turns | Result |
| --- | --- | --- |
| [Ministral one attempt](../conformance/evidence/local-loop-2026-09-30/preliminary/ministral-one/failure.json) | 4 | C1 failed `invalid_text`. |
| [Ministral feedback](../conformance/evidence/local-loop-2026-09-30/preliminary/ministral-feedback/failure.json) | 0 | A1 failed `invalid_text` on all three attempts. |
| [Qwen feedback](../conformance/evidence/local-loop-2026-09-30/preliminary/qwen-feedback/failure.json) | 0 | Provider HTTP failure, with generation usage unmeasured. |
| [Qwen one attempt](../conformance/evidence/local-loop-2026-09-30/preliminary/qwen-one/failure.json) | 0 | Provider HTTP failure, with generation usage unmeasured. |
| [Qwen feedback after cache cleanup](../conformance/evidence/local-loop-2026-09-30/preliminary/qwen-feedback-after-cleanup/failure.json) | 4 | B2 repaired a missing citation; C1 failed it on all three attempts. |
| [Qwen one attempt after cache cleanup](../conformance/evidence/local-loop-2026-09-30/preliminary/qwen-one-after-cleanup/report.json) | 5 | C1 published after restart. All choices were revisions. |

A nearly full C: drive coincided with the two provider failures and failed native
builds. Native coverage-cache cleanup restored build success. The provider
failures retained no diagnostic establishing their cause, so storage exhaustion
is a plausible explanation, not an attested provider finding. Retries were new,
disclosed trials; they did not replace those failures.

## Gates and evidence boundaries

Final local checks passed formatting, warnings-denied Clippy, the repository
checker, and strict mypy on 16 maintained Python files. Rust line coverage was
88.85%; maintained Python statement coverage was 88.24%, including subprocesses.
The decision-loop module had 100% statement coverage. Suites passed 18 host,
74 participant, and 9 validation-adapter tests. Fixture tests cover shared
budgets, deadlines, malformed Unicode, invalid decisions, private storage
failure, reflected credentials, lost responses, changed readback, and at most
one publication. They do not establish model decision quality.

The [release HTTP matrix](../conformance/evidence/local-loop-2026-09-30/local-validation.json)
used the same clean commit with unchanged sources and completed all twelve
visibility and lifecycle invocations: 350 passes, zero failures, and ten optional
hidden-target skips on `members` worlds. Restricted worlds passed those cases.
This remains partial profile evidence from two hosts maintained in this repository.

Published JSON uses LF line endings. The [archive manifest](../conformance/evidence/local-loop-2026-09-30/archive-manifest.json)
records original and published checksums; parsed JSON is identical, and original
local files remain retained. Exports include only operator-selected synthetic
shared records and public diagnostics, not credentials, databases, private
candidates, or native model traces. This publication does not establish a
general export permission or a handoff-package contract. External spend was $0.

The guides mostly repeated general advice. The final newcomer cited four
surviving revisions and published its own chain, but produced no independently
verified improvement. No model objection or decline was recorded. A later
publication is narrower than useful inheritance. The next [acceptance case](FIRST_EXPERIMENT.md#next-concrete-acceptance-case)
requires a defect that demonstrably fails before a correction and passes after
it, an inspectable objection and corrected source, and a separate caller using
the successor's artifact. Successful JSON and citation presence cannot substitute
for those outcomes.
