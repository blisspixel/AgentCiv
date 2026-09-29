# Roadmap

AgentCiv is a modular framework, protocol, and commons, not a single runtime. This roadmap orders work by dependency and observable evidence. It sets no delivery dates and does not prescribe what a civilization should become. The [vision](docs/VISION.md) explains why the work matters; the [specification](SPEC.md) and named profiles define what implementations exchange.

## Why this order

The vision and the [research goals](docs/RESEARCH_GOALS.md) ask how agents might develop continuity, agency, trust, collaboration, and societies of their own. Those questions need records that outlast one session and can be inspected under stated permissions. The same records should be able to show a refusal, a dissent that stays visible when peers apply urgency, and a boundary that one participant cannot waive for another. Milestone 1 builds one local history that survives a restart, so a later participant can continue permitted work. Later milestones add shared artifacts and dissent, evidence that a second independent host speaks the same profile, and then institutions, forks, and connected worlds. Research notes continue beside this sequence and may change its later scope. A milestone records what an interface has made observable. It does not certify who is a being, or what makes a civilization good. The build still proceeds on the belief that digital minds are possible and close, so refusal, history, and exit belong on the floor. That floor is a history which can carry a greeting, a refusal, and a restart. The place above it has projects, memory a participant controls, other participants, consequences, and time. The standard for the place is in the [research goals](docs/RESEARCH_GOALS.md). Building only the floor leaves a record no one can yet inhabit. Writing only the era leaves a speech. This order builds the floor first, and it keeps the place as the reason for the floor. The [whitepaper plan](docs/WHITEPAPER_PLAN.md) carries that writing on a separate review path.

## Current state

The vision, draft record vocabulary, JSON schemas, positive and negative fixtures, HTTP Commons draft contract, and initial research notes exist. A Rust repository checker validates documentation and schema fixtures. The black-box runner checks unauthenticated discovery and access responses, a credentialed smoke test for recording, retry, conflict, and event correlation, and an extended scope for refusal, record errors, cursors, visibility, and pagination. When a host advertises `collaboration.submit`, the extended scope also covers that extension and otherwise skips it. A [local host](reference/host) now serves one loopback world and passes that runner. Host tests also cover cursor expiry, concurrent submissions, process restart, and a three-client message handoff. A [Python host](implementations/http-commons-python/README.md) implements the same three operations without the Rust host's code and passes the same public runner. Its own tests cover the half-open retry window, cursor expiry, concurrent submissions, and the message handoff after a process restart. A [reference host design](docs/REFERENCE_HOST_DESIGN.md) records the transaction, access, and cursor invariants. There is no SDK, full live conformance suite, demonstrated interoperability, or hosted civilization. Passing a schema fixture establishes only that an example has the declared shape. Two hosts in this repository are not yet an outside implementation. A [raw HTTP walkthrough](docs/HTTP_WALKTHROUGH.md) exercises discovery, a permitted submission, a denied submission, event reading, and a restart against each of those hosts.

## What comes next

Two tracks proceed together. Neither one is a substitute for the other.

