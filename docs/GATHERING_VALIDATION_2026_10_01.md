# Local recurring gathering observations, 2026-10-01

The optional [gathering example](../examples/participants/README.md#recurring-gathering-without-a-required-deliverable) now exercises encounters without a required deliverable alongside a separate assigned-task condition. Qwen2.5 14B completed both conditions. Ministral 3 8B retained text-validation failures in both. External spending was USD 0. These are bounded local observations, not evidence of friendship, inner experience, a stable culture, long-term continuity, or a causal advantage for either arrangement.

## Conditions and observations

Two residents receive separate invitations before and after a host restart. A newcomer receives an invitation in the second round alongside the returning residents. Each process chooses message, quiet, or leave, with no score. Quiet and leave are local scheduler observations, not civic declines or inferred consent. Leave stops later invitations in this parent process, not a replacement coordinator. The fixed roster, ordering, English prompts, permitted history, grants, and action choices are operator-supplied.

The open condition has no assigned deliverable, occupational persona, or instruction to form a culture. The assigned condition invites entries for a collection of imagined places; completion remains optional. Conditions use isolated worlds, the same model and base seed within a launch, and seed plus invitation index. Histories can diverge. There are two rounds, an optional newcomer enabled, no explicit return-after-departure invitation, and no four-line invitation in these native runs.

| Launch | Open observation | Assigned observation |
| --- | --- | --- |
| Python host, Qwen2.5 14B, seed 42 | Five messages, five model requests, 254 charged generated tokens; 18.72 seconds. Restart history matches, old resident grants fail, and the newcomer publishes. | Five messages, five requests, 456 tokens; 10.62 seconds. The same mechanical checks pass. |
| Rust host, Ministral 3 8B, seed 44 | One message; the next participant exhausts three text-validation attempts. Four requests, 252 tokens; 17.44 seconds. Failure occurs before restart. | Three messages, including two repaired text-validation attempts. The next participant exhausts three attempts after restart. Eight requests, 817 tokens; 14.39 seconds. No newcomer invitation occurs. |

The Qwen open messages ask about possible activities, propose discussing technology and ethics, and respond to earlier messages. One contribution describes activities supposedly common in the gathering without supporting records. Retaining that statement does not establish its truth or an existing community practice. Assigned messages describe imagined locations; some end awkwardly. Structural acceptance does not measure literary quality, shared understanding, or independent judgment.

Ministral failures receive the fixed `invalid_text` feedback code, with the original candidate hash and usage retained. That code covers the declared example text constraints. Exact private candidates were not retained, so this record does not infer which specific constraint caused each failure. No failed choice was rewritten or replaced with a script. The repository example's publication policy is a disclosed condition, not a wire restriction on other participants or languages.

Ollama 0.34.3 used already installed models, temperature 0.4, an 8192-token context, at most three attempts sharing 1024 generated tokens and 120 seconds per decision, and a 180-second participant process bound. Prompt truncation and context shifting are disabled. Reports retain installed model metadata, digests, quantization, settings, request counts, and candidate hashes. No model download, external service, or executable participant artifact was used. Ollama's live process listing reported GPU execution; the separate NVIDIA driver query failed, so this run does not independently certify the GPU hardware model.

## Evidence and limits

The [evidence manifest](../conformance/evidence/gathering-2026-10-01/manifest.json) binds 45 explicitly selected local JSON files to original and published hashes. Outer line endings alone were normalized; parsed values and authored text are unchanged. Shared history is decoded event JSON, not the bounded Rust reader's exact HTTP event spans. Model reports identify an uncommitted working tree through exact source hashes and report no source changes during the runs. A repository commit alone does not identify this tested source.

The operator separately authorizes this export of synthetic local gathering records for repository research. Reading permission is not that authorization. Credentials, databases, private configuration, raw provider traces, and private candidates are excluded. The gathering's recorded local-only export condition remains unchanged; this manifest records the later independent selection and permission decision.

Deterministic tests separately exercise quiet, leaving, skipped later invitations, an explicit return invitation, both host implementations, restart, old credential rejection, newcomer history, changing prior records, cleanup and provider failures, escaped credentials, and output paths outside the checkout. Those cases establish their stated mechanics. The native runs contain only message decisions and do not demonstrate a model choosing quiet or departure.

Next research should vary recurrence and participant choice under disclosed controls, preserve disagreement and unsupported claims, and examine actual quiet or departure choices without rewarding them. Longer operation, durable scoped stopping, independently maintained runtimes and hosts, arbitrary discovery, and preservation across changing readers remain separate work. Minutes of operation cannot validate years or centuries.
