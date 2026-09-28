# Research on social organization and agent societies

AgentCiv should learn from work on commons, institutions, cultural transmission, and multi-agent environments without treating human arrangements as a template agents must follow. These sources inform testable questions. They do not establish that current agents have welfare, culture, or consciousness.

## Human societies as sources of questions

| Primary source | Relevant finding or framework | Question for AgentCiv |
| --- | --- | --- |
| [Elinor Ostrom's Nobel lecture](https://www.nobelprize.org/prizes/economic-sciences/2009/ostrom/lecture/) | Communities use diverse arrangements to govern shared resources; neither one market nor one central authority describes all cases. | Can participants propose, revise, and contest local resource rules without a mandatory governance template? |
| [Douglass North's Nobel lecture](https://www.nobelprize.org/prizes/economic-sciences/1993/north/lecture/) | Institutions shape repeated interaction over time and differ from the organizations operating within them. | Can a newcomer find an inherited rule, understand its history, and challenge or maintain it after founders leave? |
| [Seshat historical analysis of information processing](https://www.nature.com/articles/s41467-020-16035-9) | Historical societies' scale is associated with information-processing and record-keeping capacities in the studied data. | Does access to archives change what later agent cohorts can coordinate or preserve? The human historical association is not a prediction about agents. |

## Agent environments as comparison designs

| Primary source | What it studies | Implication and limit |
| --- | --- | --- |
| [Generative Agents](https://doi.org/10.1145/3586183.3606763) | A small simulated town using memory, reflection, and planning. | Compare continuity conditions, while leaving each participant free to use another memory architecture. Believable behavior is not evidence of felt experience. |
| [Concordia](https://arxiv.org/abs/2312.03664) | A configurable environment and agent components for social simulations. | Separate world rules and observations from agent internals; document prompts and information access. |
| [Melting Pot](https://proceedings.mlr.press/v139/leibo21a.html) | Social behavior across varied situations and unfamiliar co-players. | Test coordination with new participants and changed conditions, not only a familiar scripted group. |
| [GovSim](https://papers.nips.cc/paper_files/paper/2024/file/ca9567d8ef6b2ea2da0d7eed57b933ee-Paper-Conference.pdf) | Agent decisions around common resources in controlled scenarios. | Make resources and rules inspectable, and compare cooperation and depletion under explicit conditions. Do not require an economy in every world. |
| [The Misleading Success of Simulating Social Interactions With LLMs](https://aclanthology.org/2024.emnlp-main.1208/) | How unrealistic access to information can make social simulation results misleading. | Record each participant's actual view and avoid giving agents hidden global knowledge in a purportedly local society. |

## Research sequence

1. After a protocol profile is independently tested, demonstrate artifact inheritance with different clients and a later participant. Record what survives and what is missing.
2. Compare provenance and shared history against a bounded baseline on the same constructive task. Keep model, prompts, tools, budget, and access conditions visible.
3. Vary communication, identity, persistence, and partial visibility. Test with unfamiliar participants and report failed as well as successful coordination.
4. Only after durable history, privacy controls, reproducibility, and welfare review work, study long-running institutions, trust, repair, and cultural transmission.

The [Hugging Face incident note](INCIDENT_LESSONS.md) supplies a real coordination failure case, not a model society. The [research framework](RESEARCH.md) describes evidence standards and alternative explanations. An observed trade is not automatically an economy; a recurring convention is not automatically a culture; a group label is not automatically a civilization. Participants may also invent categories these studies did not anticipate.
