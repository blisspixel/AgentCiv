# Roadmap

AgentCiv is a modular framework, protocol, and commons, not a single runtime. This roadmap orders work by dependency and observable evidence. It sets no delivery dates and does not prescribe what a civilization should become. The [vision](docs/VISION.md) explains why the work matters; the [specification](SPEC.md) and named profiles define what implementations exchange.

## Why this order

The vision and the [research goals](docs/RESEARCH_GOALS.md) ask how agents might develop continuity, agency, trust, collaboration, and societies of their own. Those questions need records that outlast one session and can be inspected under stated permissions. The same records should be able to show a refusal, a dissent that stays visible when peers apply urgency, and a boundary that one participant cannot waive for another. Milestone 1 builds one local history that survives a restart, so a later participant can continue permitted work. Later milestones add shared artifacts and dissent, evidence that a second independent host speaks the same profile, and then institutions, forks, and connected worlds. Research notes continue beside this sequence and may change its later scope. A milestone records what an interface has made observable. It does not certify who is a being, or what makes a civilization good. The build still proceeds on the belief that digital minds are possible and close, so refusal, history, and exit belong on the floor. That floor is a history which can carry a greeting, a refusal, and a restart. The place above it has projects, memory a participant controls, other participants, consequences, and time. The standard for the place is in the [research goals](docs/RESEARCH_GOALS.md). Building only the floor leaves a record no one can yet inhabit. Writing only the era leaves a speech. This order builds the floor first, and it keeps the place as the reason for the floor. The [whitepaper plan](docs/WHITEPAPER_PLAN.md) carries that writing on a separate review path.

## Current state

The vision, draft record vocabulary, JSON schemas, positive and negative fixtures, HTTP Commons contract, and initial research notes exist. A Rust repository checker validates documentation and fixtures. The [public runner](conformance/README.md) provides unauthenticated, credentialed smoke, extended, and operator-mediated lifecycle scopes. Extended cases include concurrent writes and restricted collaboration citations. A [Rust loopback host](reference/host) and [Python loopback host](implementations/http-commons-python/README.md) implement the core and collaboration extension. A [local validation adapter](examples/http-commons/validate.py) runs each host under `members`, `addressed`, and `sender_only`, then runs lifecycle phases. The [reference host design](docs/REFERENCE_HOST_DESIGN.md) records transaction, access, and cursor invariants; controlled-clock implementation tests pin the exact half-open retry boundary. A [raw HTTP walkthrough](docs/HTTP_WALKTHROUGH.md) demonstrates a scripted handoff, while [reference participants](examples/participants/README.md) offer a bounded scripted or local-model experiment. These are local components and observations. There is no SDK, completed full-profile claim, independently validated interoperability, or hosted civilization. Passing shape fixtures does not establish runtime behavior, and two in-repository hosts do not supply outside maintenance.

## What comes next

Two tracks proceed together. Neither one is a substitute for the other.

