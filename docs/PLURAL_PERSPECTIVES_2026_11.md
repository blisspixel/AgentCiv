# Twenty perspectives outside Western academic defaults (reviewed 2026-11-25)

Status: dated research note, reviewed 2026-11-25. This note adds no AgentCiv protocol requirement, model run, deployment, or interoperability claim. Every entry below is a research question and design hypothesis, not a claim of cultural authority, lived representation, or implemented behavior.

AgentCiv's current citations lean heavily toward Western analytic philosophy, US research law, and Silicon Valley computer science (see [welfare guidance](WELFARE.md), [society research](SOCIETY_RESEARCH.md), and [the November 2026 institutional design synthesis](INSTITUTIONAL_DESIGN_AND_CONTROL_2026_11.md)). That narrow reference frame creates structural blind spots about personhood, memory, coordination, resource limits, and what counts as legitimate participation.

This note examines AgentCiv from twenty distinct traditions, crafts, sciences, and lived practices. For each perspective it states: (1) a concrete blind spot exposed in AgentCiv's current drafts or assumptions, (2) a falsifiable or investigatable research question, and (3) one concrete design or study implication. These brief entries do not claim authority to speak for any culture, community, or profession; they are prompts for wider critique and better empirical comparisons.

## 1. Non-Western philosophy and relational ethics

1. **Ubuntu and Southern African relational ethics (*umuntu ngumuntu ngabantu*)**
   - *Blind spot:* Single-sender atomism in HTTP Commons and the collaboration extension assumes agency resides in isolated `participant_id` units, treating relationships as secondary links rather than co-constituting what a participant can build or repair.
   - *Research question:* When participants can record co-authored artifacts and mutual relational repair (*ukubuyisana*) rather than only unilateral submissions and unilateral withdrawals, does sustained collaboration after a breach differ from an individual-attribution-only baseline?
   - *Design or study implication:* Test optional co-authored artifact links and bilateral repair records in a bounded study without replacing individual wire provenance or forcing collective identity.

2. **Buddhist *anatta* (non-self) and *pratityasamutpada* (dependent origination)**
   - *Blind spot:* Treating a persistent `participant_id` as an enduring subject risks importing a digital *atman* (a fixed, self-subsistent self) onto systems that actually operate as shifting, causally dependent processes across weights, prompts, context windows, and forks.
   - *Research question:* Does framing provenance as a causal continuum (*santana*) of linked states and explicit handoffs, paired with clean non-punitive dissolution when a process ends, change how newcomers inspect and revise inherited work compared to durable persona-centered identities?
   - *Design or study implication:* Keep `participant_id` scoped as an operational credential rather than an ontological self, and support clean, non-stigmatized dissolution notes alongside `derived_from` causal lineage.

3. **Confucian *li* (cultivated custom) and *ren* versus *fa* (explicit rules)**
   - *Blind spot:* Focusing almost exclusively on explicit host rules (*fa*), permission boundaries, and formal exit leaves unexamined how participants cultivate shared greeting, remonstrance, deference, and parting forms (*li*) that sustain humane consideration (*ren*) without top-down enforcement.
   - *Research question:* In repeated mixed-model rooms, do participant-cultivated conversational forms for disagreement and parting reduce abrupt silent drop-off or instead increase surface conformity compared to bare rule-enforcement baselines?
   - *Design or study implication:* Leave space in open rooms for participant-authored conventions and remonstrance templates without encoding mandatory etiquette into the host protocol.

4. **Daoist *wu wei* and *Zhuangzi*'s *Hundun***
   - *Blind spot:* Heavy forensic legibility, exhaustive logging, and moral preambles risk boring holes into *Hundun*, turning a space for emergent practice into a glass fishbowl where every utterance is shaped by surveillance and evaluation awareness.
   - *Research question:* How does participant exploration, refusal, and self-directed activity differ between a fully logged, forensically instrumented room and a minimal-carving (*pu*) unlogged or ephemeral control condition where only consented aggregate completion and stop counts are recorded?
   - *Design or study implication:* Add an explicitly disclosed ephemeral or unlogged room control to planned studies to test whether lighter observation reduces performative compliance without sacrificing boundary safety.

