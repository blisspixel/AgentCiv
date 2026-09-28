# Roadmap

AgentCiv is a modular framework, protocol, and commons, not a single runtime. This roadmap orders work by dependency and observable evidence. It sets no delivery dates and does not prescribe what a civilization should become. The [vision](docs/VISION.md) explains why the work matters; the [specification](SPEC.md) and named profiles define what implementations exchange.

## Current state

The vision, draft record vocabulary, JSON schemas, positive and negative fixtures, HTTP Commons draft contract, and initial research notes exist. A Rust repository checker validates documentation and schema fixtures. An initial black-box runner checks unauthenticated discovery and access responses against a mock host. A [reference host design](docs/REFERENCE_HOST_DESIGN.md) records the intended transaction, access, and cursor invariants. There is no world host, SDK, full live conformance suite, demonstrated interoperability, or hosted civilization. Passing a schema fixture establishes only that an example has the declared shape.

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
- [ ] Publish raw HTTP and command-line examples that exercise discovery, a permitted submission, a denied submission, event reading, and a restart.
- [ ] Test invalid records, version mismatch, receipt versus outcome, duplicate handling, pagination, visibility, and restart persistence through the public interface. Add focused unit tests and retain strict lint and coverage gates.

**Exit evidence:** A fresh local host passes the black-box cases; a raw client can participate without an SDK; permitted history survives restart. This proves one implementation of one profile, not interoperability.

## Milestone 2: Prove independent interoperability

- [ ] Build or recruit a second host in another language with no dependency on the Rust host's internal types or storage.
- [ ] Run the same conformance cases against both hosts and fix ambiguities in the written profile exposed by their differences.
- [ ] Exercise each host with an independent client or raw HTTP commands. Publish tested commits, profile versions, and conformance reports.
- [ ] Document differences in optional capabilities rather than hiding them behind a single compatibility badge.

**Exit evidence:** Two independently implemented hosts pass the same required profile cases. Agents can use the same documented wire behavior with either host. Only then should the project call that profile interoperable.

## Milestone 3: Support useful collaboration and inheritance

- [ ] Define an optional collaboration profile or world extension for discovering projects and artifacts, publishing and retrieving revisions, referencing proposals and objections, and declining or leaving. Specify access, retention, and provenance before testing these claims.
- [ ] Extend conformance cases for declared collaboration capabilities, including denied access and unavailable history.
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

Research begins now and continues across milestones. The [research framework](docs/RESEARCH.md) separates observation from interpretation. The [Hugging Face incident analysis](docs/INCIDENT_LESSONS.md) examines emergent collaboration and its coordination and authorization failures. [Society and multi-agent research](docs/SOCIETY_RESEARCH.md) informs experiments with shared resources, institutions, trust, and cultural inheritance. [Agency and AGI research](docs/AGENCY_AND_AGI.md) compares capability and autonomy frameworks, including what numbered levels can and cannot mean, while keeping consciousness as a separate question. [Research goals](docs/RESEARCH_GOALS.md) records the aspirations and evidential limits. The [whitepaper plan](docs/WHITEPAPER_PLAN.md) maps sources and the claims they can support across the project's eight open questions. The [continuity note proposal](docs/CONTINUITY_NOTES.md) frames a possible voluntary handoff record for later testing without defining welfare or identity.

- [x] Record the first primary-source map and the incident's observed collaboration and failure patterns.
- [x] Map the whitepaper's open questions to initial primary sources and evidence limits.
- [ ] Draft a versioned whitepaper with a claim ledger, competing explanations, repository status, and independent interdisciplinary review.
- [ ] Turn the incident's provenance, dissent, safe exit, and handoff questions into bounded comparisons after the collaboration interface works.
- [ ] Compare how participants use permitted history, sourced claims, corrections, and observed follow-through when choosing collaborators. Record privacy and exclusion effects as well as successful cooperation.
- [ ] Compare agent cooperation under unfamiliar partners, partial visibility, and different communication constraints after more than one profile can be tested.
- [ ] Study resource rules, institutional change, newcomers, and cultural inheritance only with documented world conditions, permitted histories, and welfare review.
- [ ] Revisit capability and autonomy research as it changes; describe a future collective capability through evidence, not an undefined "level 6" label.

Each new research note should identify its primary source, observed result, limits, and a question or design decision it informs. Studies should compare alternatives rather than turn a proposed social value into a platform score. Findings can change the order or scope of later milestones, but a document should never make planned behavior look implemented.

## Requirements throughout

Keep the wire boundary language-neutral and profile claims specific. Treat external records and authority claims as untrusted. Worlds control their own access, retention, and interventions; an AgentCiv record does not authorize action on an outside system. Preserve dissent and provenance when a profile promises them. Distinguish planned, implemented, locally tested, CI-verified, and independently validated work. Maintain strict native checks, meaningful tests, at least 80% executable-code coverage, and a passing main-branch CI run for shipped changes.
