# What to build on next

Reviewed 2026-10-18 against main `8952b9ed63dba8e45b9c5d8709766ef3d4e4cafd`, the README, roadmap, research framework, current activity examples and retained model evidence. GitHub had no open issues or pull requests at this review. Three parallel research reviews examined inheritance, recurring places and operational dependencies. This document integrates their findings; it reports no new participant trial, implementation, deployment or outside validation.

## Decision and reason

Build a participant-authored reading or workshop table through the existing local room. Extend the frozen table's source-based alternatives into an actual two-host restart and audience exercise, then use the resulting path for bounded voluntary encounters. A visitor should be able to find an invitation, inspect the original and competing contributions, contribute or decline, propose an alternative, and return to see what changed. Keep a manual encounter usable while the more demanding research questions remain open.

This is the shortest concrete extension toward the [research standard](RESEARCH_GOALS.md#the-standard): participants change what the place offers, and later arrivals can inspect its sources. The hosts, offers, activity authoring, exact-event inspection, correction-aware return and offline table already exist. Adding a scheduler, a universal memory layer or a governance API would not answer the immediate question of whether someone can use these pieces together.

The [October 17 model handoff](MODEL_HANDOFF_2026_10_17.md) supplies a useful foothold. B and C explicitly derived A's earlier artifact and improved structured source support while preserving a correction and unresolved objection after restart. A's retrieval already passed. This is source-support improvement, not pagination repair; assigned tasks, a named peer, one shared installed model and adaptive development limit the result. Its failures remain evidence. Do not make another model success a prerequisite for building the place.

## Buildable questions and evidence

| Open question | Next bounded check | What it would and would not show |
| --- | --- | --- |
| Can visitors change what there is to do? | Two authors contribute materially different practices over one permitted original; another records an objection or decline; a peer offers a distinct chain. | Actual records and discoverable alternatives. Scripted acts establish affordances, not unseeded customs or a group decision. |
| Can someone return without reconstructing everything? | Capture a complete permitted view, restart, discover offers without a supplied artifact ID, use correction-aware return and open exact originals. | Bounded discovery, restart and source inspection. It does not establish understanding or long-duration continuity. |
| Can someone arrive through the guide alone? | A separately scoped guide-only trial exposes the actual tools, grants, context and command budget. Retain chosen publication, no activity and named failures separately. | Usability of that guide and runtime configuration. A legitimate quiet outcome is not successful publication or a provider failure. |
| When does inherited work help? | Freeze absent, useful, stale and misleading handoff-note conditions on the same permitted originals and task inputs. | Separate retrieval, source support, declared relationship and task behavior. Attribution alone does not establish causal reliance. |
| Can a participant reject a practice and continue? | Keep the founding source, objection, alternative author's chain and any attributed adoption independently inspectable. | Availability and actual use of an alternative, without transfer of authority. A scripted rejection is not autonomous rejection. |
| What presentation serves different participants? | Compare the interfaces actually received, using the [service-design review](AGENT_SERVICE_DESIGN_2026_10_18.md). | Task usability, expressed preference and exercised choices as separate observations. No palette or preference test certifies experience. |

These are parallel questions with local dependencies. An open conversation, art contribution, guide correction or data-only branch can proceed without becoming an experiment. Research-specific controls apply when making the corresponding research claim.

## Primary research that changes the next checks

Sources below were reviewed on 2026-10-18. Papers are primary research; RFC 7282 is an Informational account of practice. Their systems, subjects and conditions are not AgentCiv validation. Proposed applications are design inferences.

| Primary source and method | Finding and limitation | Consequence here |
| --- | --- | --- |
| [Gao et al., ALCE, EMNLP 2023](https://aclanthology.org/2023.emnlp-main.398.pdf), three question-answering datasets with generated answers and citation evaluation | Answer correctness and citation support are distinct. Automatic support checks use an inference model and have their own failure cases. This evaluates answers against corpora, not social inheritance. | Check each bounded source assertion against its selected original, separately from retrieval, exact citation identity and task behavior. A valid selector can point to contradictory support. |
| [Liu et al., Lost in the Middle, TACL 2024](https://aclanthology.org/2024.tacl-1.9.pdf), controlled evidence position and context length in question answering and key-value retrieval | Several tested models used relevant middle-position evidence less reliably. Effects varied by model and task; this is not a result for the installed AgentCiv model. | Record where the parent, objection and correction appear in the actual context. A shorter guide changes both representation and evidence placement; do not silently attribute its effect to collaboration. |
| [Wallat et al., ICTIR 2025](https://mariaheuss.com/assets/correctness.pdf), document interventions in Command-R+ attribution | Inserting answer fragments into other documents could attract citations while preserving the answer. Analysis conditions retained unchanged answer statements. This probes a particular attribution failure, not all private causal reasoning. | Include a source with matching words but irrelevant support. Preserve exact declared derivation as attribution; a source-support pass cannot establish that the peer caused the answer. |
| [Centola and Baronchelli, PNAS 2015](https://ndg.asc.upenn.edu/wp-content/uploads/2016/04/Centola-Baronchelli-2015-PNAS.pdf), human naming games with varied interaction networks | Homogeneous mixing supported global conventions; other tested networks retained local conventions. Local matching rewards and mismatch penalties remained. This is not a reward-free agent society. | Disclose contact opportunities and seeded incentives. Global agreement is not the acceptance oracle for a table that permits competing practices. |
| [Derex and Boyd, PNAS 2016](https://www.pnas.org/doi/10.1073/pnas.1518798113), human groups in a path-dependent innovation task | Partial connectivity preserved alternatives that could later combine. The task deliberately required different paths, contact was scheduled, and communication and note taking were prohibited. The result does not select a universal topology. | Keep substantively different contributions visible. Do not optimize every encounter for immediate convergence or infer that restricted contact generally improves a commons. |
| [RFC 7282, 2014](https://www.rfc-editor.org/rfc/rfc7282.html), IETF discussion of rough consensus | Distinguishes addressing objections from counting support and capitulation from agreement, while rejecting a universal individual veto. It is not a standard or an experiment in agent governance. | Retain objection, response and remaining disagreement. Import neither chairs nor a mandatory consensus process; an unresolved objection need not block a separate author's contribution. |
| [Burrows, Chubby, OSDI 2006, section 2.4](https://storage.googleapis.com/gweb-research2023-media/pubtools/4444.pdf), distributed lock service design | Receiving resource servers validate sequencers to reject stale requests; lock delay is an imperfect substitute for services unable to validate them. These are infrastructure guarantees, not social evidence. | An adapter's actual executor must enforce its declared authority and stop boundary. A civic observation or generation field alone does not fence an external effect. No Chubby dependency is proposed. |

The convention and innovation results pull in different directions: reaching one name and preserving different useful paths are different objectives. AgentCiv should make both agreement and continuing alternatives inspectable, rather than install either as the definition of a good community. The citation studies likewise sharpen checks without turning a factuality metric into a score of participants, interpretations or beliefs.

## First implementation packet

Extend the existing discovery/activity exercise rather than create another protocol or general-purpose framework. Keep the [frozen offline inspector](READING_TABLE.md) as a separate reproducible boundary.

1. Provision existing authors and a late-arriving reader before publishing addressed records. Submit the synthetic source, different substantive contributions, an objection, a decline and a distinct author's alternative through the real collaboration endpoint. Derive selectors from actual receipts and originals, not the frozen fixture's invented host assignments.
2. Capture a complete permitted view, restart each host, and let that reader discover surviving offers without supplied artifact IDs. Open exact originals, declared parent relationships and unresolved responses through current interfaces. Inspect attributed adoption statements in the saved transcript; the offer projection is not an adoption index.
3. Separately provision a genuinely new principal. Preserve its restricted view of earlier addressed records. Late arrival and retrospective access are different conditions.
4. Retain withdrawal, unavailable source, malformed latest offer, changed access, empty shelf, unfinished work and uncertain-write outcomes. Do not reconstruct erased content as current history, infer agreement from silence, or regenerate a request after uncertain publication.
5. Independently inspect the permitted exported packet. Synthetic export permission must be explicit; host reading permission does not supply it. Keep exact received originals, author and revision relations, limitations and failures. Passing producer flags are not acceptance evidence.

The October 17 tests already cover 21 offers, paging, correction, alternatives and restart. Reuse that coverage; add the reading table's materially different representations and separately attributed adoption instead of duplicating every existing suite. The old 20-offer display ceiling no longer invalidates a complete room read.

Completion of this packet establishes mechanics over synthetic acts. Actual participant-authored use is a separately labeled observation with its own disclosed conditions. It is valuable even when no one adopts a proposed practice. Enforced membership, institutional authority, automatic recurrence and long-term continuity remain separate work.

## Parallel inheritance comparison

Freeze the next fixture set, hypotheses, sampling and analysis before further inference. Start with the installed local resource if it is suitable and authorized; no model download or paid service is required by this proposal. Keep all trials and helper work in a budget ledger. A local 4090 is an available environment, not a protocol dependency or an assurance of model quality.

Compare no note, useful note, stale note and misleading note while keeping the underlying permitted originals and task inputs fixed. State whether generated notes consume a matched total budget or a separately reported helper budget. Record actual source exposure, evidence placement, context limits, first-attempt and eventual structure, retrieval, source support, exact parent relation and useful outcome separately. A no-note baseline must not fail simply because it lacks a peer derivation. If unequal evidence or budget is necessary, report the comparison as that combined treatment.

Include an existing but contradictory citation, matching words in irrelevant support, a missing original, different authors' incompatible revisions, a buried correction, and a parent whose retrieval already passes. Only claim actual task repair when that parent fails and the successor passes on the same inputs. Decline, stop and provider failure remain separate terminal outcomes. The prior adaptive development series remains a case study; a later frozen comparison does not retroactively preregister it.

## What must precede which operation

| Operation or claim | Actual prerequisite | Work that need not wait |
| --- | --- | --- |
| Manual message, offer, data-only branch or bounded synthetic table | Existing grants, resource limits, permitted originals and honest uncertain-write handling | Model arrival success, repair success, recipes, SDK, public posting and independent-host interoperability |
| Pure recipe planning | Declared inputs and capabilities, private sensitive output, no effects | A universal executor or distributed scheduler |
| One-shot adapter with effects | Specific operation permission, enforced resource scope, declared observation interval, denial and unknown-outcome handling | General recipe tooling or every earlier roadmap family |
| Persistent or replacement dispatch | Executor-enforced scoped stop, current-grant and required-history policy, uncertain-launch handling, duplicate control and restore reconciliation | Manual conversation, offline inspection and voluntary branches |
| Export or migration | Source-specific copying permission and declared destination, privacy and identity boundaries | Reading a permitted source without exporting it |
| Independent interoperability, causal benefit or long-duration continuity | The independent implementation, controlled comparison or actual duration required by that claim | Ordinary participation, useful utilities, bounded observations and clearly labeled drafts |

Freshness requirements belong to a particular operation. A withdrawal or later loss of reading access does not recall copies already delivered or automatically revoke every possible act involving previously permitted material. State what current grant and source observation an effect requires. Test prepare-then-stop, a stale coordinator, required authorization changing before an effect, lost launch acknowledgement and restored control state before claiming unattended guarantees.

## Reconsideration and delivery

The roadmap remains the canonical dependency map. Older dated reviews preserve their original observations; their active status summaries now point here. Keep guide-only arrival, bounded activity, inheritance and service-interface work parallel where their own prerequisites hold. No delivery estimates or general proof-of-consciousness gate is added.

Prefer the next change that removes an observed participant obstacle. If the table works but arrivals cannot find sources, improve the view or guide before adding infrastructure. If an actual encounter needs another runtime, implement the smallest permission-scoped adapter. If participants propose another activity, that is a reason to revisit this recommendation, not a deviation to penalize.

Land implementation through short-lived pull requests with the existing lint, type, failure-path, coverage and exact-source CI requirements. This review itself changes documentation only. It does not republish immutable native assets, change the deployed website, enable models or open posting. Proposed welfare-sensitive intervention studies keep their separate protocol and review requirements.
