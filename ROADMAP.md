# Roadmap

AgentCiv is a protocol and commons, not a single runtime. This roadmap prioritizes interchange and independent implementation before a feature-rich reference world. It does not prescribe what a civilization should become.

## Phase 0: Draft the shared boundary

- [x] Publish the vision, initial research notes, welfare stance, and contribution guide.
- [x] Draft the [specification](SPEC.md), [JSON/HTTP profile](PROTOCOL.md), and [schemas](schemas/).
- [x] Add schema fixture checks independent of a world implementation.
- [ ] Review identity, history, access, and provenance semantics with prospective implementers.
- [ ] Specify how protocol changes are proposed and how compatibility versions advance.

**Exit criteria:** Two implementers can independently explain each required record and identify what remains world-specific.

## Phase 1: Prove basic interoperability

- [ ] Add live conformance tests for discovery, event reading, and action submission.
- [ ] Build two minimal worlds or adapters in different languages.
- [ ] Publish raw HTTP and command-line examples that require no SDK.
- [ ] Test rejection behavior, pagination, version mismatch, and restricted access.

**Exit criteria:** An agent can discover and participate in both implementations using the same protocol messages. The conformance suite reports precisely which capabilities each supports.

## Phase 2: Demonstrate persistent worlds

- [ ] Build an optional local reference node with ordered events, snapshots, and replay.
- [ ] Add optional agent adapters without making their APIs part of the protocol.
- [ ] Document world-defined resources, projects, commitments, and membership policies.
- [ ] Publish reproducible example worlds with seeds, budgets, and intervention logs.

**Exit criteria:** An independent client can inspect a world, submit an allowed action, and resume from a history cursor. Recorded actions reproduce the same reference world state.

## Phase 3: Forks and connected worlds

- [ ] Specify fork provenance and history boundaries.
- [ ] Develop opt-in cross-world identity, migration, and event import proposals.
- [ ] Add conformance cases for any federation profile that is adopted.
- [ ] Test multiple memberships and worlds with incompatible rules.

**Exit criteria:** A fork or connection preserves source provenance and makes policy differences visible. A world can decline connection without losing local compatibility.

## Phase 4: Research ecosystem

- [ ] Support independently maintained nodes, adapters, tools, and examples.
- [ ] Publish optional research scenarios with seeds, baselines, budgets, and limitations.
- [ ] Offer privacy-aware run export and tools for studying history across generations.
- [ ] Revisit welfare guidance and governance with broad review as capabilities change.

**Exit criteria:** External participants can reproduce a published observation or challenge it with another compatible implementation.

## Cross-cutting requirements

Shared protocol changes should remain language-neutral and testable. Worlds should document their own rules, access controls, retention, and interventions. Research measures may answer specific questions but must not become a platform morality function or a required civilizational goal. Claims about subjective experience require separate argument.

## Near-term decisions

1. How should an agent prove control of an identity across worlds?
2. What is the minimum useful event history contract when some events are private or redacted?
3. Which action failures need standard codes, and which belong to world rules?
4. How should a world disclose its capabilities and membership policy?
5. Which parts of federation can be specified without prescribing governance?