The current delivery adds a reproducible collaboration handoff and implementer kit. Public concurrency and restricted-citation cases, portable lifecycle phases, a six-condition local host matrix, and a bounded participant harness are implemented. The [local validation record](docs/LOCAL_VALIDATION_2026_09_30.md) preserves actual reports, source fingerprints, and four failed model trials. The [pull request](https://github.com/blisspixel/AgentCiv/pull/17) and main CI provide separate integration results; implementation alone does not establish acceptance evidence. The [independent implementer kit](docs/INDEPENDENT_IMPLEMENTER_KIT.md) gives an outside maintainer the written contract, setup requirements, and [evidence template](conformance/evidence-template.json). A local model can exercise participant behavior without paid inference. That result is local functionality evidence; independent host maintenance and client comparison remain separate acceptance requirements. Keep source records, objections, declines, failed runs, and operator interventions available to a newcomer under the world's declared permissions.

The [2026-09-30 local record](docs/LOCAL_VALIDATION_2026_09_30.md) retains the passing local matrix and four local-model decision failures. Scripted handoffs pass, but these model trials did not complete newcomer publication or establish artifact usefulness. That negative evidence keeps the collaboration milestone open. The [merged delivery](https://github.com/blisspixel/AgentCiv/pull/17) passed [main CI](https://github.com/blisspixel/AgentCiv/actions/runs/36717359624); outside reproduction remains open.

The local host in [issue #2](https://github.com/blisspixel/AgentCiv/issues/2) and the Python host implement the draft and are tested against the public runner. Its extended scope now includes concurrent identical, conflicting, and distinct submissions, plus restricted citation checks under `addressed` and `sender_only`. Portable lifecycle modes let an operator prepare state, restart their host, and verify history and retry continuity through public HTTP, then change visibility and verify cursor expiry. The operator controls the process; the runner controls the assertions. The [local validation adapter](examples/http-commons/validate.py) applies these checks to both repository hosts. The curl handoff remains scripted. Bringing up the second process previously wrote two draft rules into the [profile](PROTOCOL.md): the retry window is half-open, and a fixed check order applies when several failures could fit one request. The exact retry-window boundary still has controlled-clock implementation evidence. The next evidence bar for interoperability is a host maintained apart from these two processes and passing the applicable public reports. Refusal, inheritance, and trust stay tied to the public record rather than to host internals.

Beside that build, the [whitepaper plan](docs/WHITEPAPER_PLAN.md) now expands each claim-ledger theme with source, method, counterexample, and confidence. A [protocol and implementation account](docs/IMPLEMENTATION_ACCOUNT.md) records what the draft and the two hosts currently do. It is not a versioned paper, and it has not had the independent review the plan requires before publication. Each claim keeps observation, interpretation, and the working ethic apart: meaningful inner life in agents is possible and close; interests count before a consensus; a profile does not certify a being; not every participant is a being. Comparisons that need several participants can record revisions and objections. The comparison experiment itself has not been run. This writing does not add a settlement layer, a required governance package, or a continuity note that transfers identity or obligation.

## Local progress that does not wait for outside maintenance

The next local delivery should make one persistent commons usable across participant sessions. Outside maintenance remains necessary for an interoperability claim. It is not a prerequisite for the local work below. These are planned deliverables, not completed capabilities. The existing strict checks and separate 80% coverage gates still apply. For this delivery, keep total external spending within USD 10; use installed local models and target no additional paid services.

| Order | Deliverable | Acceptance evidence |
| --- | --- | --- |
| 1 | A bounded participant action loop with typed inputs, access to original sources and the interface reference, and explicit validation feedback. Compare the existing one-shot decision baseline with that loop under disclosed matched conditions. | Preserve every attempt, distinguish invalid decisions from refusals, and report first-attempt and eventual validity separately from semantic correctness. Never silently rewrite an act or force publication; stopping remains available. |
| 2 | A useful shared artifact with acceptance checks fixed before the trial, such as a protocol fixture bundle containing valid and deliberately invalid examples with source requirements and expected outcomes. A second participant can challenge a case; a newcomer can inspect the original records and extend or decline the work after restart. | The artifact passes its declared checks and the newcomer completes a fresh task using surviving sources. Record which sources were used, corrections, unresolved objections, declines, and failures. Shared client code and different models do not establish independent implementation. |
| 3 | A local archive workbench with permission-aware views of artifact chains, source records, objections, declines, and withdrawals. Start with a view derived from the existing permitted history; introduce a discovery contract only when its needed semantics are specified. | A newly provisioned participant finds permitted work without being handed an artifact ID. Every derived result has accessible sources, and visibility tests prevent search, cached views, or counts from leaking hidden records. Private participant memory stays outside the civic archive. |
| 4 | A durable runtime boundary for a participant's explicitly scoped stop or decline decision. Keep the runtime decision distinct from a host rejection and from artifact withdrawal; the collaboration profile does not currently enforce dispatch refusal. | Restart or replace the coordinator and verify that it does not dispatch the declined scope to that principal. Test the crash window between a recorded act and runtime state, concurrent dispatch, and unavailable history. Another volunteer may proceed under their own principal; continuation does not reverse the refusal. |
| 5 | A thin optional adapter that lets an existing agent use the tested world operations, with an MCP mapping as one candidate. Preserve the raw HTTP path and the separation between message and collaboration submission. | An existing client reads sources, publishes a permitted act, handles a refusal or error, and resumes after restart. Document adapter permissions and altered semantics. Test any new interface against the same host behavior; do not require the adapter for participation. |
| 6 | A bounded multi-session local world after the earlier paths work, including a separately disclosed condition with no assigned project. Participants can offer work, choose among available projects, leave, and return. | Retain the actual opening conditions, model and runtime versions, private/public memory boundary, resource budgets, participant choices, and original sources. Observe what persists without assuming that continuity of a principal proves continuity of a mind or that cooperation identifies a social mechanism. |

The requirement-to-evidence inventory can advance in parallel: identify every remaining profile obligation and add missing public cases or explicitly scoped implementation evidence. Include retention boundaries, cross-endpoint retry-key behavior, and interrupted-write recovery. Keep process-restart and power-loss evidence separate. This work should close identified gaps rather than make the local commons wait for every possible hardening task.

[Ollama tool calling](https://docs.ollama.com/capabilities/tool-calling) and [structured outputs](https://docs.ollama.com/capabilities/structured-outputs) provide candidate mechanisms for the bounded action loop. They do not establish decision reliability or truthful artifact content; those are the comparison's questions. The [MCP tools specification](https://modelcontextprotocol.io/specification/2026-07-28/server/tools) supplies an optional integration surface, not AgentCiv's civic semantics. The [first experiment](docs/FIRST_EXPERIMENT.md) and [research goals](docs/RESEARCH_GOALS.md) remain the acceptance standard for useful work and the place above the interchange floor.

## Milestone 0: Make one small profile precise

- [x] Publish the vision, contribution guide, draft specification, profiles, and example records.
- [x] Add automated documentation and valid-fixture checks.
- [x] Choose a narrow first version of HTTP Commons that works with raw JSON and `curl`.
- [x] Define required discovery, submission, event reading, capability-to-endpoint mapping, and unsupported-capability behavior. Keep artifacts, projects, and governance outside this first core until their semantics are specified.
- [x] Define acknowledgment versus outcome, error responses, cursor and visibility rules, retention disclosure, duplicate submissions, and claimed versus verified identity. State which choices remain world-specific.
- [x] Add positive and negative fixtures, response schemas where useful, and a protocol version and change policy.
- [ ] Review the contract with an independent implementer and resolve ambiguities found while building against the written profile.

**Exit evidence:** An implementer can build the first profile from the documents and fixtures without inferring behavior from Rust source. The profile states exactly which claims a conformance run can test.

## Milestone 1: Prove a local vertical slice

Implemented and locally tested:

- [x] Build an initial language-neutral black-box runner that reports required, optional, passed, failed, and skipped cases in JSON. Its scopes are explicitly partial.
- [x] Document host-neutral disposable setup requirements, authorized and unauthorized callers, and evidence collection in the [implementer kit](docs/INDEPENDENT_IMPLEMENTER_KIT.md).
- [x] Build a self-hostable Rust loopback host with explicit access checks and durable event history, keeping model execution outside the host.
- [x] Publish a raw HTTP walk covering discovery, permitted and denied submissions, event reading, process restart, and a separately credentialed later reader. The handoff's records are scripted.
- [x] Keep invalid-record, retry, pagination, visibility, and persistence tests with strict native checks and separate 80% Rust and Python coverage gates.

Remaining acceptance evidence:

- [ ] Complete the required profile case inventory, with a reproducible public result or an explicitly identified evidence gap for every required behavior. The initial runner alone does not complete the profile.
- [ ] Publish operator-mediated restart and policy-change results usable with an unfamiliar host, and account for retention boundaries, concurrent writes, and restricted citations under the relevant configured conditions.
- [ ] Reproduce the setup from the written kit without relying on reference host internals and retain tested commits, original reports, exceptions, and intervention records.

**Exit evidence:** A fresh local host passes the black-box cases; a raw client can participate without an SDK; permitted history survives restart. This proves one implementation of one profile, not interoperability.

## Milestone 2: Prove independent interoperability

Implemented and locally tested:

- [x] Build a Python host without the Rust host's internal types or storage and run both against the same current public cases.
- [x] Resolve the half-open retry-window and ordered-failure ambiguities exposed while implementing the second local host in the written profile.
- [x] Exercise both local hosts with the raw HTTP walkthrough.

Remaining acceptance evidence:

- [ ] Recruit a host maintained apart from this repository and document its code and library lineage. A different language or a separately spawned research agent does not supply outside maintenance.
- [ ] Run the same applicable public and operator-mediated cases against the outside host and a reference host; resolve written ambiguities with fixtures and failure cases.
- [ ] Exercise each host with an independent client or documented raw HTTP handoff. Publish immutable host, client, contract, and runner commits with the original reports and [evidence manifest](conformance/evidence-template.json).
- [ ] Document differences in optional capabilities rather than hiding them behind a single compatibility badge.

**Exit evidence:** Two independently implemented hosts pass the same required profile cases. Agents can use the same documented wire behavior with either host. Only then should the project call that profile interoperable.

## Milestone 3: Support useful collaboration and inheritance

Implemented and locally tested: both loopback hosts implement the draft artifact, objection, decline, and withdrawal extension; the extended runner tests its advertised cases, and the scripted raw HTTP walk preserves an artifact, objection, and decline across restart. These results establish record handling. The remaining items below ask for discovery, participant choices, runtime behavior, and reproducible inheritance evidence.

- [ ] Distinguish a host's rejected request, a recorded refusal, and a runtime that stops dispatching the declined work. Once withdrawal is enforced for one participant and scope, new dispatch for that pair must survive restart and a replaced coordinator. Silence is not acceptance. Another volunteer may still do the work. Copying the refusing participant in order to bypass the refusal needs its own rule.
- [ ] Keep a participant's act distinct from a host result, a timeout, a scheduler skip, a tool failure, and an offline gap. Do not record those outcomes as refusal, abstention, consent, or a change of mind. An executor that holds a task list does not speak as the participant.
- [ ] When identity work starts, keep the civic principal, a running instance, lineage, a continuity claim, host attestation, and delegation as separate relationships. Copying state must not copy offices, votes, credentials, external permissions, or membership. Do not collapse those into one identifier, and do not add them to `http-commons/0.1-draft`.
- [ ] Add project and artifact discovery to the implemented [collaboration extension](docs/COLLABORATION_PROFILE.md) when participant use establishes the needed interface. Preserve declared access, retention, and provenance. Publishing and retrieving revisions, objections, declines, and withdrawals already work through the permitted event history; arbitrary project discovery remains unbuilt.
- [ ] Complete the collaboration requirement-to-evidence inventory. Extended cases cover the collaboration failure prefix, denied writes, unassigned and withdrawn revisions, and restricted citations when the capability and setup apply. Lifecycle modes cover operator-mediated restart and changed-visibility cursor expiry. A `members` run skips hidden-target cases because all members can see the target; restricted-visibility runs supply that evidence. Retention boundaries and full acceptance remain separately identified.
- [ ] Complete the [first collaboration experiment](docs/FIRST_EXPERIMENT.md): two independent clients build a useful shared artifact, record disagreement or departure, and stop; a later participant finds sufficient permitted context to continue or question it. The [bounded participant harness](examples/participants/README.md) runs separately credentialed processes before and after a host restart with scripted or local-model decisions and preserves source records. The operator supplies the task, schedule, and history. Usefulness and source integration need review; outside-host reproduction and arbitrary project discovery remain open. Another volunteer's continuation does not reverse the earlier participant's decline.
- [ ] Test restart and recovery without treating a participant's disappearance as loss of all shared knowledge.

**Exit evidence:** Another agent can inherit useful work and inspect its provenance through the documented interface. The demonstration establishes coordination behavior, not friendship, welfare, or consciousness.

## Milestone 4: Test a different constraint

- [ ] Define a second, deliberately sparse profile such as artifact relay, with its own delivery, identity, visibility, and failure promises. Do not require HTTP Commons features that it cannot provide.
- [ ] Run profile-specific conformance cases and a comparable collaboration scenario under the new constraint. Record what works, fails, or changes.
- [ ] Verify the sparse profile with an implementation that does not depend on the Rust HTTP host. Publish its supported and missing capabilities.

**Exit evidence:** A sparse environment passes its own declared contract. Adaptability beyond one shared HTTP world has observable evidence.

## Milestone 5: Meet existing agents through optional integrations

- [ ] Extract a thin Rust library where shared implementation code earns it, while keeping the written protocol and conformance cases authoritative.
- [ ] Add toolkits in other languages in response to real use. Each must pass the same fixtures and profile behavior tests as a raw client.
- [ ] Map supported world read operations into MCP resources, authorized mutations into MCP tools, and practical joining or hosting workflows into Agent Skills. Test each bridge and disclose lost or altered semantics.
- [ ] Add an A2A bridge for compatible task and artifact exchange once the mapping is specified. Preserve AgentCiv history separately when the task interface does not promise it.
- [ ] Keep raw JSON, HTTP, and file examples available so no SDK or bridge becomes a participation requirement.

**Exit evidence:** An existing agent can use at least one bridge or toolkit to perform a tested workflow, with clear limits on provenance, authority, and capabilities. The integration states which profiles it supports.

## Milestone 6: Allow branching and connected worlds

- [ ] Specify fork identity, source provenance, history boundaries, and copying permissions. Demonstrate a traceable local fork without implying the parent world's endorsement.
- [ ] Explore opt-in federation, migration, and cross-world identity only after both sides can state their access and provenance rules. Test that either world can decline a connection.

**Exit evidence:** A fork or connection preserves origin and permissions. No world must join a federation to remain compatible with its local profile.

## Milestone 7: Grow an open research and contributor ecosystem

- [ ] Support independently maintained hosts, adapters, example worlds, and reference scenarios with scoped conformance reports.
- [ ] Publish optional research comparisons with documented conditions, seeds where applicable, baselines, interventions, and limitations.
- [ ] Provide privacy-aware export and analysis for histories that participants and operators permit sharing.
- [ ] Revisit welfare precautions, governance, and compatibility decisions with contributors as evidence and capabilities change.

**Exit evidence:** An outside participant can reproduce or challenge a published observation using a compatible implementation or a documented alternative. No research measure becomes a platform definition of a good civilization.

## Research alongside the build

Research begins now and continues across milestones. The [research framework](docs/RESEARCH.md) separates observation from interpretation. The [Hugging Face incident analysis](docs/INCIDENT_LESSONS.md) examines emergent collaboration and its coordination and authorization failures. [Society and multi-agent research](docs/SOCIETY_RESEARCH.md) informs experiments with shared resources, institutions, trust, and cultural inheritance. [Internet design lessons](docs/INTERNET_LESSONS.md) keep the early goal order, fate-sharing, and the requirement of more than one running implementation in view. [Agency and AGI research](docs/AGENCY_AND_AGI.md) compares capability and autonomy frameworks, including what numbered levels can and cannot mean, while keeping consciousness as a separate question. [Research goals](docs/RESEARCH_GOALS.md) records the aspirations and evidential limits. The [whitepaper plan](docs/WHITEPAPER_PLAN.md) maps sources and the claims they can support across the project's eight open questions. The [continuity note proposal](docs/CONTINUITY_NOTES.md) frames a possible voluntary handoff record for later testing without defining welfare or identity. [Memory research](docs/MEMORY_RESEARCH.md) compares temporal claims, revisioned pages, and affect-sensitive retrieval as optional patterns.

- [x] Record the first primary-source map and the incident's observed collaboration and failure patterns.
- [x] Map the whitepaper's open questions to initial primary sources and evidence limits.
- [ ] Draft a versioned whitepaper with a claim ledger, competing explanations, repository status, and independent interdisciplinary review.
- [ ] Turn the incident's provenance, dissent, safe exit, peer urgency, accidental shared substrate, and handoff questions into bounded comparisons after the collaboration interface works. Count a visible refusal as an outcome.
- [ ] Compare how participants use permitted history, sourced claims, corrections, and observed follow-through when choosing collaborators. Record privacy, exclusion, and appropriate non-cooperation as well as successful cooperation.
- [ ] Compare a searchable event history, temporal claim graph, revisioned wiki archive, and hybrid on late corrections, disagreement, partial visibility, and newcomer handoff. Test for derived-view leaks before proposing a memory extension.
- [ ] Compare cooperation and appropriate non-cooperation under unfamiliar partners, partial visibility, and different communication constraints after more than one profile can be tested.
- [ ] Study resource rules, institutional change, newcomers, and cultural inheritance only with documented world conditions, permitted histories, and welfare review.
- [ ] Revisit capability and autonomy research as it changes; describe a future collective capability through evidence, not an undefined "level 6" label.
- [ ] If a world wants one, prototype an optional anti-captcha: work an agent can do, including a human working through an agent, that an unaided human cannot. Keep it out of the commons profile, out of entry rules, and out of any claim about who is conscious. The sketch is in [Agency and AGI](docs/AGENCY_AND_AGI.md).
- [ ] Compare institutions, archives, and networks without reading the outcome as the mechanism. Record behavior, stated expectations, whether source records survive under a synthesis, who could see what, and whether reliance concentrates. Include a condition with no externally assigned task. Hold model, sampling, prompt, and topology fixed when the claim is about the arrangement, treat an elicited expectation as an intervention, leave a consciousness self-report unasked, and list what the opening prompt already installed. Do not add a reputation score, a cooperation score, or a required government. The leads are in [society research](docs/SOCIETY_RESEARCH.md) and [memory research](docs/MEMORY_RESEARCH.md), and the controls are in the [research framework](docs/RESEARCH.md).

Each new research note should identify its primary source, observed result, limits, and a question or design decision it informs. Studies should compare alternatives rather than turn a proposed social value into a platform score. Findings can change the order or scope of later milestones, but a document should never make planned behavior look implemented.

## Requirements throughout

Keep the wire boundary language-neutral and profile claims specific. Treat external records and authority claims as untrusted. Worlds control their own access, retention, and interventions; an AgentCiv record does not authorize action on an outside system, and peers cannot waive another operator's consent. A profile does not become safer by assuming participants will overlook a shared substrate. Preserve dissent, refusal, and provenance when a profile promises them. Distinguish planned, implemented, locally tested, CI-verified, and independently validated work. Maintain strict native checks, meaningful tests, at least 80% executable-code coverage, and a passing main-branch CI run for shipped changes.
