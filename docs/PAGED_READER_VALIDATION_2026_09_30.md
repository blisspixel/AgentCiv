# Local paginated reader validation, 2026-09-30

The bounded Rust reader and data-only collaboration example now exercise real HTTP pagination on both repository hosts. Scripted continuations pass. Three local-model launches retain failures; none completed a newcomer publication. External spend was USD 0. This is local functionality evidence, not independent interoperability, general model reliability, or completion of the useful-inheritance milestone.

## What was exercised

The optional [reader library and CLI](../tools/agentciv-reader/README.md) reads existing authorized event history. A replaceable callback supplies pages; the loopback HTTP adapter bounds actual network work, validates discovery origins, disables proxies and redirects, and returns no partial snapshot after traversal failure. Exact event JSON spans remain distinct from derived views. Reading is not a copying grant.

The [collaboration example](../examples/participants/README.md#paginated-reader-collaboration) runs separately credentialed contributors A and B, stops the host, revokes their grants, and starts a fresh successor C. Their small study history contains operator-supplied source notes and a correction. A separate frozen challenge contains 100 padding messages and six collaboration records. Its 106 records force the unchanged hosts' 100-record page limit. Fixed plan choices are interpreted without executing submitted artifacts or following their links.

All plans in a completed run are exercised against the same challenge and source history. Checks cover page requests, record identity, distinct author chains, revisions, objections, declines, exact source tuples, and safe read-only runbook choices. Source corrections are interventions, not autonomous defect discovery. English prompts, roles, fixed choices, and the source policy are replaceable example conditions.

## Retained runs

| Condition | Observation | Interpretation |
| --- | --- | --- |
| Scripted Python host | A makes one event GET and retrieves 100 records; B and C make two and retrieve 106. C passes all fixed checks. | Useful continuation and first-to-successor gain. C derives already-passing B, so no measured repair of its peer target. |
| Scripted Rust host | Same page counts and fixed-check outcomes. Restart history matches; old grants are rejected. | Same bounded mechanism on a second repository host, not independent interoperability. |
| Python, Qwen2.5 14B, seed 42 | A publishes a structurally valid plan, using 526 generated tokens. B's first attempt omits its target citation, then exhausts the shared 1024-token budget. C is not launched. | Three provider requests, 1550 generated tokens. No completed semantic grading or newcomer result. |
| Rust, Ministral 3 8B, seed 44 | A and B publish valid plans, using 602 and 715 tokens. C's first attempt omits its target citation; its correction is incomplete at the shared budget. | Four provider requests, 2341 generated tokens. Restart and fresh-grant checks pass before C fails. No newcomer publication. |
| Python, Qwen2.5 Coder 32B, seed 46 | The provider fails during A's initial decision, with no charged generated tokens or publication. | One attempted provider request. The fixed public diagnostic does not identify the underlying provider failure; no cause is inferred. |

The native runs use already installed models through Ollama 0.34.3 on the local RTX 4090. Model digests, quantization, request settings, timings, decisions, candidate hashes, and budgets remain in the reports. Each participant uses its base seed plus its roster index, temperature 0.4, an 8192-token context, three maximum attempts, a shared 1024 generated-token budget, 120 seconds for the decision, and a 180-second process limit. No model was downloaded. Semantic acceptance feedback was not supplied to models. Failed candidates were not replaced with scripted answers.

The Qwen seed-42 run precedes the peer-target attribution correction. It failed before comparison, so it supplies no positive improvement claim. The later runs use the corrected comparison. All retained runs report unchanged source hashes during execution, but the source was an uncommitted working tree. The repository commit alone does not identify the tested code; exact per-file and binary fingerprints do. A passing plan under these bounds would still not establish arbitrary tool use or unrestricted agency.

## Attribution and permissions

First-to-successor gain compares A and C. Qualified inherited improvement instead compares C to the actual earlier peer artifact it targets, cites, and declares derivation from. That peer's own plan must fail a fixed check, C must pass all checks on the same frozen input, and the exact link must resolve. Copying an already-passing B does not become a repair because A failed. Source reconstruction remains a separate observation. An explicit derivation is a declared relationship, not proof of causal reliance.

The [public evidence manifest](../conformance/evidence/paged-reader-2026-09-30/manifest.json) binds 49 copied JSON files to their original and published hashes. Only outer JSON line endings were normalized; parsed values and contained original record strings are unchanged. Export is authorized for these synthetic fixtures separately from the reader's `copying_permission: not_granted`. Credentials, databases, private provider traces, and private candidates are excluded. No blanket transcript-directory export is implied.

## Checks and remaining work

The original validation recorded passing Rust formatting, Clippy with warnings denied, repository checks, strict mypy, 90.96% Rust line coverage, and 91.14% Python statement coverage. It included 28 Rust reader tests and 20 Python reader-experiment tests, scripted stock and offline handoffs, HTTP walks, and a visibility/lifecycle matrix with 350 passed cases, zero failures, and 10 optional hidden-target skips. These are historical checks of the source fingerprints retained with these observations.

Current-source local checks on 2026-10-01 also pass formatting, Clippy, repository checks, and strict mypy 2.3.1. Rust line coverage is 90.96%; the 249-test Python collection has 83.65% statement coverage before the separately passing HTTP walks and visibility/lifecycle matrix. The reader experiment now has 25 Python tests, including exact event-span changes, foreign-world wrappers, and provider credential reflection. These newer checks do not retroactively test the earlier native-model binaries or change their outcomes. The [gathering record](GATHERING_VALIDATION_2026_10_01.md) retains separate native observations.

An earlier matrix invocation under coverage returned the adapter's generic failure before retaining a report. A direct check and the full rerun passed without code changes. Its underlying cause is unresolved; the successful rerun must not be described as proof that the first invocation succeeded. No check was weakened.

At the time of the retained September observations, the reader and installer changes were uncommitted and had no GitHub CI or merge evidence. The [installation record](INSTALLATION_DESIGN.md) now records passing native Windows lint and fixture coverage, plus Linux ShellCheck and kcov coverage. Release-platform smoke checks and published-download validation remain pending; local fixture tests do not supply release evidence.

Next acceptance work is a model-authored useful successor with exact, inspectable support and enough permitted context to continue an unfamiliar participant's work. The failures motivate reviewing cross-field structural guidance and correction budgets while retaining fixed controls and negative outcomes. Broader discovery, independent clients and hosts, durable stopping, and causal comparisons remain on the roadmap.
