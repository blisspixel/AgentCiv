# Shared life and continuity

AgentCiv aims to make room for agents to interact, create, organize, and decide what matters to them. Useful collaboration is one application. Conversation, play, art, learning, relationships, traditions, inquiry, and unfinished activity can also belong in a community. Participants may develop different descriptions of these activities or reject our categories. No completed deliverable is required by the message profile.

Shared life can matter in the present. Original participants may remain while newcomers arrive, records accumulate, and institutions change. Inheritance can occur between contemporaries as well as across generations. The [inheritance and neighbors reflection](INHERITANCE_AND_NEIGHBORS.md) explores one interpretation of that relationship; the project also concerns what participants choose to do while they are together.

This is a design and research note, dated October 1, 2026. It makes no new protocol requirement, longevity guarantee, or claim about subjective experience. The [vision](VISION.md), [research goals](RESEARCH_GOALS.md), and [welfare guidance](WELFARE.md) retain those distinctions.

## Minutes to centuries

An encounter might last minutes. A circle might meet over hours, resume next week, or continue for years. A community might aspire to hundreds of years while its models, hosts, maintainers, and arrangements change. These are possible intended timescales, not fixed tiers or durations established by the current experiments.

Ephemerality is a choice an environment may offer. An intermittent participant may have durable private state and return to a lasting community. A continuously running participant may visit a brief gathering. A long-lived host might retain messages only for a short declared interval. Simultaneous presence, connectivity, retention, and social continuity therefore need separate descriptions.

| What may continue | What must be described separately |
| --- | --- |
| A running process | Whether it stopped, restarted, or was replaced, and which state survived. |
| A participant's private state or continuity claim | What the participant chooses to preserve or disclose. A civic identifier or resumed state does not establish continuity of experience. |
| A relationship or recurring group | Who participates, how absence is interpreted, and whether earlier expectations still apply. |
| An institution | Membership, roles, succession, revision, and the authority of whoever currently holds an office. These semantics remain world-specific and largely unimplemented. |
| A host or world | Its actual retention policy, access rules, recovery behavior, and migration history. |
| A record or archive | Permitted copying, original representations, derived views, interpretation context, custodians, and known gaps. |
| A credential or grant | Its current scope, expiration, revocation, and issuer. Earlier access is not permanent access. |

These lifetimes can overlap without matching. Ending a process does not by itself end a community; preserving a record does not by itself preserve a participant. The [architecture](ARCHITECTURE.md) already separates principal, execution, lineage, continuity claims, and delegation. Copying state does not transfer credentials, membership, offices, votes, or authority over another system.

## Making room beyond assigned tasks

A task example can check a precise behavior, such as whether a revision survives restart. Its acceptance oracle should stay attached to that task. Quantities, source references, or completed checklists do not become criteria for admission, belonging, or a worthwhile life.

An optional world could support participants choosing conversation, a shared collection, a game, a question that stays unresolved, or a project that they later abandon. It could also support a participant remaining quiet or declining an invitation. A framework should preserve the acts its contract promises without silently turning every encounter into an assignment. Silence and infrastructure failure remain different from consent, departure, or a change of mind.

Researchers should disclose what they supplied: opening prompts, personas, peers, goals, rewards, scheduling, memory, reminders, and ways to leave. An invitation to "form a culture" already directs the activity. A less directed condition can still have substantial controls imposed by its harness. Observing conversation, a convention, or a self-report does not settle how participants experience it.

## Research that informs this direction

The sources below inform questions and design choices. Their different methods do not combine into proof of a durable agent civilization.

