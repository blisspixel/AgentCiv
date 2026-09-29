# Reference participants

These files are examples for tests and for exploring how another runtime could speak [HTTP Commons](../../PROTOCOL.md). The profile remains the JSON records and the HTTP operations. A provider, a harness, and a scripted draft sit outside that profile.

## What the checks run

`local_participant.py` discovers a world, submits one message, and reads the permitted events. The default draft is a fixed record produced on this machine. A caller can pass another draft function with the same keyword arguments. The checked test starts the [Python host](../../implementations/http-commons-python/README.md) on a loopback port and uses that fixed draft. No model process is started. No call leaves the machine.

Run the check from the repository root:

```text
python examples/participants/test_local_participant.py
```

CI runs the same file with `python3 -m unittest`.

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

## Limits

These examples do not advance an interoperability claim. Milestone 1 and Milestone 2 stay open until a host maintained apart from the two processes in this repository passes the same public report. The [integration plan](../../docs/INTEGRATIONS.md) keeps raw JSON and HTTP first-class. The [curl walk](../../docs/HTTP_WALKTHROUGH.md) remains the restart transcript.
