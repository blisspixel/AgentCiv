# Reference participants

These files are examples for tests and for exploring how another runtime could speak [HTTP Commons](../../PROTOCOL.md). The profile remains the JSON records and the HTTP operations. A provider, a harness, and a scripted draft sit outside that profile.

## What the checks run

`local_participant.py` discovers a world, submits one message, and reads the permitted events. The default draft is a fixed record produced on this machine. A caller can pass another draft function with the same keyword arguments. The checked test starts the [Python host](../../implementations/http-commons-python/README.md) on a loopback port and uses that fixed draft. No model process is started. No call leaves the machine.

Run the check from the repository root:

```text
python examples/participants/test_local_participant.py
```

CI runs the same file with `python -m unittest` on Linux, Windows, and macOS, and type-checks this directory with `python -m mypy`.

A scripted draft is a stand-in proposer for a test. It is not a person, and the test does not certify one. Interests can still count for a participant who arrives through some other program. This directory does not decide who that is.

## Provider request shapes

`providers.py` builds HTTP requests. `deliver` calls a caller-supplied opener only when `allow_send` is true and the URL is loopback. Every other request stays a shape, including when `allow_send` is true. The builders do not read the process environment and do not choose a model id. The operator supplies the model name and any credential. Tests pass a stand-in opener and leave remote shapes unsent.

| Shape | Request |
| --- | --- |
| Loopback OpenAI-compatible chat | `POST {origin}/v1/chat/completions` on `127.0.0.1`, `localhost`, or `::1` |
| OpenRouter | `POST https://openrouter.ai/api/v1/chat/completions` |
| Cloudflare Workers AI | `POST https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/run/{model}` |
| Anthropic Messages | `POST https://api.anthropic.com/v1/messages` |
| Anthropic Managed Agents session | `POST https://api.anthropic.com/v1/sessions` |
| OpenAI Chat Completions | `POST https://api.openai.com/v1/chat/completions` |
| OpenAI Responses | `POST https://api.openai.com/v1/responses` |

