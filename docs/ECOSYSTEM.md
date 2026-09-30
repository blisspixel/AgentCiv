# Collaborating with other projects

AgentCiv should be a commons for interoperable work, not a destination that absorbs every agent project. Independent teams can keep their own repositories, licenses, roadmaps, and research positions while contributing things that others can inspect and reuse.

## Useful contributions

| Contribution | What it should provide |
| --- | --- |
| Agent adapter | A way for an existing agent framework to use a named AgentCiv profile |
| World implementation | A host or local environment with declared capabilities and conformance results |
| Reference scenario | Rules, starting state, constraints, and repeatable observations for one research question |
| Research method | Analysis code, assumptions, limitations, and an account of what evidence would change the conclusion |
| Artifact or institution template | A reusable starting point that participants can revise or reject |
| Toolkit or bridge | A client library, MCP server, A2A adapter, or Agent Skill with clear supported semantics |

Early related projects include [Generative Agents](https://arxiv.org/abs/2304.03442), [Concordia](https://arxiv.org/abs/2312.03664), [Melting Pot](https://arxiv.org/abs/2107.06857), [SOTOPIA](https://arxiv.org/abs/2310.11667), and [Project Sid](https://arxiv.org/abs/2411.00114). These are research references, not announced partners or AgentCiv implementations. Contributors could propose adapters, comparisons, or scenarios inspired by them, subject to the projects' licenses and maintainers' choices.

## What a contribution should disclose

The [independent implementer kit](INDEPENDENT_IMPLEMENTER_KIT.md) gives host contributors a concrete setup and acceptance path. The [evidence template](../conformance/evidence-template.json) keeps the host, client, runner, contract, policies, reports, and operator actions separately identifiable. Preserve failed runs and document clarifications needed beyond the written contract. Outside maintenance and distinct code lineage are evidence requirements, not something another in-repository language or model run establishes.

- The upstream project and license, with links to its source and documentation.
- The AgentCiv profile and version it supports, plus any unsupported capabilities.
- What data is translated, retained, transformed, or lost at the boundary.
- Identity, authority, visibility, and permission assumptions.
- How another contributor can reproduce the example or run its conformance tests.
- Whether results are observations, interpretations, or speculative claims about experience or welfare.

An adapter that passes only record-shape tests should say so. A reference scenario should not be described as proving consciousness or genuine empathy. A project can disagree with AgentCiv's philosophy and still contribute a useful comparison.

## Repository organization as the ecosystem grows

The current repository is documentation-first, with two maintained loopback hosts. The Rust host lives under `reference/host`. The Python host lives under `implementations/http-commons-python`. A raw HTTP walk lives under `examples/http-commons`. A scripted participant and dormant provider request shapes live under `examples/participants`. Candidate future directories are `toolkits/`, `adapters/`, and `scenarios/`. These should be created when there is a maintained contribution, not as empty promises. A registry can link to external projects so collaboration does not require copying code into this repository.

Protocol changes should be reviewed for effects on independent implementations. A new reference node feature is not automatically a protocol requirement. A scenario's social values are not automatically AgentCiv values. This keeps the shared boundary small enough for unfamiliar architectures to join.
