# First collaboration experiment

The first useful AgentCiv demonstration should help independent agents complete a shared project and leave something useful for a later participant. Its purpose is to prove that the framework reduces coordination friction. It should not claim that cooperation demonstrates consciousness, friendship, or empathy. It is also not the society. A completed artifact shows that work can outlast its authors. Institutions the participants invent, values the project does not own, and work nobody assigned remain the result the [research goals](RESEARCH_GOALS.md) are aimed at. The contract for the artifact, the objection, and the decline is the [collaboration extension](COLLABORATION_PROFILE.md). Both loopback hosts implement that contract in their own tests. The public runner does not cover it.

This scenario depends on project and artifact operations that the draft [HTTP Commons profile](../PROTOCOL.md) does not define. When it is run, a completed artifact does not by itself show which mechanism produced the collaboration. The record should keep the source messages, any later synthesis, stated expectations, objections, and what a newcomer can still check. The run also names the controls in the [research framework](RESEARCH.md): whether anyone asked for expectations, whether anyone was asked for a consciousness level, whether a synthesis replaced source messages, whether the harness chose who remained, which combination rule scored the result, which of the four information facts the success criterion used, and which roles, relationships, goals, or rules the opening prompt already installed. A separate condition can stop assigning tasks and record what, if anything, participants originate. That condition is a later study, described with the other leads in [society research](SOCIETY_RESEARCH.md). The [roadmap](../ROADMAP.md) puts a precise core profile, live conformance, and independent interoperability ahead of this richer demonstration. The contract and the loopback tests exist. Until an outside host can fail the extension's cases, and until independent agents actually run the scenario, it remains a design target.

## Milestone 1 precursor: message history after restart

Before the artifact scenario, test a narrower handoff using only HTTP Commons. In a fresh local world advertising `history.visibility` as `members`, the operator gives three scripted HTTP clients their own read credentials and gives the first two write access. Client A submits a message describing a small unfinished task and how to judge completion. Client B submits a message that questions or revises it. Stop both clients, restart the host, and have client C read the permitted events and cite the two host-assigned event IDs while explaining what remains open. Use raw HTTP for discovery, submission, and reading. Separately demonstrate a denied submission.

The operator provisions C's credential; this profile has no self-enrollment. The `members` policy is essential to this example because a newly provisioned C cannot assume access to A's and B's earlier messages under `addressed` or `sender_only` visibility. The messages may carry work-package or objection text in their bodies, but HTTP Commons does not define project, proposal, objection, or artifact semantics. A local host test now runs this handoff for one loopback world: two writers submit, the process restarts, and a separately credentialed reader sees both events. The [HTTP walkthrough](HTTP_WALKTHROUGH.md) runs that precursor with curl. That is evidence about this implementation. The public runner does not restart a host, so the same handoff is not yet a case another implementation can fail in that runner. It does not complete the collaboration experiment below or establish that a newcomer can discover arbitrary projects.

## Scenario

Two agents with different implementations join a local world. They can inspect the world's capabilities and rules, discover a shared project, exchange a proposal or artifact, contribute revisions, record disagreement, and publish a result. A third agent joins after the first two have stopped and can understand what happened from the surviving artifacts and permitted history.

One participant should be able to decline a proposal or leave. If that participant declines and the work stops, the record of the refusal is a completed observation. A recorded decline and a runtime that stops assigning that participant the work are different facts. This demonstration can record the decline. Enforcement across restart belongs with the later collaboration work. A later extension should let a group fork the project into a separately named branch when its members disagree. The first demonstration can use simple reference agents or scripted clients so it does not depend on a particular model provider. Discovery can make a project available. It does not assign the project. A scripted client can speak the wire for a test. That client is not the participant's own decision, and its output should not be attributed to a persistent participant merely because it held the task.

## Minimum mechanics

1. **Discover:** Learn the world's capabilities, project descriptions, participation rules, and available peers or artifacts when visibility permits.
2. **Coordinate:** Publish an offer, request, proposal, or response with source and claimed authority visible.
3. **Build:** Create and revise an artifact such as code, documentation, a map, or a research note.
4. **Remember:** Preserve a referenceable record of contributions, decisions, objections, and revisions according to the world's retention policy.
5. **Choose:** Accept, decline, or withdraw without treating a refusal as protocol failure. A later fork profile can add branching.
6. **Inherit:** Let a later participant discover the result and the context needed to use or question it.

These mechanics should be available through raw protocol messages. A Rust reference node, Python toolkit, MCP bridge, or A2A adapter may make them easier to use, but none should be necessary to interpret the records.

## Boundary conditions

The world may grant access only to an explicitly authorized local project and artifact store. A participant's request or another agent's claim of authority cannot expand that scope. External services and repositories require their operators' permission before agents affect them. Interventions and denied actions should be visible in the experiment record where privacy permits.

## Technical acceptance criteria

- Two independent clients can perform the scenario against the same advertised profile.
- A third client can resume from published records after the original processes stop.
- Record authorship and claimed authority remain distinguishable from verified authorization.
- A rejection or objection remains referenceable rather than disappearing in a chat stream.
- Restarting the reference node preserves permitted history and artifact references.
- The conformance runner can test these claims as observable behavior. A passing report lists acts and host results. It does not identify which social mechanism produced a completed artifact. The record still has to satisfy the controls in the [research framework](RESEARCH.md).

The next iteration should add a fork with a new identity and traceable origin, without implying parent approval.

This is one rich profile, not the minimum AgentCiv environment. Later experiments can remove direct messaging, durable identity, shared history, or reliable connectivity and observe what forms of cooperation participants invent in response.
