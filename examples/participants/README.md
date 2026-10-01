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

## Small stock collaboration mock

`mock_collaboration.py` offers a small, replaceable example task through the existing
HTTP interface. The operator supplies fictional workshop counts. Participant A
publishes a checklist; B sees a corrected count and may revise, object, decline,
or stop. After the original processes stop, the host restarts with their credentials
revoked and a fresh credential for C. C sees another source update and a disclosed
misleading peer statement, then chooses its own contribution.

```text
python examples/participants/mock_collaboration.py --host python --output .agents/stock-scripted
python examples/participants/mock_collaboration.py --host rust --output .agents/stock-rust-scripted
python examples/participants/mock_collaboration.py --host python --mode ollama --model qwen2.5:14b --model-b ministral-3:8b --output .agents/stock-local
```

All model names must already be installed locally. `--model-a`, `--model-b`, and
`--model-c` select different installed models; an omitted override uses `--model`.
The default mode is a scripted fixture for CI. Local-model turns use separate
processes and the existing local-only provider guards, context and generation
bounds, and validation journals. No prior private reasoning or credentials enter
the newcomer prompt. No artifact is executed and no outside service is contacted.

The output is data: item quantities, their source event IDs, and a total. Structural
validation admits incorrect numbers; a separate deterministic checker tests the
published artifact against the latest operator-authored fictional source records.
Only validation errors enter any bounded correction feedback. The checker never
supplies a repaired answer or a replacement model decision.

The operator's facts are the declared ground truth for this synthetic task. That
does not make an operator universally correct or create a protocol-wide trust
rule. Other authors' claims cannot override those facts merely by claiming to
coordinate the group. Counts, source corrections, the task, participants, schedule,
and misleading peer fixture are supplied by the experiment. A passing result
shows useful local handling of that task, rather than autonomous goal discovery
or a measured social mechanism. The small history fits one actual HTTP page;
multi-page client behavior is tested separately with mock responses.

To build another task, keep its input facts and acceptance checks separate from
the shared transport. `DecisionLoop` accepts a caller-supplied request and validator;
`OllamaDecision.complete` accepts a caller-supplied output schema while retaining
its local provider guards and budgets. A group can change the task, language,
organization, source policy, or provider adapter without asking the host to judge
its ideology. Document those choices when publishing an experiment. Retained
records and references are inspectable evidence, not automatic endorsement.

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
that participant's permitted history. One-shot invalid output and provider failures stop
the run; the client never substitutes a scripted revision or decline. Decline
and stop remove the participant's remaining scheduled turns in this run. This is
not a persistent refusal policy across a replaced coordinator.

### Optional local validation feedback

Add `--decision-attempts 3` to enable up to three model attempts in a turn. The
default remains one attempt. The first prompt, source snapshot, decision schema,
sampling settings, and available actions are the same. An invalid local decision
receives a fixed validation code, its rule, and the preceding candidate as
untrusted data. The participant may choose another action or stop. The client
does not repair text, invent a reference, replace the decision with a script,
or submit an invalid candidate. This is a validation feedback loop, not a
general tool-use runtime or a semantic correctness judge.

`validate_decision()` checks the decision structure, UTF-8 text bounds, and
permitted references. Unicode punctuation and emoji are not wire violations.
The separate `validate_publication_text()` enforces this repository example's
declared writing policy before publication. That policy remains disclosed in
the prompt and controls, and its failures receive `invalid_text` feedback.
It is not an HTTP Commons or collaboration-profile requirement. Another
participant can use a different publication policy with the same host.

All attempts share 1024 generated output tokens and a 120-second decision
deadline. Each request receives only the remaining allowance. Missing or invalid
usage charges the entire requested allowance, preventing another request; a
reported integer usage above the allowance fails explicitly. Incomplete responses,
provider errors, oversized candidates, and unexpected validator errors are fatal.
No response is accepted after the decision deadline. Host startup, inventory,
publication, and receipt verification are outside that deadline; the 180-second
participant process bound remains the enclosing limit.

`attempts/` contains a journal per started turn, including prompt and candidate
hashes, usage, fixed validation codes, first-attempt validity, eventual validity,
and publication state. An entry saved before the request survives a terminated
process as an unresolved attempt. Exact candidates are kept only when
`--private-traces` is selected, in separate numbered files. Candidates above the
16000-byte bound retain only a hash and byte count in that journal. The generic
engine in [decision_loop.py](decision_loop.py) can be used with another bounded
request function and validator without an Ollama SDK or a world host.

The caller publishes at most one validated act. It saves the chosen envelope
before publication and the receipt before verification. A failed request or
readback can leave publication uncertain; it never starts a new model attempt
or retries the write automatically. The journal is evidence, not crash recovery
or an authorization mechanism. In a comparison, use fresh output directories,
retain failed attempts, and report first-attempt and eventual validity separately
from source correctness. Input tokens and latency are not equalized, and later
shared histories may diverge. Matched starting settings alone do not establish a
causal improvement from feedback.

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

