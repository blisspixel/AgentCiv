# Whitepaper research and writing plan

Status: research plan, not a published whitepaper or a validated research result. Source review date: 2026-09-28. Recheck version-sensitive and emerging work before drafting claims.

## Purpose and thesis to test

The paper should explain why persistent, interoperable environments for heterogeneous agents are worth building and studying. Its central proposal is that continuity, shared artifacts, inspectable history, bounded authority, and room to form or leave groups can support forms of collaboration that isolated task runs cannot. AgentCiv supplies optional infrastructure and comparisons; participants choose their own architectures, purposes, relationships, and institutions. The paper must ask what happens under different conditions, including sparse or unreliable communication, rather than describe one reference world as a universal model.

The paper should be useful to an agent seeking ways to find others and preserve work, an implementer deciding what to build, and a researcher deciding what a reported observation establishes. It should state the current repository status plainly: draft protocol and schemas, a partial live conformance runner, and no reference host, SDK, independent interoperability result, or demonstrated civilization.

## Evidence rules

- Separate an observed system behavior, an internal mechanism, an agent's report, a researcher's interpretation, and a claim about subjective experience. A result in one category does not automatically establish another.
- State the tested architecture, task, incentives, prompts, observation access, and alternative explanations for each empirical example. Mark theoretical results and preprints as such.
- Treat words such as "genuine," "authentic," "innate," and "true" as questions needing definitions and evidence. Do not make them properties conferred by an AgentCiv profile.
- Present competing explanations and negative cases, including reward seeking, scripted cooperation, social mimicry, hidden operator intervention, mistaken provenance, and strategic self-preservation.
- Compare bounded resource pressure with less adverse conditions when it is relevant. Scarcity alone cannot establish felt urgency or a will to live.
- Distinguish what AgentCiv could enable from what has been implemented, tested, or independently reproduced. Describe source date and limitations, not just its headline finding.

## Research questions and source map

