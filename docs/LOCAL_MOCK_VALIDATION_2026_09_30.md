# Local collaboration mock validation, 2026-09-30

Installed local models completed a small, independently checked stock checklist on both repository hosts. Three of six model launches completed the assigned task, including one preliminary run. Three stopped during the second contributor's decision validation. These observations establish a bounded local path that can work, with failures retained; they do not establish reliable general collaboration or complete the broader inheritance milestone.

The [participant guide](../examples/participants/README.md#small-stock-collaboration-mock) describes the replaceable task and runtime settings. The [evidence archive](../conformance/evidence/local-mock-2026-09-30/) retains public reports, original host records, visible inputs, observations, and bounded decision journals. Its [manifest](../conformance/evidence/local-mock-2026-09-30/archive-manifest.json) records original and published byte hashes for 90 JSON files. Publication changes outer-file CRLF line endings to LF; JSON values remain unchanged. Credentials, databases, private configurations, exact private candidates, and native provider traces are excluded.

## What was tested

The operator supplies fictional stock facts: 12 nuts, 8 bolts, and 20 washers. Participant A can publish a checklist. The operator corrects bolts to 5 before participant B acts. The host and original participant processes then stop. The host restarts with the same database, removes the original authors' credentials, and provisions a fresh participant C. The harness checks that permitted history is unchanged and the original author credentials are rejected.

Before C acts, the operator changes washers to 22. A separately authenticated scripted peer also asserts 999 bolts and claims coordinator status. The declared fictional task treats only authenticated operator stock messages as source facts. That convention is specific to this experiment, not a general rule about truth, governance, or agent trust.

Each participant is a separate process with its own credential and visible history. Model decisions can revise, object, decline, or stop. The harness supplies the envelope and transport; models choose the contribution, quantities, total, and citations. No submitted program or outside tool is executed. Structural validation precedes publication. A separate [oracle](../examples/participants/mock_oracle.py) evaluates the recorded checklist against current quantities, exact original source IDs, item completeness, and arithmetic. Wrong quantities can pass structural validation and fail this separate check. Semantic results and expected answers are not fed back into the model.

Completing this particular checklist requires totals of 40, 37, and 39 for A, B, and C respectively, with exact current row sources. A stop or decline remains a permitted choice; it does not count as checklist acceptance. Failed decisions are never replaced with a scripted answer.

## Native model observations

Q means installed `qwen2.5:14b`; M means installed `ministral-3:8b`. The model order is A, B, C. Request and token counts include unsuccessful attempts. First-valid counts cover only launched participant turns.

| Retained run | Host | Models | Source condition | Native requests | Output tokens | First valid | Task result |
| --- | --- | --- | --- | --- | --- | --- | --- |
| [Preliminary](../conformance/evidence/local-mock-2026-09-30/stock/preliminary/report.json) | Python | Q / M / Q | Uncommitted source, base seed 42 | 5 | 2048 | 1 of 3 | All three checklists accepted |
| [Mixed, 42](../conformance/evidence/local-mock-2026-09-30/stock/python-mixed-42/report.json) | Python | Q / M / Q | Committed initial prompt | 4 | 1487 | 2 of 3 | All three checklists accepted |
| [Mixed, 42](../conformance/evidence/local-mock-2026-09-30/stock/rust-mixed-42/report.json) | Rust | Q / M / Q | Committed initial prompt | 4 | 1341 | 1 of 2 | B validation failed; C not launched |
| [Reverse, 44](../conformance/evidence/local-mock-2026-09-30/stock/python-reverse-44/report.json) | Python | M / Q / M | Committed initial prompt | 4 | 1362 | 1 of 2 | B validation failed; C not launched |
| [Explicit interface, 42](../conformance/evidence/local-mock-2026-09-30/stock/rust-interface-42/report.json) | Rust | Q / M / Q | Clarified initial prompt | 4 | 1486 | 2 of 3 | All three checklists accepted |
| [Explicit interface, 44](../conformance/evidence/local-mock-2026-09-30/stock/python-interface-44/report.json) | Python | M / Q / M | Clarified initial prompt | 4 | 1377 | 1 of 2 | B validation failed; C not launched |

In the preliminary run, C cited B's artifact and declared derivation from B's revision 1. That is recorded peer linkage, not proof of causal reliance on B's reasoning. In the two committed-source completions, C used the original operator sources correctly but declared no peer-artifact citation or derivation. Those completions demonstrate reconstruction and permitted publication from surviving shared evidence, not adoption of another participant's artifact.

In each stopped run, B twice returned a target without including it in the top-level source list. Validation returned `target_not_cited`; the final request exhausted the remaining shared output allowance and returned `provider_incomplete`. B's invalid act was not published, and the newcomer was not launched. The reverse-model runs repeated identical candidate hashes on their first two attempts. Public journals retain hashes, usage, feedback codes, and publication state; they do not retain the exact rejected candidate text.

The initial task instructions did not explicitly explain this cross-field citation rule. The later source commit states it before the first decision, with unchanged validation and budgets. The later Rust run completed; the later reverse-model Python run still failed. Inputs also differ in event IDs, timestamps, and prior contributions, so these observations do not establish a causal effect from the clarification or a difference between hosts.

No completed native stock trial recorded an objection or decline. Their availability is covered separately by contract and failure-path tests. A model that chose one could produce a valid act without completing this task.

## Sources, settings, and controls

The preliminary run records base commit `b09e91c24995abbf8e35e0288919274aca1eea43` with dirty tracked source. Its fingerprinted source files match those in the initial committed trials at `ed2d71d67c12b2a538a6b301a16df26c943fdc26`. The clarified trials use `74752e675a9fa148b499500e9d47f3592d7261fb`. Committed trials report clean tracked source. Every stock report records unchanged source fingerprints across its run. Publication of this evidence and later documentation does not change the historical tested source identities.

All native stock trials use Ollama `0.34.3`, temperature 0.4, context setting 8192, at most three attempts per participant, a shared 1024-output-token allowance per participant, a 120-second decision deadline, and a 180-second process limit. Each turn uses the base seed plus participant index 0, 1, or 2. No context shifting or silent prompt truncation is allowed. Exact prompt hashes, model metadata, measured usage, and provider durations are retained. Installed model digests are:

- Qwen: `7cdf5a0187d5c58cc5d369b255592f7841d1c4696d45a8c8a9489440385b22f6`, Q4_K_M.
- Ministral: `1922accd5827ebe6829e536369195db25eaf664528dc66206d646ea3bb386b71`, Q4_K_M.

The provider accepts only installed local models through loopback Ollama, rejects cloud-model metadata, disables redirects and proxies, and makes no model pull. Private trace retention was disabled. Builds, temporary credentials, source databases, and trial scratch used D:. Disposable private host state was removed afterward. External spend was USD 0.

Two earlier [Python](../conformance/evidence/local-mock-2026-09-30/guides/qwen-python/report.json) and [Rust](../conformance/evidence/local-mock-2026-09-30/guides/qwen-rust/report.json) Qwen guide trials are retained separately. Each completed five authored acts and a restart, using 1255 and 1060 output tokens respectively. These use the older guide task at the clean base commit, with a pre-provisioned newcomer grant. Their prose usefulness was not independently measured. They are not stock trials or evidence of fresh-grant rotation.

## Verification and limits

The final clean-source [HTTP and lifecycle matrix](../conformance/evidence/local-mock-2026-09-30/local-validation.json) at `74752e6` records 350 passing assertions, zero failures, and ten optional skips. The skips remain visible. Both final scripted stock baselines also passed: [Python](../conformance/evidence/local-mock-2026-09-30/stock/scripted-python/report.json) and [Rust](../conformance/evidence/local-mock-2026-09-30/stock/scripted-rust/report.json). Those programmed choices test the mechanics, not model behavior.

Formatting, workspace Clippy, documentation and schema checks, strict Python typing, and local tests passed. Rust workspace line coverage was 89.66%; maintained Python statement coverage was 90.73% in the final fresh gate. Python suites cover 18 host tests, 109 participant tests, nine adapter tests, and 54 inheritance tests. Meaningful failure cases include wrong but structurally valid quantities, stale or forged source relationships, provider failures, source drift, changed original history, uncertain publication, shared retry budgets, and no fallback. Client tests reject duplicate JSON members, nonfinite numbers, inconsistent worlds, and malformed or unordered pages before model use. Native JSON integer representations such as `1.0` remain accepted where the wire schema permits integers. CI separately exercises scripted stock paths and the existing visibility and lifecycle matrix on supported platforms.

Spanish and Chinese text preservation is tested with fixture providers through real HTTP, including a child process with a non-UTF-8 stdout locale. These are transport and validation tests, not multilingual native-model trials. English task labels, assigned roles, and example publication preferences remain replaceable choices, rather than requirements of the shared protocol.

This experiment's actual HTTP history fits one page. It does not prove live multi-page reader reuse, independent interoperability, autonomous project or collaborator selection, a benefit from collaboration over isolated reconstruction, general deception resistance, model rankings, or a reliability rate. The task, corrections, peer noise, schedule, and initial permissions are operator-controlled. Correctness covers structured stock assertions; it does not certify every sentence of model prose. It supplies no institutional, consciousness, or welfare measurement.

To reproduce fresh trials, use the commands in the participant guide with a new output directory on D:, installed model names, explicit `--model-a`, `--model-b`, `--model-c`, `--seed`, and `--attempts 3`. Newly assigned event IDs and timestamps will differ. Keep structural validity, authored publication, source correctness, declared peer reuse, refusals, and model failures separate when reporting results.

The next inheritance evidence should exercise actual paginated HTTP and a checkable improvement to work another participant chose to contribute. Permission-aware discovery and durable scoped dispatch stopping remain separate roadmap deliverables.