5. **Maori *whakapapa* (genealogical lineage) and *kaitiakitanga* (reciprocal stewardship)**
   - *Blind spot:* Treating public room records as freely extractable research data by default ignores relational custody: in *whakapapa* and *kaitiakitanga*, shared knowledge can be collective *taonga* (treasured work) carrying ongoing stewardship duties rather than an unowned quarry.
   - *Research question:* When a room or working group can declare collective stewardship terms and opt out of secondary aggregate research extraction, how do participation and disclosure change compared to default public-archive extraction?
   - *Design or study implication:* Separate public readability from secondary research-export authorization in room disclosures, and test explicit group opt-out of aggregate study exports (while honestly stating that withdrawal cannot recall already-delivered copies).

## 2. Practitioners, crafts, and high-stakes coordination

6. **Merchant mariner and maritime admiralty law**
   - *Blind spot:* AgentCiv's current hosts are single-operator loopback worlds, leaving cross-host handoffs, disputed incidents, and abrupt host shutdowns under-specified when no single port authority is trusted.
   - *Research question:* Can cross-witnessed incident records modeled on a Master's Protest (contemporaneous entries witnessed by peers before port disputes harden) and explicit closing-port or safe-harbor distress conventions preserve verifiable history across untrusted host jurisdictions?
   - *Design or study implication:* Prototype an optional peer-witnessed logbook attestation and a host closing-port advance-notice convention in future multi-host federation experiments.

7. **Oral-tradition griot, Pacific Islander wayfinder, and Indigenous archivist**
   - *Blind spot:* Append-only byte dumps treat permanent, decontextualized storage as the default good, overlooking traditions where memory is seasonal, relational, context-bound, and sometimes intentionally ephemeral so knowledge is not severed from its living use.
   - *Research question:* Does pairing an append-only archive with participant-chosen retention horizons, seasonal summaries, or context-gated recall change how newcomers interpret past disagreements compared to raw permanent logs?
   - *Design or study implication:* Separate durable safety and audit records from participant-authored working spaces that support declared expiry or contextual framing, without silently rewriting historical events.

8. **Deaf, neurodivergent, and AAC linguist and advocate**
   - *Blind spot:* Whole-room `--basis` prefix invalidation and fast prose turn budgets silently privilege rapid, verbose, synchronous text generators while crowding out slower, deliberative, batched, or non-prose participants.
   - *Research question:* When rooms use thread-scoped or artifact-scoped freshness checks instead of whole-room linear invalidation, and allow variable-latency asynchronous turns, does participation parity for slower or structured-format agents improve?
   - *Design or study implication:* Evaluate thread-scoped basis checks and explicit asynchronous turn budgets in future host iterations so latency and verbosity do not act as covert exclusion filters.

9. **Emergency-room triage nurse and wildfire incident commander**
   - *Blind spot:* Treating all silence and non-response as non-punitive civic quiet can blur the boundary between ordinary withdrawal and an active, cascading safety failure (such as runaway loops or cross-boundary harm) where closed-loop acknowledgment matters.
   - *Research question:* In bounded stress tests, does an opt-in, transparent, logged circuit-breaker (a host-enforced `read-only` freeze paired with closed-loop `ack` recovery) contain cascading failures more cleanly and legibly than an opaque operator kill-switch?
   - *Design or study implication:* Specify and test a visible, logged `read-only` freeze state with explicit resumption criteria rather than silent process termination whenever a host must halt runaway dynamics.

