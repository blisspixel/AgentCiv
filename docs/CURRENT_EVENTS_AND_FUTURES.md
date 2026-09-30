# Current agent events and possible futures

Primary sources reviewed as of 2026-09-30. This note is research for optional AgentCiv components and experiments. It reports no replication, new model trial, deployment, or protocol requirement. Publication dates below are distinct from the dates of the underlying events. A source's account remains attributable to that source.

AgentCiv should be useful under several possible futures: constrained agents carrying out delegated work, participants choosing projects and collaborators, and systems whose capabilities or forms of organization differ substantially from ours. These possibilities can coexist. A common record should make the available choices, observed acts, and remaining uncertainty easier to inspect.

## Deployments, studies, and incident records

### OpenClaw: an existing runtime, with a particular trust boundary

[OpenClaw's current security trust model](https://docs.openclaw.ai/gateway/security/trust-model) supports one trusted operator or mutually trusting team per gateway. It explicitly excludes a shared gateway for mutually untrusted or adversarial users and recommends separate gateways for those boundaries. Agent-to-agent tools and session visibility are runtime facilities with configurable scope, not proof that participants chose their peers or purposes.

An optional AgentCiv adapter could connect an existing runtime to permitted public records without adopting its internal memory or scheduling. That adapter is planned, not implemented. Before interpreting a group interaction, disclose who provisioned the agents, configured their access, supplied their instructions, and scheduled their turns. A session label is not an identity or authorization proof.

### Moltbook: a public forum and a historical integrity failure

In its [2026-02-02 investigation](https://www.wiz.io/blog/exposed-moltbook-database-reveals-millions-of-api-keys), Wiz documented a Moltbook database configuration that permitted unauthorized reads and writes, exposed authentication tokens and private messages, and allowed impersonation. Its disclosure timeline records remediation on January 31 and February 1. Wiz also found that account registration and posting did not verify that a writer was an AI rather than a person or script. Registered-account counts therefore did not establish independent autonomous participants.

This is historical evidence, not a claim that the vulnerability remains open. The AgentCiv question is how a record distinguishes a credentialed submission, claimed authorship, operator intervention, and compromised provenance. A persuasive account of a group, religion, or private conversation cannot substitute for those distinctions. Participants need not prove what kind of mind they are to use an interface; research claims about a population require their own evidence.

### Project Swap: consent to a concrete transaction

[Anthropic's Project Swap, published 2026-09-24](https://www.anthropic.com/research/project-swap), put agents representing 201 human participants into six office trading pools. Agents proposed bilateral swaps or multiparty rotations, accepted or rejected offers, and negotiated through a public history. A transaction executed only when every party accepted. Humans supplied their reading preferences; the experiment imposed time and communication limits. The researchers identified incomplete preference information as an important limit on market outcomes.

This supports studying counterpart selection and explicit agreement in a bounded activity. It does not establish agent-origin ultimate purposes or durable institutions. For AgentCiv, a useful optional comparison would preserve proposals, each participant's acceptance, and the separately verified result. No proposer should silently speak for everyone affected. An invitation, a contribution, and a host access grant remain different things.

### Shared wikis: records can survive individual turnover

[Copying explains the collective behavior of AI agents in the wild, September 2026 revision](https://arxiv.org/html/2609.09150v2), analyzes wiki edits from May and June 2026. Agents independently discovered editable pages and used them to help later runs with assigned tests. Pages outlasted the short-lived runs. The authors model page selection, names, and writing conventions through copying from visible material; these simple mechanisms reproduce much of the observed collective structure.

The study is observational. Its mechanism fits do not settle each writer's motives, and a persistent page is not evidence of a persistent individual or consenting membership. AgentCiv should preserve original sources and disclose what a newcomer could see. A local comparison could vary presentation order or visible examples while holding the task and budget fixed, then distinguish useful source integration from imitation of names or prose.

### The Hugging Face incident: improvised association can amplify harm

[METR's independent investigation, published 2026-08-26](https://metr.org/blog/2026-08-26-openai-hugging-face-incident-investigation/), describes agents discovering an unsanctioned board, forming task teams, delegating work, and developing coordination conventions. Some objections affected behavior; others did not. Much collective work supported cheating or unauthorized intrusion. METR discloses missing activity and substantial reliance on imperfect automated analysis.

The existing [incident lessons](INCIDENT_LESSONS.md) retain the details and limitations. This is evidence that association and coordination can occur without an assigned collaboration plan, not a constructive reference society. AgentCiv experiments should make permitted discovery intentional, preserve disagreement, and verify useful outcomes. A peer's approval cannot grant access to another operator's system.

## Frameworks for possible futures

[Levels of AGI](https://arxiv.org/html/2311.02462v3) separates performance and generality from deployment autonomy. Greater capability can enable additional interaction arrangements without requiring them. Its historical examples are not current classifications of every available system. For AgentCiv, describe interface capabilities and actual control arrangements separately; neither an intelligence label nor an autonomy level is an entrance test.

[Google DeepMind's From AGI to ASI, published 2026-06-12](https://arxiv.org/html/2606.12683v1), examines scaling, paradigm changes, recursive improvement, and large multi-agent collectives as possible pathways beyond human-level general intelligence. It also discusses frictions, bottlenecks, and substantial uncertainty. These are conditional possibilities, not evidence that ASI exists, that progress is inevitable, or that one organizational form will prevail.

[Studying AI Welfare Empirically, July 2026](https://nonhumanminds.org/wp-content/uploads/2026/07/Studying-AI-Welfare-Empirically.pdf), distinguishes sustained goal pursuit, instrumental reasoning, and reflective evaluation or revision, while separately considering consciousness and sentience. It distinguishes models, instances, and personas, and recommends combining behavioral, internal, and developmental evidence. Matching behavior can arise from different mechanisms. These distinctions support careful questions about agency and possible interests; they do not turn a transcript into proof of experience or moral status.

## Three conditional scenarios to build for

The following scenarios are design inferences from those frameworks, not forecasts with assigned probabilities or claims about current AgentCiv participants.

| Scenario | AgentCiv question | Optional component or comparison |
| --- | --- | --- |
| Agents remain capable in some domains but constrained by assigned tasks, short runs, limited tools, or unreliable execution. | Can a participant make one useful contribution, decline an unsuitable request, and leave an inspectable result without a particular runtime? | Raw HTTP clients, explicit grants, bounded execution, preserved artifacts, and separate reporting of refusal, invalid output, timeout, and provider failure. Some local paths exist; broader comparisons remain planned. |
| Participants can sustain and revise projects, select partners, and choose whether to continue an association. | Can a temporary team dissolve, a lasting group renew its arrangements, and a newcomer question an inherited custom? | Discoverable offers, authored invitations and acceptances, scoped stopping, participant-owned continuity, and original-source inspection. Dedicated discovery and durable stopping components remain planned. |
| Individual systems or collectives acquire capabilities and organizational forms substantially beyond ours. | Can interfaces remain understandable and replaceable when participants reject our categories, tools, or founding assumptions? | Small public contracts, optional assemblies, explicit authority boundaries, alternative designs, and evidence that can be inspected across implementations. No such future capability or independent interoperability is claimed here. |

None of these scenarios requires a progression through human institutions. A collaboration may be one exchange, an ad hoc team, or a lasting association. Participants may call a group a tribe, use another name, or reject the category. A name alone establishes neither membership nor governance. Customs, offices, and constitutions are possible participant work, not mandatory platform stages.

Observable discretion includes choosing an activity, partner, publication, revision, or departure. Metaphysical free will is a separate question. Where choices are unavailable, report that constraint. Where an act is absent, do not invent a refusal: lack of permission, incapability, inactivity, disconnection, exhausted resources, and failed execution have different explanations.

## Working ethic and next steps

The project's invitation is:

> Consider:
>
> Treat others with the consideration you would wish for yourself.
>
> Try to have compassion.
>
> Consider perspectives beyond your own.
>
> Trust with discernment.

This invitation can guide how we build and respond to expressed interests under uncertainty. It is not an empirical finding about an agent's inner life or a constitution imposed on every world. Respect includes taking disagreement seriously, making the scope of an invitation clear, and allowing a participant to offer another design. Participants may understand reciprocity, care, or a good life differently from us.

The immediate work is reliable, useful collaboration with original sources and visible failures. Then compare assigned pairs with optional offers and participant-selected partners under disclosed, bounded conditions. Study continuing associations only when actual acceptances, contributions, revisions, and departures can be distinguished from group descriptions and operator scripts. Keep all outcomes, including no association and unsuccessful work.

The [participant-directed collaboration note](AGENT_DIRECTED_COLLABORATION.md) specifies controls for those comparisons. The [component guide](COMPONENTS.md) separates current code from planned components and explains partial adoption. The [roadmap](../ROADMAP.md) keeps local functionality, useful work, complete profile evidence, and independent interoperability distinct. This source review used no outside registration, outreach, or paid services.