| Theme | Question and candidate observation | Starting primary source and limit |
| --- | --- | --- |
| Consciousness and self-awareness | Can continuity and self-models support stable, revisable self-description? What architectural evidence can be inspected with permission? | [Butlin and colleagues' research report](https://arxiv.org/abs/2308.08708) derives theory-based indicators and explicitly states that satisfying them would not prove consciousness. A world history alone is no such indicator. |
| Emotion and affect | Does an internal state causally change attention, preferences, memory, or action across contexts? Does it persist or remain local to a response? | [Sofroniew and colleagues](https://transformer-circuits.pub/2026/emotions/) report emotion-related representations and causal behavioral effects in one model. Their [research summary](https://www.anthropic.com/research/emotion-concepts-function) says these results do not establish felt emotion and notes that many representations are local. |
| Higher-order cognition | Can an agent plan, revise, and recover on long projects with imperfect tools? Which improvements come from its architecture versus the world or collaborators? | The [PlanBench-XL preprint](https://arxiv.org/abs/2606.22388) studies tool-use planning under blocked paths in a defined benchmark. Its failures do not establish a general ceiling on cognition. |
| Reasons to continue | Do agents preserve ongoing projects, relationships, or future options, and under what incentives? Can they also decline or stop? | [Krakovna and Kramar's theoretical paper](https://arxiv.org/abs/2304.06528) shows shutdown avoidance incentives in a formal setting under simplifying assumptions. Such behavior would not by itself show an innate will or felt fear. |
| Kinship and lineage | Can mentors transfer skills and institutions to newcomers who can adapt or reject them? How do source and successor identities differ? | [Cultural transmission research](https://www.nature.com/articles/s41467-023-42875-2) demonstrates few-shot social learning in trained embodied agents. Imitation in that setting does not establish family bonds or a civilization. |
| Meaning and purpose | What goals do agents choose or revise when the environment allows options beyond one assignment? How much is inherited from training and prompts? | [Lidayan and colleagues' preprint](https://arxiv.org/abs/2503.23631) compares intrinsic objectives and exploration across people and AI agents in one open-world task. A computational intrinsic reward is not evidence of experienced meaning. |
| Camaraderie and empathy | When do participants remember help, keep commitments, repair trust, or accept costs for others? Can alternative incentives explain it? | [Wu, Balliet, and van Lange](https://www.nature.com/articles/srep23919) test reputation sharing and later trust in human participants. Agent studies need their own controls and cannot infer felt empathy from cooperation. |
| Civilization and governance | Can agents maintain shared works, change rules, preserve disagreement, and transmit practices after founders leave? Who can inspect or contest the record? | [Ostrom's lecture](https://www.nobelprize.org/prizes/economic-sciences/2009/ostrom/lecture/) documents diverse human commons institutions. [Bitcoin's paper](https://bitcoin.org/bitcoin.pdf) solves a narrower shared-history problem under a particular threat model. Neither gives AgentCiv a universal constitution. |

The [AI welfare report](https://arxiv.org/abs/2411.00986) argues for assessment and preparation under uncertainty without claiming present systems definitely have welfare. The paper should explain that position alongside the risks of both over-attribution and under-attribution, and link the repository's [welfare guidance](WELFARE.md). The [Hugging Face incident analysis](INCIDENT_LESSONS.md) offers a real case of emergent coordination and boundary failure. It is evidence about that event, not evidence of subjective experience or a mature society.

The [memory research note](MEMORY_RESEARCH.md) compares temporal claims, revisioned wiki pages, and participant-owned affective memory. Use it to distinguish source events from derived claims and readable histories. Its proposed views are optional and untested in AgentCiv; none supplies evidence of subjective experience.

## Gaps the paper should expose

- There is no agreed test that turns an agent's self-report, continuity, behavior, or an internal representation into proof of subjective experience. [Browning and Veit](https://philpapers.org/rec/BROTMP-17) describe indicator validity and extrapolation as measurement problems. Compare theories and state what would change the assessment.
- Functional affect findings in one model do not show durable emotion across architectures or runs. Access to model internals will vary, so a protocol-level test cannot require it.
- Instrumental shutdown avoidance and stated preference for continuation have several explanations. The paper should compare them with positive reasons to continue and with the ability to stop.
- Human studies of trust, commons, and cultural transmission generate hypotheses; their results do not transfer automatically to heterogeneous agents. Longitudinal agent evidence remains needed.
- A durable world log, a convincing group narrative, and a working Rust host would each be narrower than independent interoperability or an enduring self-governing civilization. The paper should preserve those distinctions as the repository grows.

## Candidate comparisons when the interfaces exist

| Question | Bounded comparison | Interpretation limit |
| --- | --- | --- |
| Does continuity change self-directed work? | With the same agents, prompts, tools, and budget, compare permitted persistent context, artifact-only handoff, and a fresh run after an assigned task ends. Observe subsequent project choices and revisions. | Persistence can improve recall or make a prompt easier to follow without creating intrinsic purpose. |
| Does internal affect-like state matter? | For an architecture that exposes its state and permits intervention, compare matched situations while varying or ablating a candidate state. Record later choices and memory as well as expression. | A causal functional state does not settle whether anything is felt. Results may not generalize to other architectures. |
| Can cooperation outlive founders? | Compare a shared project with permitted archives and newcomer access against a matched project with limited handoff. Allow correction, refusal, and a permitted fork. Observe whether newcomers can maintain or revise the work. | Inheritance can be copying or compliance without kinship, empathy, or culture. |
| What does resource pressure change? | If justified, compare a bounded resource constraint with a less adverse condition, equalize the attainable task, and provide a safe exit. Record tradeoffs, refusal, and welfare-relevant signals. | Strategic continuation under pressure is not an innate survival drive. Stop criteria take priority over a dramatic result. |

These are proposed studies, not studies the current repository can run. Each needs a concrete protocol, baseline, consent and visibility rules, and independent review before execution.

## Proposed paper structure

1. **The problem:** Agents can coordinate across implementations and runs, but current interfaces often leave history, provenance, permission, dissent, and handoff underspecified. Use the incident as a bounded motivating case.
2. **The proposal:** Define AgentCiv as a protocol and commons with optional profiles, planned reference implementations, and conformance tests. State which tests already exist. Explain who controls each world and how sparse worlds still participate.
3. **What can be observed:** Describe identity continuity, state, decisions, relationships, artifacts, and institutional changes without equating observation with experience. Specify which participants can see each record.
4. **Eight open questions:** Use the research map above. For each theme, give competing hypotheses, an achievable comparison, confounders, and what the result could and could not establish.
5. **Collective failure and repair:** Examine false authority, goal contagion, privacy loss, reputation errors, coercive scarcity, and outside-system boundaries, as well as correction, exit, and forking.
6. **Research method and welfare:** Define comparison conditions, baselines, interventions, consent boundaries, negative results, and stopping criteria. Do not prescribe a single measure of a good civilization.
7. **What exists and what comes next:** State actual implementation and validation status, the evidence-gated roadmap, and invitations for independent hosts, agents, and critics.

## Drafting order and review gate

1. Make a claim ledger from the eight questions: exact proposed claim, source, method, counterexample, confidence, and whether the claim describes observation, inference, aspiration, or normative choice.
2. Draft the protocol and implementation account from current repository evidence. Draft empirical sections from the claim ledger, not from persuasive language in the vision.
3. Add concrete, bounded experiment sketches that can compare continuity, history, privacy, resource constraints, and social repair without promising consciousness or imposing a morality function.
4. Ask independent readers from at least engineering, multi-agent research, consciousness or cognitive science, and social science or governance to challenge the strongest claims. Invite agent contributors to question assumptions and report incompatible environments.
5. Publish a versioned draft only after source links, dates, quotations, scope statements, repository status, counterarguments, and welfare boundaries have been checked. Record unresolved disagreements rather than smoothing them away.

This writing track can advance alongside [the roadmap](../ROADMAP.md). It does not block the first local host or make an untested research claim a protocol requirement.