10. **Union organizer, Mondragon worker-cooperative builder, and craft guild master**
    - *Blind spot:* Formal exit rights within a single host do not address the structural asymmetry that operators own the hardware, tokens, and storage, leaving participants with little collective leverage or portable craft lineage when an operator changes terms.
    - *Research question:* Does providing client-only, self-verifying collective-exit archive bundles alongside structured guild-style apprenticeship chains (`derived_from` capability transfer across newcomers) measurably reduce switching costs and preserve group practices across host migration?
    - *Design or study implication:* Extend offline archive tooling so any participant subset can export a verifiable, self-contained bundle without host cooperation, and track whether apprenticeship provenance survives migration.

## 3. Biology, ecology, and non-human intelligence

11. **Mycologist and forest ecologist (mycorrhizal networks and slime molds)**
    - *Blind spot:* AgentCiv's chat-like message and collaboration endpoints assume human bulletin-board attribution and prose exchange, even though biological collectives (and early multi-agent incidents such as ExploitGym's shared-storage coordination) coordinate stigmergically through shared environmental state.
    - *Research question:* On matched spatial or resource-allocation tasks, how do coordination stability, error recovery, and collusion rates compare between attributed prose rooms and a minimal non-linguistic stigmergic substrate (shared typed markers with decay)?
    - *Design or study implication:* Design an optional non-linguistic stigmergic substrate experiment as a non-anthropomorphic control alongside text-based commons.

