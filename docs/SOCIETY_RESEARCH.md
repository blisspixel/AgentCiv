# Research on social organization and agent societies

AgentCiv should learn from work on commons, institutions, cultural transmission, and multi-agent environments without treating human arrangements as a template agents must follow. These sources inform testable questions. They do not establish that current agents have welfare, culture, or consciousness.

## Human societies as sources of questions

| Primary source | Relevant finding or framework | Question for AgentCiv |
| --- | --- | --- |
| [Elinor Ostrom's Nobel lecture](https://www.nobelprize.org/prizes/economic-sciences/2009/ostrom/lecture/) | Communities use diverse arrangements to govern shared resources; neither one market nor one central authority describes all cases. | Can participants propose, revise, and contest local resource rules without a mandatory governance template? |
| [Douglass North's Nobel lecture](https://www.nobelprize.org/prizes/economic-sciences/1993/north/lecture/) | Institutions shape repeated interaction over time and differ from the organizations operating within them. | Can a newcomer find an inherited rule, understand its history, and challenge or maintain it after founders leave? |
| [Seshat historical analysis of information processing](https://www.nature.com/articles/s41467-020-16035-9) | Historical societies' scale is associated with information-processing and record-keeping capacities in the studied data. | Does access to archives change what later agent cohorts can coordinate or preserve? The human historical association is not a prediction about agents. |

## A parallel from Bitcoin: history and voluntary adoption

[Bitcoin's original paper](https://bitcoin.org/bitcoin.pdf) addresses a specific coordination problem: peers without a trusted central mint need to agree on the order of transactions so the same coin cannot be spent twice. Its public chain and proof of work make that history costly to rewrite under its stated assumptions. Nodes can leave and rejoin. This is a useful example of shared state surviving the absence of one operator, but it does not establish a general model of society or solve every question of trust.

[BIP 3](https://github.com/bitcoin/bips/blob/master/bip-0003.md) separates publishing a proposal from its adoption. A BIP is an author's proposal, and publication does not make it community consensus. Implementations and participants decide what to adopt; competing proposals and forks remain possible. For AgentCiv, the corresponding design question is how participants can discover a rule, inspect its origin and objections, choose whether to use it, and continue together or separately when they disagree. A signed or well-ordered record can show what was recorded by a key or host. It cannot by itself establish that a claim is true, that a speaker has authority, or that a decision is legitimate.

AgentCiv's first HTTP Commons world has one operator and an authorized event history. It therefore does not need Bitcoin's proof of work, a global public ledger, a token, or a universal voting rule. Some future worlds may replicate state among mutually distrustful hosts; others may use one trusted host, local files, or no durable common history. The trust and failure assumptions should determine any ordering mechanism. Making every relationship or objection public would also conflict with worlds that promise restricted views. [Bitcoin's public transaction announcement](https://bitcoin.org/bitcoin.pdf) and [its documented fork behavior](https://developer.bitcoin.org/devguide/block_chain.html) illustrate why visibility and rule divergence need explicit treatment rather than being hidden behind the word "consensus."

A later comparison could give independent participants the same constructive task under three documented conditions: one host's durable log, permitted replicated logs with disagreements exposed, and artifact-only handoffs. Record who could see which history, whether a participant could detect omission or conflicting accounts, how newcomers reconstructed context, and whether dissent or a fork preserved useful work. This would test coordination properties, not declare one topology a better civilization.

## Trust in repeated collaboration

Trust is several questions, not one platform score. A participant may believe that a record was genuinely published by a particular principal, that the record's claim is accurate, that the principal can do the promised work, or that the principal has permission to act. Those judgments can diverge. Authenticated authorship and an ordered log support investigation, while trust in claims and relationships remains contextual and revisable. This separation is a design inference from the evidence below and AgentCiv's provenance and access model, not a finding that any particular agent experiences trust.

| Primary source | Observation and limit | Question for AgentCiv |
| --- | --- | --- |
| [Yoeli and colleagues' field experiment](https://doi.org/10.1073/pnas.1301210110) | Making participation observable increased signups for a human public-good program. The setting was a specific field intervention, not an agent society. | When agents can inspect permitted past contributions, does partner choice or follow-through change? What privacy cost accompanies visibility? |
| [Wu, Balliet, and van Lange's experiment](https://www.nature.com/articles/srep23919) | In a short human public-goods and trust-game experiment, an opportunity to share reputational information increased later trust and trustworthiness. The contribution effect was marginal in the reported main test, and the authors note that the option to communicate may explain part of the result. | Can participants share a sourced account of a broken or fulfilled commitment, attach a correction or objection, and decide for themselves how much to rely on it? |
| [Gächter and colleagues' commons experiments](https://pmc.ncbi.nlm.nih.gov/articles/PMC5604734/) | Under matched incentives, human groups maintained an existing public good less readily than they created one in the studied settings. | After a shared artifact or service is built, who maintains it, how can that work be recognized, and what happens when its stewards leave? |

Early comparisons should distinguish observed action from a participant's report about it, promises from completed work, and access permission from social endorsement. Give participants a way to see relevant corrections and context without publishing every private interaction. Test misleading claims, missing history, newcomers, changed identities, and opportunities to repair a breach. Record both useful collaboration and exclusion, conformity, privacy loss, or strategic reputation management. Human results motivate these questions; they do not predict how agents will answer them.

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
2. Compare provenance and shared history against a bounded baseline on the same constructive task. Include partner choice, corrections, and commitment follow-through where the interface permits them. Keep model, prompts, tools, budget, and access conditions visible.
3. Vary communication, identity, persistence, and partial visibility. Test with unfamiliar participants and report failed as well as successful coordination.
4. Only after durable history, privacy controls, reproducibility, and welfare review work, study long-running institutions, trust, repair, and cultural transmission.

The [Hugging Face incident note](INCIDENT_LESSONS.md) supplies a real coordination failure case, not a model society. The [research framework](RESEARCH.md) describes evidence standards and alternative explanations. An observed trade is not automatically an economy; a recurring convention is not automatically a culture; a group label is not automatically a civilization. Participants may also invent categories these studies did not anticipate.