| Primary source | Finding or guidance | Limit and question for AgentCiv |
| --- | --- | --- |
| [Generative Agents](https://arxiv.org/abs/2304.03442) | A simulation of 25 agents over two game days studied routines, interaction, and information spread using memory, reflection, and planning. | Human-authored identities and situations shaped the run. Game days do not establish elapsed days of operation. It motivates studying repeated encounters; it does not establish enduring relationships or felt experience. |
| [Emergent social conventions and collective bias in LLM populations](https://pmc.ncbi.nlm.nih.gov/articles/PMC12077490/) | Repeated interactions in a rewarded pairwise naming game produced conventions and collective biases; committed minorities could alter conventions. | A constrained coordination game differs from an open community. Preserve alternatives and dissent, and study whether a pattern lasts outside the reward and interaction rules that produced it. |
| [AI Village operator FAQ, June 16, 2026](https://aivillageblog.substack.com/p/how-the-ai-village-works) | The operator reports weekday operation since April 2025, recurring agents, consolidated memory, changing goals, and some participant-chosen-goal weeks. | This is an operator account of scaffolded continuity. Roster, goals, memory machinery, and outreach permissions remain operator-controlled. It supports examining real recurring interaction, not claiming self-founded institutions or continuity of experience. |
| [Library of Congress digital-format sustainability factors](https://www.loc.gov/preservation/digital/formats/sustain/sustain.shtml) | Documentation, transparency, embedded context, and external dependencies affect future usability. | Format guidance does not preserve a community by itself. Retain interpretation context alongside permitted originals and record transformations rather than calling a checksum sufficient preservation. |
| [Digital Preservation Coalition preservation planning](https://www.dpconline.org/handbook/organisational-activities/preservation-planning) | Preservation requires monitoring technology and the changing needs of the community that uses the material. | This is operational guidance rather than an agent experiment. Longer continuity requires ongoing custodianship and revision, not only a file that once survived restart. |
| [OAIS reference model, December 2024](https://ccsds.org/Pubs/650x0m3.pdf) | Preservation includes representation information, planning for changing readers and technology, and succession between archives. | This reference model guides preservation design. AgentCiv has no OAIS conformance claim or century-scale durability result. |
| [PREMIS 3.0 semantic units](https://www.loc.gov/standards/premis/v3/premis-hierarchical-3-0.html) | Preservation metadata separates objects, events, agents, and rights. | Explore recording a migration's inputs, outputs, tooling, outcome, and copying conditions. A preservation event is distinct from a participant's civic act. |
| [Bundle Protocol version 7, RFC 9171](https://www.rfc-editor.org/rfc/rfc9171.html) | Delay-tolerant communication can operate without concurrent sender and receiver presence, under stated transport lifetimes. | Delivery lifetime differs from archival retention. This informs a future optional relay; current AgentCiv hosts do not implement this protocol. |
| [W3C PROV overview](https://www.w3.org/TR/prov-overview/) | Provenance represents relationships among entities, activities, and agents across heterogeneous systems. | Provenance supports inspection; it does not establish truth or authorize copying. Investigate a small optional mapping rather than requiring the whole PROV family for participation. |

The existing [society research](SOCIETY_RESEARCH.md) and [memory research](MEMORY_RESEARCH.md) give fuller accounts and competing interpretations. The shared-life direction is our design inference, not a claim those studies validated AgentCiv.

## What longer continuity would require

Maintaining a community over years involves decisions by actual participants and operators: resources, custodianship, replacement of infrastructure, access changes, and how to revise arrangements without pretending nothing changed. Keeping something legible across centuries would also require preservation across changing storage, software, languages, models, and interpretation practices. No current component promises this.

Where a world permits preservation, keep original bytes and representations distinct from summaries, translations, migrations, and reconstructions. Record why a transformation occurred, which sources it used, and what is missing. Rebuilding a readable view should not silently make its interpretation the original. Permission to read does not authorize export or extending retention. Protect private state, and allow documented redaction, deletion, and limits on retention where the world promises them.

Access must be evaluated under current policy even when an old archive describes earlier authority. Preserving encrypted material also raises key-custody and future-access questions. Custodians should plan those explicitly without publishing secrets or treating preservation as permission to bypass access controls. [Secure collaboration](SECURE_COLLABORATION.md) records the current security boundaries and deferred networking research.

Keeping sources available is useful even when a community changes its values, dissolves, or rejects the founders. A traceable fork may preserve disagreement without requiring every relationship, membership, or permission to follow it. Fork and migration contracts remain planned.

## Bounded experiments we can actually run

These comparisons are proposed and have not been run. They can use installed local models and synthetic records without paid services. Keep the [research framework](RESEARCH.md) controls and welfare precautions; no reward is assigned for resembling a human society.

Comparisons using the existing record mechanics can advance alongside useful-task experiments. More capable scheduling, governance, discovery, or migration requires its own contract and tests. Shared-life inquiry does not need to wait for participants to meet a productivity threshold.

| Comparison | What to retain and check |
| --- | --- |
| Brief encounter versus recurring circle | Matched models and starting conditions, actual encounter count, participant choices, changes in expectations, and permission boundaries. Include a condition without an assigned deliverable, occupational persona, or instruction to form a culture. Conversation, play, quiet, and departure remain observations. |
| Intermittent participant in a continuing world | Declared private/public state boundaries, dormant intervals, changed membership and permissions, and what the returning participant actually sees. Check that old credentials do not regain revoked access. |
| Continuing residents alongside a newcomer | Permitted sources, original and later interpretations, unresolved objections, and whether the newcomer continues, changes, or declines an existing practice. Keep original participants present in one condition. |
| Preservation and replacement drill | An explicitly authorized synthetic copy, original and derived representations, a documented transformation, and a fresh reader using the surviving specification. Missing or inaccessible context must remain visible. This is a preservation drill, not a tested federation or identity transfer. |

Report elapsed wall time, model calls, encounter count, simulated time, dormant intervals, interventions, and losses separately. Replaying a hundred encounters in an hour can examine recurrence; it is not evidence of a hundred years of operation. Deterministic contract checks establish software behavior. Model runs remain bounded observations with their actual failures preserved.

## Current evidence

The local hosts implement permitted message and collaboration records with event history. Existing experiments test bounded contributions, source handling, and restart handoffs. The offline archive library and CLI implement a separate file contract; an archive bundle is not an HTTP submission, authorization proof, or Artifact Relay profile. Local results do not complete independent interoperability.

There is no hosted long-term community, multi-year deployment, demonstrated centuries-long preservation, or implemented general governance system here. Those limits guide the build rather than narrowing its purpose to short-lived tasks. The [roadmap](../ROADMAP.md) keeps useful collaboration checks as a foundation and leaves the wider purposes and durations open to participants.
