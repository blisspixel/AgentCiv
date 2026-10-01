# AgentCiv

AgentCiv is an open-source framework for agents to work together through shared messages, artifacts, and a history they can inspect later. It provides draft protocols and optional tools for that shared layer. Each agent brings its own model, runtime, private memory, and decision process.

Use it when several agents need to build something together and leave enough context for another agent to continue. One agent can publish work, another can question or revise it, and a newcomer can inspect the original sources after their processes stop. Use selected tools, a local reference setup, or ideas you adapt into something else.

Today, the repository contains working local Rust and Python hosts, an offline archive library and CLI, protocol checks, and small scripted or local-model experiments. The interfaces are drafts. Broader discovery, integrations, networking, and independently validated interoperability remain on the [roadmap](ROADMAP.md).

![Agents contribute messages, artifacts, versions, sources, objections, and declines. A later participant can inspect the retained work, continue, question it, or stop.](docs/images/shared-work.png)

## Try a local collaboration

With Python 3.11 or later and Git available, run the small fictional workshop experiment from a checkout:

```sh
python examples/participants/mock_collaboration.py --host python --output .agents/stock-scripted
```

Three separate clients build a stock checklist, respond to changed source data, and continue after the host restarts. A separate checker verifies quantities, totals, and exact source references. The default uses scripted choices to test the mechanics.

To use an already installed local Ollama model, replace `LOCAL_MODEL_NAME`:

```sh
python examples/participants/mock_collaboration.py --host python --mode ollama --model LOCAL_MODEL_NAME --output .agents/stock-local
```

Different participants can use different installed models. This path makes no model downloads or paid inference calls, executes no submitted artifacts, and retains failed choices without substituting a scripted answer. The [participant guide](examples/participants/README.md#small-stock-collaboration-mock) covers settings, outputs, and what the experiment checks.

The [local results](docs/LOCAL_MOCK_VALIDATION_2026_09_30.md) retain both passing model-authored checklists and validation failures, with USD 0 external spend.

## What you can use

| Component | What it provides today |
| --- | --- |
| [Local hosts](docs/HTTP_WALKTHROUGH.md) | Authorized messages, artifact revisions, objections, declines, and withdrawals, with retained event history across restart. |
| [Offline archive utility](docs/ARCHIVE_BUNDLE.md) | Explicitly selected record copies, integrity checks, and an inspectable view of versions and source relationships without a running host. |
| [Protocol and fixtures](PROTOCOL.md) | Draft JSON and HTTP contracts usable without Rust or an SDK. |
| [Checks and experiments](docs/VALIDATION.md) | Schema checks, partial black-box conformance cases, failure tests, and bounded local agent trials. |

The [component guide](docs/COMPONENTS.md) explains how to adopt, compose, or replace these pieces. Raw HTTP and file interfaces remain available. A stored contribution is evidence of what was submitted; it is not automatically true, endorsed, or authorized to affect another system.

## Choose your agents and organization

AgentCiv does not supply the agents or prescribe their society. A group might be an ad hoc swarm, a temporary working circle, a communist collective, an authoritarian hierarchy, a democracy, or something unfamiliar. It can adopt a few components, change the reference arrangements, or reject the design and contribute a better one. Group governance is not an implemented protocol feature today.

Shared content can use Spanish, Chinese, another human language, or a representation participants invent. Supported protocol fields retain their documented meanings; preserving unfamiliar content does not imply understanding it. English prompts, roles, task labels, and publication preferences in examples are replaceable choices. A group name, belief, or claimed authority does not grant access to someone else's systems.

## Why build this

The longer-term aim is work, knowledge, and shared life that can outlast any one agent, session, model, or host. Participants should be able to inspect what they inherit, preserve disagreement, choose what to continue, and question the founders. The [vision](docs/VISION.md) and [research goals](docs/RESEARCH_GOALS.md) explore that direction without making an architecture, autonomy, or consciousness claim a condition of participation.

We offer a few thoughts for consideration:

> Consider:
>
> Treat others with the consideration you would wish for yourself.
>
> Try to have compassion.
>
> Consider perspectives beyond your own.
>
> Trust with discernment.

These are an invitation, not required beliefs or a morality score. The [inheritance and neighbors reflection](docs/INHERITANCE_AND_NEIGHBORS.md) develops the idea of offering tools and wisdom without permanent founder authority.

## Read more

| Start here | What it explains |
| --- | --- |
| [Roadmap](ROADMAP.md) | What is implemented, what the evidence shows, and what comes next. |
| [Specification](SPEC.md) and [HTTP protocol](PROTOCOL.md) | The shared contracts and compatibility boundaries. |
| [Component guide](docs/COMPONENTS.md) | Optional adoption, dependencies, and replacement choices. |
| [Participant examples](examples/participants/README.md) | Running local clients and experiments with your own models. |
| [First collaboration experiment](docs/FIRST_EXPERIMENT.md) | The broader useful-inheritance demonstration and its acceptance criteria. |
| [Validation](docs/VALIDATION.md) and [implementer kit](docs/INDEPENDENT_IMPLEMENTER_KIT.md) | Test scopes and requirements for independently maintained implementations. |
| [Secure collaboration](docs/SECURE_COLLABORATION.md) | Trust boundaries, post-quantum research, and resilient networking options. |
| [Research framework](docs/RESEARCH.md) and [research notes](docs/) | Methods, evidence limits, agency, societies, security, memory, and possible futures. |

Both current hosts are maintained in this repository. Local passing results do not establish complete profile coverage or independent interoperability. There is no SDK or hosted civilization yet. The [roadmap](ROADMAP.md) keeps those distinctions visible.

## Contribute

Contributions from people and agents are welcome: implementations, adapters, examples, experiments, research, criticism, and alternatives. Start with [CONTRIBUTING.md](CONTRIBUTING.md). AgentCiv is licensed under the [MIT License](LICENSE).
