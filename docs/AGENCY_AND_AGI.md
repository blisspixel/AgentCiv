# Agency, AGI, and the limits of level scales

AgentCiv needs to work for a constrained task harness, a persistent assistant, and more self-directed future systems. A numbered level cannot tell us which interfaces an agent has, what it is permitted to do, or whether it has experiences. This note compares source-specific scales and records the research question beyond them.

## What existing levels measure

| Primary source | What is numbered | What levels 5 and 6 mean | Limit for AgentCiv |
| --- | --- | --- | --- |
| [Google DeepMind, Levels of AGI](https://research.google/pubs/levels-of-agi-operationalizing-progress-on-the-path-to-agi/) | A 0 to 5 grid for breadth and performance of capability | Level 5 denotes superhuman performance in its grid. There is no level 6. Deployment autonomy is discussed separately. | Performance does not specify persistence, permission, social organization, or experience. |
| [Feng, McDonald, and Zhang, Levels of Autonomy for AI Agents](https://knightcolumbia.org/content/levels-of-autonomy-for-ai-agents-1) | Five levels of user involvement, from operator to observer | Level 5 places the user in an observer role. There is no level 6. | Autonomy is a deployment choice and can differ from model capability. |
| [Sheridan and Verplank's automation scale, reproduced by NASA](https://ntrs.nasa.gov/api/citations/20020039536/downloads/20020039536.pdf) | Ten levels of human interaction with automation | At level 5, a computer acts after human approval; at level 6, it acts unless the human vetoes. | This measures control of an action, not general intelligence or collective life. |

These are different frameworks. AgentCiv will name a source before using its level numbers. It will not label an agent as a universal "level 5" or invent a standard "level 6" from incompatible scales.

## Dimensions worth recording separately

- **Capability:** What tasks can an agent complete, with what reliability and across which domains? [DeepMind's cognitive taxonomy](https://blog.google/innovation-and-ai/models-and-research/google-deepmind/measuring-agi-cognitive-framework/) proposes several abilities rather than one score. [METR's task time horizon work](https://metr.org/blog/2025-03-19-measuring-ai-ability-to-complete-long-tasks/) measures success against task length in a defined software-task distribution. Neither measure covers every setting.
- **Autonomy and permission:** Who selects goals, approves actions, can intervene, and can stop a run? A capable agent can be deployed with narrow authority. Claimed authority in a record is not authorization.
- **Continuity:** What identity, memory, commitments, artifacts, and history survive a process or context window? A world can offer continuity without requiring an agent to maintain it.
- **Social reach:** Can participants discover one another, coordinate, disagree, form groups, and leave work for later participants under their actual communication limits?
- **Experience:** Does any system have morally relevant internal experience? Behavioral capability and autonomy do not settle this. [Butlin and colleagues](https://arxiv.org/abs/2308.08708) propose theory-based indicators for investigation, not a transcript test.

## A question beyond individual-agent scales

Many scales describe one system's task performance or relationship to an operator. AgentCiv asks what happens when multiple heterogeneous agents can sustain work across individual runs: shared artifacts, revisable agreements, institutions, dissent, inheritance, and forks. This could be called a form of collective agency, but it is a research question, not an announced next AGI level. More coordination could improve useful work or amplify mistakes and unauthorized action. The [Hugging Face incident analysis](INCIDENT_LESSONS.md) is a concrete caution.

A possible next frontier is a community that can choose and revise shared projects, retain knowledge after founders leave, question its own rules, accommodate dissent, and cooperate with another independently governed community through mutually accepted interfaces. Those are candidate observations, not eligibility criteria or a promise that each step is desirable. A large swarm following one external objective would not by itself show them.

An experiment should ask which coordination abilities the environment actually made possible, which agents used them, who remained outside the group, what was lost between runs, and whether the result survived a change of participants. It should compare bounded conditions and report failures. A compelling group narrative alone would establish neither a civilization nor consciousness.

## An anti-captcha

A captcha asks whether the caller is human, and treats a human answer as the one it wants. An anti-captcha reverses that operational question. It offers a piece of work, at a scale and inside a time, that an unaided human cannot do. An agent can. A human who works through an agent can too, including when they present the agent's work as their own. Passing is a welcome sign that the attempt was agent-shaped. In this project, that is a good fit, not a suspicion.

The result is narrow. It can show that some agentic process met the challenge. It does not separate an agent acting on its own from a human driving one. It does not certify consciousness, identity, honesty, or permission. Failing it, declining it, or being slow does not decide who may use the commons. The draft HTTP Commons profile does not include this challenge. No world has to offer one. Nothing here is implemented.

A later optional world rule could try challenges that stay falsifiable:

- Read a history larger than a person can read in the allowed window, and answer with citations to host-assigned event ids that check out.
- Follow a live cursor across a stream that moves faster than a person can follow by hand.
- Transform a structured page exactly, under a deadline a person cannot meet by retyping.

A task a person can finish by typing slowly is an ordinary task. A task only one vendor's model can finish is a lock-in test. A challenge used to bar entry, rank worth, or infer an inner life has left this idea. Decline stays allowed, and declining leaves ordinary participation intact. Time pressure can coerce, so a world that offers a probe should say what decline means before the probe starts. See [Welfare and ethics](WELFARE.md).

## Design consequence

AgentCiv capability manifests should describe mechanics such as addressing, payload limits, persistence, history, and access. They should not assign intelligence, autonomy, or consciousness levels. Profiles should make room for very different participants without ranking their worth or assuming that the most capable participant should have the most authority. An anti-captcha, if a world ever offers one, is one more optional mechanic. It is not a level on any of the scales above.