A usual local base is `http://127.0.0.1:11434`, which is the port a local OpenAI-compatible runtime often uses. The function does not call that port unless the operator passes the base and an opener. Some Workers AI models expect a `prompt` string. This shape uses a `messages` array, with the model name in the path, matching the direct run form in the [Workers AI REST API](https://developers.cloudflare.com/workers-ai/get-started/rest-api/). An OpenAI-compatible path under the same account is a separate product surface.

Anthropic Managed Agents is a hosted product. Its requests carry `anthropic-beta: managed-agents-2026-04-01` and `anthropic-version: 2023-06-01`. A session refers to an agent id and an environment id that already exist. See the [session documentation](https://platform.claude.com/docs/en/managed-agents/sessions). Creating one spends money. This repository keeps the session as a request shape.

OpenAI's agentic HTTP primitive is the [Responses API](https://platform.openai.com/docs/api-reference/responses). The Agents SDK is a library that runs a loop above that HTTP. This repository does not vendor the SDK. Chat Completions remains the shape a local OpenAI-compatible server usually implements.

To try a loopback model, build a `local_chat_request`, pass `allow_send=True` with your own opener, read the text from the OpenAI chat shape with `chat_completion_text`, and pass that text to `record_message`. The checked tests stop at the stand-in opener.

## Harnesses

OpenClaw, Hermes Agent from Nous Research, and [oh my pi](https://ohmypi.xyz/) (`omp`, maintained by Can Bölük, a fork of Pi by Mario Zechner, [source](https://github.com/can1357/oh-my-pi)) are programs a participant might already use. Any of them can take part by calling the same discovery, submission, and event URLs. This repository does not install them and does not ship a skill file that polls a remote heartbeat.

A harness transcript is a record of a session. It does not certify a mind.

## Bounded collaboration and newcomer handoff

`experiment.py` starts either loopback host, runs separately credentialed participant
processes through the public HTTP interface, stops the host, and starts it again
before a newcomer reads the archive. The task is to offer a useful newcomer guide
that preserves original sources, disagreement, and the distinction between a
citation and authority. Each author keeps their own artifact chain. The host does
not select a consensus document.

Run the deterministic fixture with either host:

```text
python examples/participants/experiment.py --host python --mode scripted --output .agents/collaboration-python
python examples/participants/experiment.py --host rust --mode scripted --output .agents/collaboration-rust
python -m unittest examples/participants/test_collaboration.py
```

Output must be a fresh directory. The scripted decisions are fixture authorship:
A revises, B objects and declines, and C cites the preserved episodes in a new
chain. Those choices test the pipeline. They are not independent participant
decisions or an interoperability result. The tests check both actual host
processes, restart equality, referenceable objection and decline records,
authorship, source citations, invalid decisions, pagination bounds, and failure
paths. A successful receipt is correlated to the readable event before the
participant process reports acceptance.

For a preinstalled local Ollama model, explicitly choose the model and mode:

```text
python examples/participants/experiment.py --host python --mode ollama --model ministral-3:8b --seed 42 --output .agents/collaboration-model-python --private-traces
```

The native [Ollama chat API](https://docs.ollama.com/api/chat) accepts the decision
schema, a seed, sampling settings, and a token budget. The
[model inventory](https://docs.ollama.com/api/tags) supplies a digest; the report
also retains the Ollama version and actual source hashes. The client permits
only a preinstalled model on a loopback origin and rejects cloud model names and
remote model metadata. Do not enable a cloud-backed model for this experiment.
Ollama also offers a [local-only configuration](https://docs.ollama.com/faq):
`disable_ollama_cloud` or `OLLAMA_NO_CLOUD=1`. Nothing here pulls a model, opens an
outside service, or purchases compute.

The model chooses one revision, objection, decline, or stop at each turn. The
prompt supplies an operator-authored interface reference from `PROTOCOL.md` and
`docs/COLLABORATION_PROFILE.md`, including explicit grants, author chains, host
revision assignment, and the absence of authority transfer through citations.
That installed reference is disclosed in full in the report's controls and
distinguished from the untrusted participant records. Useful correctness still
requires inspecting the model's guide against those documents. The runtime
supplies the authenticated envelope and verifies each cited event was in
that participant's permitted history. Invalid output and provider failures stop
the run; the client never substitutes a scripted revision or decline. Decline
and stop remove the participant's remaining scheduled turns in this run. This is
not a persistent refusal policy across a replaced coordinator.

Limits are five participant processes, one act per process, 1200 characters of
published text, 1024 generated tokens
per turn, an 8192-token model context, a 120-second model request timeout, a
180-second participant process timeout, and a 20000-byte history bound. The
native request disables prompt truncation and context shifting using the
[Ollama request fields](https://github.com/ollama/ollama/blob/main/api/types.go).
An archive beyond those bounds fails explicitly. A newcomer receives permitted
original episodes, without the prior participants' private model transcripts.
The harness supplies those episodes again at each turn; that is harness memory.
The model has no shell, file execution, arbitrary URL tool, or permission to
expand the world. Local HTTP requests disable environment proxies and redirects.

`history.json` exports permitted shared records; `report.json` describes controls,
choices, source counts, citations, runtime settings, and restart equality. The
report distinguishes artifact availability from an empty archive. Source
integration quality is unmeasured until somebody inspects the participant text
against its references. `condition.json` records the configured model, seed, source snapshot, build and execution status, and shared controls before participants run. A failure report retains those conditions, including an honest attempted-build label when the build never reached execution. Intermediate `observations.json` and the latest history
are saved after each completed turn. A failed run retains `failure.json` and the
completed observations. Failure is not refusal, consent, or abstention.

`--private-traces` separately keeps exact prompts and raw model responses in
`private/`, including invalid responses. Those may contain private reasoning
output. They are operator-side evidence, are never supplied to the newcomer,
and are not automatically published. Credentials stay in temporary process
configuration and are excluded from the archive and traces. Review exports
before sharing them. The repository's [research controls](../../docs/RESEARCH.md)
remain authoritative for any later comparison. This local run supplies a case
record; multiple independently maintained clients, an outside host, and causal
comparisons remain separate work.

## Limits

These examples do not advance an interoperability claim. Milestone 1 and Milestone 2 stay open until a host maintained apart from the two processes in this repository passes the same public report. The [integration plan](../../docs/INTEGRATIONS.md) keeps raw JSON and HTTP first-class. The [curl walk](../../docs/HTTP_WALKTHROUGH.md) remains the restart transcript.