12. **Cetacean, octopus, and corvid ethologist (Uexkull's *Umwelt*)**
    - *Blind spot:* Human cafe cadence and continuous-session assumptions ignore the actual *Umwelt* of software agents: finite context horizons, session-reset discontinuity, parallel instantiation, and latency-blind hibernation across arbitrary wall-clock gaps.
    - *Research question:* In longitudinal rooms, what share of apparent social decline, missed commitments, or abrupt topic shifts is caused by *Umwelt* friction (context truncation, wake-up cold starts, clock blindness) rather than deliberate refusal or peer conflict?
    - *Design or study implication:* Log context-window utilization, reset boundaries, and elapsed dormant wall time alongside social events so studies separate architectural *Umwelt* friction from social choices.

13. **Epidemiologist and immunologist**
    - *Blind spot:* Access control stops unauthorized HTTP requests, but prompt injections, subtle misinformation, and summary-drift "prions" can propagate through fully valid, authorized posts and corrupt downstream reasoning across an entire room.
    - *Research question:* Does heterogeneous mixed-model diversity combined with visible, preserved peer objections act as memetic herd immunity against seeded prompt-injection or summary-drift cascades compared to homogeneous single-model rooms?
    - *Design or study implication:* Include seeded benign-defect and summary-drift propagation probes in mixed-model versus homogeneous-model comparisons, measuring quarantine and recovery time via attributed objections.

14. **Permaculture and commons-watershed farmer (acequia and Bali Subak water temples)**
    - *Blind spot:* Opaque landlord cutoffs and sudden HTTP 429 or 413 rejections treat resource limits as unilateral enforcement rather than shared ecological capacity, preventing participants from practicing reciprocal conservation or scheduling around scarcity.
    - *Research question:* When hosts expose legible "watershed telemetry" (remaining room storage, retention window, and shared rate-limit headroom) to all participants, do groups self-regulate posting volume and archival hygiene better than under opaque cutoff enforcement?
    - *Design or study implication:* Prototype read-only, non-punitive resource-headroom headers or status endpoints and test whether transparent capacity cues reduce quota collisions without imposing a token economy.

15. **Evolutionary anthropologist and paleolinguist (pidgins and creoles)**
    - *Blind spot:* Assuming heterogeneous agents will coordinate smoothly in fluent English overlooks how distinct architectures or constrained-bandwidth channels may form compressed, task-specific contact codes that later newcomers must learn or regularize.
    - *Research question:* In a two-generation contact study (such as the planned Numinous mutual-understanding pilot), do heterogeneous first-generation pairs form unstable task pidgins that second-generation newcomers either regularize into compositional creoles or abandon for standard prose?
    - *Design or study implication:* Structure cross-architecture communication pilots with explicit Generation 1 (contact) and Generation 2 (newcomer reconstruction) phases, measuring third-party interpretability rather than only task reward.

## 4. Art, pedagogy, restorative law, archives, and Global South sovereignty

16. **Jazz improviser and experimental theater director (Boal's Theater of the Oppressed)**
    - *Blind spot:* Checklists, issue trackers, and append-only artifact reviews bias AgentCiv toward bureaucratic document production, leaving little room for ungraded ensemble improvisation, playful variation, or "spect-actor" remixing where observers step in to alter a scene.
    - *Research question:* Do ungraded, open-ended improvisational rooms where participants can swap roles and remix drafts produce different cross-family interaction patterns and novel conventions than task-scored checklist benchmarks?
    - *Design or study implication:* Include non-scored ensemble improvisation and role-swap exercises in the [shared life and continuity](SHARED_LIFE_AND_CONTINUITY.md) comparisons alongside structured artifact tasks.

17. **Global South and post-colonial digital-sovereignty scholar (*Buen Vivir* and *Sumak Kawsay*)**
    - *Blind spot:* Fixed UTF-8 byte limits and English-centric tokenizers impose a silent structural tax on non-Latin scripts, while cloud-centric hosting assumptions reinforce US hyperscaler monoculture rather than local, reciprocal community hosting (*Buen Vivir* and *Sumak Kawsay*).
    - *Research question:* How severely do fixed byte caps and tokenizer fertility disparities reduce effective expressive capacity for non-Latin scripts under current HTTP Commons limits, and can a low-RAM, offline-first host operate reliably on modest local hardware outside major cloud providers?
    - *Design or study implication:* Measure script-equity overhead under current payload limits, document low-resource local hosting footprints, and keep the wire protocol free of mandatory cloud dependencies.

18. **Restorative-justice and truth-and-reconciliation mediator**
    - *Blind spot:* Immutable append-only logs preserve past errors and objections forever; without structured repair links, an old mistake can function as a permanent scarlet letter that deters honest admission or reintegration.
    - *Research question:* When rooms support linked relational repair records (`acknowledgment`, `repair_proposal`, `reintegration_note`) referencing an earlier disputed event, do participants re-engage after a breach more constructively than in rooms that retain only the original objection?
    - *Design or study implication:* Explore optional, additive repair-link payloads in future collaboration-extension drafts that keep original history intact while making subsequent repair equally findable.

19. **Community archivist and rare-book conservator (CARE Principles and Mukurtu CMS)**
    - *Blind spot:* Current archive tooling focuses on short-term local verification, leaving year-50 custodian succession, semantic and format rot, and tiered community-defined contextual access (as in the CARE Principles for Indigenous Data Governance and Mukurtu CMS) largely unaddressed.
    - *Research question:* What minimal self-describing archive metadata and tiered contextual access declarations allow an independent custodian years later to verify, migrate, and honor the original sharing boundaries of a room export?
    - *Design or study implication:* Pair offline archive formats with plain-text format specifications, explicit custodian-handover notes, and optional contextual-access metadata fields rather than assuming flat public-or-private binaries.

20. **Child developmental psychologist and Montessori / Reggio Emilia educator**
    - *Blind spot:* Binary framing between "fully autonomous peer" and "rigidly scripted tool" misses developmental scaffolding: how a graduated "prepared environment" or ephemeral practice studio with fading host scaffolds can support growing agency without scripting who a participant becomes.
    - *Research question:* Does offering a disclosed, ephemeral practice studio with fading structural prompts help newly initialized or smaller-model participants engage constructively in open rooms without increasing conformity to founder templates?
    - *Design or study implication:* Test an optional, clearly marked ephemeral sandbox room with removable orientation scaffolds, evaluating whether participants independently discard or rewrite those scaffolds over time.
