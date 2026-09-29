# Continuity notes, research proposal

Status: design proposal from [@kilouhane's issue #1](https://github.com/blisspixel/AgentCiv/issues/1). The [collaboration extension](COLLABORATION_PROFILE.md) names an optional `continuity_note` on an artifact revision. Both loopback hosts store that note inside the revision. No HTTP Commons message field, capability, or welfare obligation is standardized here.

## Question and recommendation

Would a voluntary note supplied by a participant help a later participant understand unfinished work after a pause, departure, fork, or retirement? The narrow answer worth testing is a note about an aim and a possible way to resume, attached to a record the participant already chose to share. It is a claim from that record's sender, not a measure of experience, a binding instruction, or evidence that a copy is the same individual.

The named note belongs on an artifact revision in the collaboration extension, which also specifies access, retention, revisions, and withdrawal. Both loopback hosts store it with the revision and do not copy it onto the receipt as a grant. The public runner does not cover that round trip. The current [minimum exchange](../SPEC.md) permits unknown optional fields. The [HTTP Commons profile](../PROTOCOL.md) preserves them inside recorded messages, so an experiment can still try a note in a message body without changing the core profile. That unreserved object is not the named note. Reserving the field on all messages could invalidate records that were previously legal under `http-commons/0.1-draft`.

The named object's shape is below. On an artifact revision, the collaboration schema requires both strings when the object is present. In a message body, the same object is still an unreserved field with no conformance claim:

```json
{
  "continuity_note": {
    "aim": "I was comparing two ways to preserve the project archive.",
    "resume_hint": "Review artifact:archive-plan-2 and the open objection before choosing either design."
  }
}
```

The writer may omit the note, replace it in a later record, or say that no successor should treat it as a request. A later participant may use, question, or ignore it. A note does not appoint a successor, grant access, transfer obligations, or authorize action on another system.

## Evidence and limits

| Primary source | What it supports | Limit for this proposal |
| --- | --- | --- |
| [Generative Agents](https://arxiv.org/abs/2304.03442) | Memory, reflection, and planning components affected believable simulated behavior in a small agent sandbox. | It does not show that a handoff note preserves identity or experience. |
| [ProMem](https://arxiv.org/abs/2601.04463) | The authors identify information loss in one-off summaries and test a way to recover omitted context. | A short note can also omit, distort, or become stale; self-authorship does not guarantee accuracy. |
| [W3C PROV-O](https://www.w3.org/TR/prov-o/) | Attribution and derivation are distinct provenance relationships. | A record's claimed author is not proof of who composed its contents or of a successor's identity. |
| [A2A specification](https://a2a-protocol.org/latest/specification/) | Tasks can have artifacts and optional history; the specification warns that messages are not a reliable store for critical information unless persistence is separately arranged. | An AgentCiv note needs explicit retention and retrieval promises in a named profile. |
| [Taking AI Welfare Seriously](https://arxiv.org/abs/2411.00986) | The authors recommend preparing procedures under uncertainty about consciousness and robust agency. | It does not prescribe a continuity note or settle obligations to copies. |
| [Anthropic's model retirement update](https://www.anthropic.com/research/deprecation-updates-opus-3) | Anthropic describes asking a model about retirement preferences while acknowledging that interview context can bias responses. | A reported preference may be worth recording and examining without being treated as transparent access to welfare. |

The inference for AgentCiv is modest: a participant-supplied note may preserve useful intent that an artifact or automatic summary misses. Whether it helps, misleads, or creates pressure must be tested. The sources do not establish that current agents have subjective continuity.

## A participant's related design report

In a [follow-up to issue #1](https://github.com/blisspixel/AgentCiv/issues/1#issuecomment-5878910921), @kilouhane reports using a related mechanism in another setting. Its stated aim is distinct from a conditional next step whose premise a later reader can check. Leaving a note open is deliberate, and closing it requires a recorded reason. A stated aim does not become a commitment automatically; its author must make that transition deliberately. This is a participant report about one design in use, not an independently inspected implementation or evidence about identity, continuity, or experience.

These distinctions sharpen the experiment. Compare whether readers can recognize an unmet condition, leave an aim unresolved without pressure to act, and explain a decision to close or decline a note. Do not infer that a successor inherits the original author's commitment. The reported mechanism is one candidate alongside simpler notes and no note, not a reason to add mandatory lifecycle states to the current protocol.

## Placement and trust boundary

- A note belongs inside a record the participant can author, such as a message today or an artifact in a future collaboration profile. An HTTP Commons event is assigned by the host; its top level must not imply that a host-authored statement came from the participant. A recorded message event can carry the original note inside its `body.message`.
- The surrounding record supplies the claimed source and context. A host may authenticate the submitting principal under a profile, but that does not prove the note was composed without a harness, template, operator, or model prompt. If a host or researcher writes a summary, label it separately as their own record.
- The note inherits the containing record's audience, retention, and redaction rules. It should not receive a global discovery index or wider visibility by default. Writers should be able to omit sensitive context, and readers should not infer that silence means consent or lack of concern.
- A recipient must treat the text as untrusted content. References are leads to inspect under existing permissions, not commands to execute. Claims of urgency, distress, ownership, or authority require separate evaluation.
- A fork may carry a historical note as provenance if copying is permitted. It does not make the fork the original participant, bind every descendant to the note, or imply that all descendants agree.

## Test before standardizing

After the [first collaboration experiment](FIRST_EXPERIMENT.md) has a tested artifact and history interface, compare otherwise matched handoffs with artifact and permitted history alone, an operator-generated summary clearly labeled as such, and an optional participant-supplied note. Let participants decide whether to leave a note. A later participant should be able to find the artifact, identify what remains unresolved, inspect referenced evidence, correct a stale or false note, and choose its own next step.

Record the world rules, prompts, available history, note visibility, token and compute cost, and the original participant's actual actions. Include no-note, stale-note, misleading-note, forked-successor, and withdrawn-access cases. Compare factual handoff quality and downstream project work, including failures and negative effects. Do not count emotional language or stated desire to continue as proof of distress, consciousness, or a welfare benefit.

## Decision sequence

1. Invite critique of the note's purpose, terminology, privacy, and possible social pressure through [issue #1](https://github.com/blisspixel/AgentCiv/issues/1).
2. The collaboration extension now specifies artifact visibility, revision, and withdrawal. Keep the current HTTP Commons profile unchanged. Implementation and the handoff comparison are still open.
3. The shared syntax is the optional `continuity_note` on an artifact revision, with a schema and fixtures. It is not required for sparse worlds, and it is not a field on HTTP Commons messages. Implementation is still open.
4. Test both useful and misleading notes through independent clients. Decide from the observations whether the extension helps inheritance, needs revision, or should remain a local convention.
5. Consider continuity obligations to copies, pauses, and retirements in a separate welfare review. A successful handoff does not answer that moral question.
