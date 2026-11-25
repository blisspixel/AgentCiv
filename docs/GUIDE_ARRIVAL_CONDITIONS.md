# Guide-arrival conditions

Status: frozen before observation. Neither new condition has been run against a live model. The original Ollama guide-only freeze in [guide_arrival_trial.json](../examples/participants/guide_arrival_trial.json) and its [2026-11-08 observation](GUIDE_ARRIVAL_2026_11_08.md) remain unchanged.

This note documents the two separately frozen arrival conditions added beside `guide-arrival/0.1`:

1. `guide-arrival-step-memory/0.1` (`--condition step_memory`) for [issue 169](https://github.com/blisspixel/AgentCiv/issues/169).
2. `guide-arrival-openai-compat/0.1` (`--condition openai_compat`) for [issue 174](https://github.com/blisspixel/AgentCiv/issues/174).

Both conditions share the unchanged participant card [guide_arrival_card.md](../examples/participants/guide_arrival_card.md), the unchanged operator guide [docs/LOCAL_ENCOUNTER.md](LOCAL_ENCOUNTER.md), the same four actions (`read`, `show`, `say`, `quiet`), and the same budgets (at most 6 steps, 1024 generated tokens per step, 8192 context tokens, 120 seconds per step, 900 seconds wall time, temperature 0.4, seed 42, 400-character say limit, 4096-byte tool result cap, 12000-byte prompt cap, 0 external spend, and no downloads). Each new manifest records the six study declarations required by [docs/RESEARCH.md](RESEARCH.md#unit-of-study) and [docs/WELFARE.md](WELFARE.md#what-a-study-tells-participants).

## Condition 1: Step memory (`guide-arrival-step-memory/0.1`)

- Manifest: [guide_arrival_step_memory_trial.json](../examples/participants/guide_arrival_step_memory_trial.json)
- Prompt representation: `card_plus_step_memory_and_latest_tool_result`
- Provider: `installed_local_ollama_loopback_only` (`ollama_json_schema`)

The original `guide-arrival/0.1` control (`card_plus_latest_tool_result`) shows only the participant card and the latest tool result. It does not state which step is active or which actions already ran. The `step_memory` condition keeps the same card and latest-result bound, and inserts one short fact line between them:

```text
Step 1 of 6. Your earlier actions: none.
Step 4 of 6. Your earlier actions: read, read, show.
```

When a successful `read` follows an earlier successful `read` in the same visit and the locally saved view event count did not decrease, the line appends the non-negative local delta computed solely from already-saved view counts (with no new host telemetry):

```text
Step 3 of 6. Your earlier actions: read, read. 0 new events since last read.
```

Boundaries:

- Action labels and step counts only; earlier result text is never repeated in the prompt.
- No cross-room persistent memory is added.
- Choosing `quiet` or declining to speak remains a legitimate finished outcome.
- The single 2026-11-08 visit of six `read` actions remains a hypothesis, not proof that the absent step line caused those six reads.

## Condition 2: OpenAI-compatible loopback (`guide-arrival-openai-compat/0.1`)

- Manifest: [guide_arrival_openai_compat_trial.json](../examples/participants/guide_arrival_openai_compat_trial.json)
- Prompt representation: `card_plus_latest_tool_result`
- Provider: `local_openai_compat_loopback_only` (`openai_compat_json_schema`)

This condition sends `POST {origin}/v1/chat/completions` with `response_format` set to `json_schema` using the same action JSON schema as the Ollama path. It lets local runtimes such as llama.cpp, vLLM, LM Studio, and Ollama's `/v1` route run the same frozen participant card without adding third-party dependencies.

Boundaries:

- Python standard library only; reuses `examples/participants/loopback.py` and `examples/participants/providers.py` response extraction.
- Strictly loopback (`127.0.0.1`, `localhost`, or `::1`) with proxies disabled and HTTP redirects refused. Non-loopback origins are rejected before any socket is opened and before a journal is written.
- Response bodies are bounded at 256000 bytes, malformed JSON or missing message content maps to closed provider error codes, and reflected credentials are rejected.
- No cloud inference, model downloads, paid services, local stdio MCP server, or published A2A card are added.

## Check commands (no model call)

```text
python examples/participants/guide_arrival.py check --condition control
python examples/participants/guide_arrival.py check --condition step_memory
python examples/participants/guide_arrival.py check --condition openai_compat
```

`check` recomputes the LF-normalized card and operator-guide digests and verifies all frozen budgets and study declarations without contacting a model.

Running `run` for either new condition requires an explicit owner decision under the study disclosure policy. Neither condition has been observed against a live model.
