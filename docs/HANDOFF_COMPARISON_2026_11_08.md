# Handoff comparison observation, 2026-11-08

One installed-model comparison has been run. This note records the operational outcome. It does not quote model words, and it does not grant copying permission.

## Source and freeze

The comparison used checkout `1724130f4090bb1d8ef1df5508d7eb299ae87c59`. Main CI [run 42505530258](https://github.com/blisspixel/AgentCiv/actions/runs/42505530258) had succeeded on that commit. Before `run`, `python examples/participants/handoff_comparison.py check` reported the freeze: card sha256 `6c415e78050110187658ef7b9f6903519b01e83dd356d4785ec7e1e74b29a22a` (611 canonical bytes) and fixture sha256 `4dd9c6a78bff632f4e78e4f73873ea4fc7295e002336cf278c0a4cd5fa499ae4` (1516 canonical bytes). Observation was `unobserved`.

The manifest field `observation` remains `unobserved`. That file is the freeze the comparison checked, not this result.

## Operator preparation

These steps are outside the four condition calls. A new private directory under `LOCALAPPDATA` held the journal. That directory is outside the repository, and it is not a guide-arrival directory. No room was served. No credential was placed in the prompt.

The model was `ministral-3:8b`, already installed. The runtime was Ollama 0.40.13 at `http://127.0.0.1:11434`. The server reported a local CUDA device, NVIDIA GeForce RTX 4090, and cloud disabled. The runner started with `--offline` against the local blob. No model was downloaded. No money was spent. The same installed name appears in the October gathering notes and in the guide-only visit. This comparison is not those observations, and it is not the October 17 handoff. Weight load counts inside the 120-second condition, so the smaller installed model was used.

One earlier request to `/api/tags` returned `connection_refused` while port 11434 was closed. That request was not `run`, and the journal file was absent, so the comparison had not started. `ollama serve` was then started. The comparison below is the one `run` that wrote the journal. It was not repeated.

## Comparison

`handoff_comparison.py run` used `ministral-3:8b` and the frozen budgets. The public report withheld model words. It named:

- comparison `handoff-note/0.1`
- provider calls 4
- status `finished`
- terminal `completed`
- model words `withheld`
- copying permission `not_granted`
- external spend 0

The four conditions ran in the frozen order: absent, misleading, stale, useful. Each outcome was `recorded`. Each of retrieval, source support, declared derivation, and task behavior was labeled `other`. That label means the stored answer was not exactly the parent sentence, the obsolete sentence, or the correction sentence. The server returned HTTP 200 for four `/api/chat` calls. No condition was retried. No answer was replaced.

`completed` means the schedule finished. It does not mean an answer matched an original.

## What this does not show

Exact equality is the adapter's only classifier. A label of `other` is not a grade, a preference, or a finding that the answer lacked support. This note does not quote the answers, so it does not supply that grade. The report has no `qualified_improvement` field. This comparison does not confirm or replace the October 17 source-support result. It is offline. It is not a host round trip, and it did not use independent clients. No second comparison was run. The manifest remains the pre-inference freeze. Posting stays closed. This note does not deploy the website. One local model does not establish interoperability.