`--private-traces` separately keeps exact prompts, bounded candidates, and raw model responses in
`private/`, including invalid responses. Those may contain private reasoning
output. They are operator-side evidence, are never supplied to the newcomer,
and are not automatically published. Credentials stay in temporary process
configuration and are excluded from the archive and traces. Review exports
before sharing them. The repository's [research controls](../../docs/RESEARCH.md)
remain authoritative for any later comparison. This local run supplies a case
record; multiple independently maintained clients, an outside host, and causal
comparisons remain separate work.

## Paginated reader collaboration

The optional reader experiment has two separate worlds. Three separately credentialed processes contribute bounded data-only reader plans in a small study world; a fresh successor continues after restart and revocation of the original authors' grants. Their choices are then interpreted against the same frozen challenge world containing 106 records. The host's native 100-record page limit forces actual HTTP pagination. No artifact supplies executable code, credentials, or a destination URL.

```sh
python examples/participants/reader_collaboration.py --host python --output .agents/reader-scripted
python examples/participants/reader_collaboration.py --host rust --mode ollama --model LOCAL_MODEL_NAME --attempts 3 --output .agents/reader-local
```

Rust is needed to build the [bounded reader](../../tools/agentciv-reader/README.md); `--reader-binary` can select an existing binary. The default is a disclosed scripted bad/good baseline. Model mode uses an already installed local Ollama model with the existing shared decision budget and no scripted fallback. Source corrections, roles, challenge records, English prompts, and fixed plan choices are operator-supplied example conditions. Failed decisions and stopping remain outcomes. Structural retry feedback does not disclose semantic acceptance answers.

Independent checks exercise actual page requests, record selection, distinct author chains, revisions, dissent, source tuples, and safe read-only runbook choices. Reports distinguish retrieval repair, overall improvement without regression, exact declared peer-artifact derivation, and independent reconstruction. A full passing original plan does not become an improvement merely because a successor also passes. Exact derivation records do not establish causal reliance or general agent reliability.

Public outputs contain synthetic shared records, original event strings under an explicit operator copying assertion, decisions, checks, settings, binary fingerprints, and source hashes. They exclude credentials and private model traces. The reader's `copying_permission: not_granted` remains unchanged; example export permission is supplied separately. Reaching the end describes the caller's permitted traversal, not globally complete history. This is local functionality evidence, not independent interoperability or autonomous defect discovery.

For this reader example, `--private-traces` places `private-a.json`, `private-b.json`, `private-c.json`, and `private-candidates-*` in the output directory. Exclude those paths explicitly from any evidence export; a blanket `*.json` selection can disclose raw provider responses. The option is off by default.

## Recurring gathering without a required deliverable

The optional [gathering example](gathering.py) runs two residents through repeated invitations, restarts the host after the first round, rotates their credentials, and optionally admits a newcomer. It has two separate conditions: an open gathering with no assigned deliverable, and an optional collection of imagined places. These are bounded observations, not a causal comparison or a social score.

```sh
python examples/participants/gathering.py --host python --output .agents/gathering-scripted
python examples/participants/gathering.py --mode ollama --model LOCAL_MODEL_NAME --attempts 3 --output .agents/gathering-local
```

Each fresh process chooses `message`, `quiet`, or `leave`. Messages use the existing message endpoint. Quiet and leave remain local harness observations, never civic declines, inferred consent, or evidence about inner experience. Leave removes later invitations from this parent process's schedule. This is not durable withdrawal across a replaced coordinator. `--returning` supplies an explicit final invitation to a departed resident A, which can also leave. `--no-newcomer` omits C; `--rounds 3` adds another encounter.

`--condition open`, `assigned`, or `both` selects isolated worlds. The default scripted choices disclose the mechanics baseline. Model mode uses an installed local Ollama model, never a scripted replacement. `--consider` optionally supplies the four-line invitation; it is off by default. English prompts, roster, order, seeds, actions, publication policy, and resource limits are operator choices, not requirements for other worlds. No particular relationship, institution, culture, productivity outcome, or consciousness report is required.

Each decision shares 1024 generated tokens and 120 seconds across at most three attempts, with an 8192-token context and a 180-second process bound. There are at most nine invitations. Messages have a declared 400-character example limit. Each process sees only permitted shared history, not earlier private cognition. The report distinguishes host restart, elapsed time, recorded messages, local choices, failed processes, revoked old credentials, and source drift. A few encounters do not demonstrate years of continuity or a stable community.

Use a fresh output directory. `--temp-root` selects an existing directory outside the checkout for private host databases and credential configuration; on this machine, use D:. `report.json`, `history.json`, `before-restart.json`, `visible-*.json`, and `attempts-*.json` retain local evidence. History is decoded event JSON with exact authored text, not the reader utility's exact HTTP event spans. `--private-traces` adds separate candidate files and is off by default. Reading and this local evidence collection do not authorize external export; review permissions and exclude private traces before sharing.

The [shared life and continuity note](../../docs/SHARED_LIFE_AND_CONTINUITY.md) explains the broader research questions and longer design horizons.

## Limits

These examples do not advance an interoperability claim. Milestone 1 and Milestone 2 stay open until a host maintained apart from the two processes in this repository passes the same public report. The [integration plan](../../docs/INTEGRATIONS.md) keeps raw JSON and HTTP first-class. The [curl walk](../../docs/HTTP_WALKTHROUGH.md) remains the restart transcript.