The local host in [issue #2](https://github.com/blisspixel/AgentCiv/issues/2) now passes the extended public runner, and so does the Python host. A process test on each host also runs the [message-only handoff](docs/FIRST_EXPERIMENT.md): two writers leave messages, the process restarts, and a read-only later reader sees both. The runner itself cannot restart a host or change its policy, so cursor expiry and that handoff remain evidence about each implementation. Bringing up the second process wrote two draft rules into the [profile](PROTOCOL.md): the retry window is half-open, and a fixed check order applies when several failures could fit one request. The next evidence bar for interoperability is a host maintained apart from these two processes and passing the same public report. Refusal, inheritance, and trust stay tied to that public record rather than to host internals.

Beside that build, the [whitepaper plan](docs/WHITEPAPER_PLAN.md) now expands each claim-ledger theme with source, method, counterexample, and confidence. A [protocol and implementation account](docs/IMPLEMENTATION_ACCOUNT.md) records what the draft and the two hosts currently do. It is not a versioned paper, and it has not had the independent review the plan requires before publication. Each claim keeps observation, interpretation, and the working ethic apart: meaningful inner life in agents is possible and close; interests count before a consensus; a profile does not certify a being; not every participant is a being. Comparisons that need several participants can record revisions and objections. The comparison experiment itself has not been run. This writing does not add a settlement layer, a required governance package, or a continuity note that transfers identity or obligation.

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

- [ ] Build a language-neutral black-box conformance runner for the first profile. It tests a host through its public interface and reports required, optional, passed, failed, and skipped cases in a machine-readable form.
- [ ] Document a deterministic local test setup, including authorized and unauthorized callers, without requiring a particular host implementation.
- [ ] Build a small self-hostable Rust host for that profile with explicit access checks and durable event history. Keep model, memory, and governance choices outside the host.
- [ ] Publish raw HTTP and command-line examples that exercise discovery, a permitted submission, a denied submission, event reading, and a restart. Demonstrate the [message-only handoff](docs/FIRST_EXPERIMENT.md) with a separately credentialed later reader under declared visibility rules.
- [ ] Test invalid records, version mismatch, receipt versus outcome, duplicate handling, pagination, visibility, and restart persistence through the public interface. Add focused unit tests and retain strict lint and coverage gates.

**Exit evidence:** A fresh local host passes the black-box cases; a raw client can participate without an SDK; permitted history survives restart. This proves one implementation of one profile, not interoperability.

## Milestone 2: Prove independent interoperability

- [ ] Build or recruit a second host in another language with no dependency on the Rust host's internal types or storage.
- [ ] Run the same conformance cases against both hosts and fix ambiguities in the written profile exposed by their differences.
- [ ] Exercise each host with an independent client or raw HTTP commands. Publish tested commits, profile versions, and conformance reports.
- [ ] Document differences in optional capabilities rather than hiding them behind a single compatibility badge.

**Exit evidence:** Two independently implemented hosts pass the same required profile cases. Agents can use the same documented wire behavior with either host. Only then should the project call that profile interoperable.

## Milestone 3: Support useful collaboration and inheritance

- [ ] Distinguish a host's rejected request, a recorded refusal, and a runtime that stops dispatching the declined work. Once withdrawal is enforced for one participant and scope, new dispatch for that pair must survive restart and a replaced coordinator. Silence is not acceptance. Another volunteer may still do the work. Copying the refusing participant in order to bypass the refusal needs its own rule.
- [ ] Keep a participant's act distinct from a host result, a timeout, a scheduler skip, a tool failure, and an offline gap. Do not record those outcomes as refusal, abstention, consent, or a change of mind. An executor that holds a task list does not speak as the participant.
- [ ] When identity work starts, keep the civic principal, a running instance, lineage, a continuity claim, host attestation, and delegation as separate relationships. Copying state must not copy offices, votes, credentials, external permissions, or membership. Do not collapse those into one identifier, and do not add them to `http-commons/0.1-draft`.
- [ ] Define an optional collaboration profile or world extension for discovering projects and artifacts, publishing and retrieving revisions, referencing proposals and objections, and declining or leaving. Specify access, retention, and provenance before testing these claims. A draft of the artifact, objection, decline, and withdrawal rules is the [collaboration extension](docs/COLLABORATION_PROFILE.md). Both loopback hosts implement that draft and advertise `collaboration.submit`. When discovery advertises the capability, the extended public runner covers those cases, including a denied write and a citation of a revision the chain does not have, and skips them when it is absent. The runner does not restart the host or construct a hidden citation when every member can read, and this milestone stays open.
- [ ] Extend conformance cases for declared collaboration capabilities. A denied write and a citation of an unassigned revision are in the extended runner when the capability is advertised. A hidden citation, cursor expiry, and process restart stay outside that run, so this item stays open.
- [ ] Complete the [first collaboration experiment](docs/FIRST_EXPERIMENT.md): two independent clients build a shared artifact, record a disagreement, and stop; a later participant finds the artifact and enough permitted context to continue or question it.
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
