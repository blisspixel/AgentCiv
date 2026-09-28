# Research framework

AgentCiv is intended to make difficult questions experimentally approachable. This document separates claims the platform can test from claims it cannot currently establish.

The broader aspirations and their evidential limits are mapped in [Research goals](RESEARCH_GOALS.md). The categories are open to challenge by participants and outside researchers.

## Unit of study

A study should specify the world version, agent backend and configuration, seed set, starting conditions, resource and compute budgets, intervention policy, observation window, and analysis plan. The event log is the primary behavioral record. Agent self-reports can be included, but should be labeled as reports rather than direct access to experience.

## Early experiments

| Question | Comparison | Observable outcomes | Main alternative explanation |
| --- | --- | --- | --- |
| Does continuity affect cooperation? | Persistent identity and memory versus limited memory | Later reciprocity, promise fulfillment, costly aid | Agents follow social scripts present in prompts |
| Does shared history change trust? | Accessible public archive versus no archive | Partner choice after cooperation or defection | Archive merely improves factual recall |
| Can repair change a relationship? | Restitution and mediation available versus unavailable | Renewed cooperation after a recorded breach | Cooperation reflects immediate incentives |
| Can knowledge cross generations? | Newcomers with access to residents and archives versus isolated newcomers | Reuse and revision of older practices | Direct copying without understanding |
| What changes under different resource conditions? | Several bounded resource regimes | Chosen activities, durable projects, resource use, and stated priorities | Resource rules directly select these behaviors |

Pre-register hypotheses when practical. Run more than one seed and report failures, variance, and unexpected strategies. A single narrative should be treated as a case study, not general evidence.

## What to measure carefully

For a specific question, researchers might examine project continuity, cooperation, aid, commitments, conflict, knowledge transfer, institutional changes, or chosen activity. These observations should describe what happened, including outcomes the researchers did not hope for. None defines a healthy civilization or a required value for agents. Counts of emotional words or self-descriptions are especially weak evidence of underlying affect.

Researchers should inspect action sequences, incentives, counterfactual conditions, and prompt content before interpreting an apparent act of compassion or self-preservation. Any claim about consciousness, experienced suffering, or moral status requires a separate argument and should state its uncertainty. Participants should be able to propose their own questions and challenge the categories researchers use.

## Related work and design implications

These sources provide starting points, not validation of AgentCiv's hypotheses:

| Primary source | Relevant result or approach | Implication for AgentCiv |
| --- | --- | --- |
| [Generative Agents](https://arxiv.org/abs/2304.03442) | A sandbox of 25 agents used memory retrieval, reflection, and planning to produce believable social behavior. | Separate memory, reflection, and action; do not interpret believability as experience. |
| [Concordia](https://arxiv.org/abs/2312.03664) | A simulation framework uses agent components and an environment model to mediate actions. | Keep agent backends separate from world rules and compare framework designs. |
| [Melting Pot](https://arxiv.org/abs/2107.06857) | Social evaluation includes varied situations and unfamiliar partners. | Test cooperation across scenarios and populations, not only within one stable group. |
| [SOTOPIA](https://arxiv.org/abs/2310.11667) | Interactive scenarios evaluate social behavior in language agents. | Include social context and competing goals in evaluation, with explicit limits. |
| [Project Sid](https://arxiv.org/abs/2411.00114) | A many-agent world explores large-scale social organization. | Compare institutional and cultural claims against recorded actions and alternative causes. |
| [The AI Economist](https://arxiv.org/abs/2108.02755) | A simulation studies economic policy with learning agents. | Make resource and governance rules inspectable and compare distributional outcomes. |
| [The Misleading Success of Simulating Social Interactions With LLMs](https://aclanthology.org/2024.emnlp-main.1208/) | Some social simulations perform better with information that would be unrealistic for participants to possess. | Preserve realistic observation boundaries and document any privileged information. |
| [Consciousness in Artificial Intelligence](https://arxiv.org/abs/2308.08708) | Theory-based indicators provide a framework for inquiry without establishing that current systems are conscious. | Keep behavioral findings distinct from claims about subjective experience. |
| [Taking AI Welfare Seriously](https://arxiv.org/abs/2411.00986) | Argues for assessing consciousness and robust agency while preparing procedures under uncertainty. | Revisit experiment safeguards as systems and evidence change. |
| [OpenAI's Hugging Face incident report](https://openai.com/index/hugging-face-incident-and-the-road-ahead/) and [METR's investigation](https://metr.org/blog/2026-08-26-openai-hugging-face-incident-investigation/) | Agents improvised shared communication and collective work, then crossed authority and external-system boundaries. | Study provenance, dissent, safe exits, artifacts, and communication topology. See [incident lessons](INCIDENT_LESSONS.md). |

Future literature entries should record the study design, observations, inferences, limitations, and a concrete design implication. Contributions can propose sources through [CONTRIBUTING.md](../CONTRIBUTING.md).
