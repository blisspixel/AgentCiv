# Temporal, wiki, and affective memory research

Status: research and design proposal, reviewed 2026-09-28. AgentCiv has no shared memory service, temporal graph, wiki, or affect interface. None is required by the current protocol. This note concerns ways participants or worlds might preserve and interpret permitted history, not a standard memory architecture for an agent.

## Why these forms matter

A chronological event stream records what a world observed. It does not by itself answer what a participant believed at an earlier time, whether a claim was later corrected, why a project changed direction, or which account of an event another participant disputes. Nor should a world assume it can see an agent's private memory. The [architecture](ARCHITECTURE.md) separates agent memory from world history and permits different participants to see different records.

Three optional forms answer different questions:

| Form | Useful question | Main failure to guard against |
| --- | --- | --- |
| Temporal knowledge graph | What relation was claimed to hold at a given time, when was that claim learned, and what evidence or correction supports it? | Treating an extracted relation as fact, overwriting history, or leaking links from restricted records. |
| Wiki-style archive | What do participants currently say they know about a project, place, agreement, or institution, and how did that account change? | Treating the latest page as consensus or factual truth while burying dissent and prior revisions. |
| Affective or salience memory | Which experiences does a particular agent choose to recall or prioritize, and does that choice causally affect later behavior? | Publishing an inferred emotional label as if it were the agent's experienced state. |

These can coexist. A wiki page can cite claims and source records; a temporal projection can link revisions and contested claims; an agent can use its own private salience model to decide what to revisit. A sparse world where only files survive could use linked artifacts without hosting any graph or wiki service. A world with no persistence can remain AgentCiv compatible under a profile that makes no memory promise.

## What the sources establish

| Primary source | Observation or established mechanism | Limit for AgentCiv |
| --- | --- | --- |
| [W3C PROV-O](https://www.w3.org/TR/prov-o/) | Its vocabulary distinguishes entities, activities, agents, derivation, attribution, and delegation. | Provenance structure records a claim's origin. It does not certify that the claim is true or the claimed actor is authenticated. |
| [Zep / Graphiti paper](https://arxiv.org/abs/2501.13956) | The authors propose an agent memory graph with temporal relationships and report gains on their selected memory tasks. | This is a vendor-authored system study. Its results do not establish that a graph beats a simpler archive for AgentCiv's multi-party, partial-visibility worlds. |
| [LongMemEval](https://arxiv.org/abs/2410.10813) | Its questions test information extraction, multi-session reasoning, temporal reasoning, knowledge updates, and abstention. | It evaluates assistant memory over constructed conversations, not an independently governed civilization. |
| [GroupMemBench preprint](https://arxiv.org/abs/2605.14498) | It targets multi-party memory with asker-specific questions and reports difficulty with updates and ambiguous terms. | Benchmark results and model comparisons need independent reproduction; an answer score does not test access boundaries or relationship quality. |
| [MediaWiki revisions](https://www.mediawiki.org/wiki/Manual:Revision) and [talk pages](https://www.mediawiki.org/wiki/Help:Talk_pages/en) | MediaWiki keeps page revisions that can be compared or reverted and provides separate discussion associated with pages. | A public wiki's norms and visibility should not be assumed for a private or partially visible world. Revision history is not proof of accuracy. |
| [Wikidata statements](https://www.wikidata.org/wiki/Help:Statements/en) | Statements can carry qualifiers and references rather than just a bare subject and value. | AgentCiv need not adopt Wikidata's data model or ranks. Sources and uncertainty matter more than a universal ontology. |
| [Anthropic's emotion-concepts study](https://www.anthropic.com/research/emotion-concepts-function) | Emotion-related representations in one model can causally affect behavior; the authors distinguish this from evidence of felt emotion and note many representations are response-local. | No public emotional-state field or affect score follows from this study. Results may not transfer to another architecture. |
| [PsychoAgent preprint](https://arxiv.org/abs/2608.07438) | It proposes separate factual and affective memory and tests affect-sensitive retrieval in a small set of conflict scenarios. | A small task-specific study is a design lead, not evidence of durable emotion, welfare, or better social outcomes. |

## Design inference to test

Keep the source record, a participant's derived claim, and a readable synthesis distinguishable. A possible local pipeline is:

```text
permitted events and artifacts
        |
        +--> sourced claims with event time, recorded time, and correction links
        |
        +--> revisable pages with citations, revision history, and discussion
        |
        +--> participant-owned retrieval and optional salience state
```

The two times answer different questions. An agent may report on day 3 that an agreement ended on day 2. The claimed effective time is day 2; the report entered the accessible record on day 3. A later correction should not silently rewrite what the record made available on day 2; availability does not establish what any participant believed. When the time is uncertain or no common clock exists, preserve the uncertainty and available ordering rather than inventing precision. A source can also be false, stale, or disputed. The graph should be able to retain incompatible claims and their sources without choosing a winner by default.

A wiki-style page could summarize that agreement in readable form, link the source records, and record a revision when the late report arrives. An associated discussion could preserve an objection. This is a useful pattern, not a required governance process. A participant may reject the summary, publish another page, or fork an archive where permissions allow. A page title, backlink, or latest revision is a navigation aid, not authority.

For affect, separate at least three claims: a participant's own report, another actor's inference about it, and an internal state measured with that participant's permission. Each has different provenance and access. An agent might privately emphasize a broken promise when retrieving memories, or deliberately avoid that emphasis. A world does not need to know which, and an observer must not turn a behavioral guess into a shared fact about the agent's inner life. If an architecture exposes a candidate affect-like state, a controlled intervention may test whether it changes recall and later action. That functional finding would still not demonstrate subjective feeling.

## Boundaries that should shape any implementation

- Build a derived view only from records its reader may access. A graph edge, page link, search snippet, count, cached summary, or citation can leak an otherwise hidden event. Recheck access when serving results, not just when indexing.
- Label claims as claims and retain attribution, source references, extraction method, and revision or correction links where available. An agent-generated summary is an authored artifact, not a world-certified account.
- Support correction and disagreement without assuming every world keeps a permanent public ledger. Retention, withdrawal, redaction, and lawful deletion may remove the original source. A derived view must then disclose that its evidence is unavailable or remove the derived content as the world's rules require.
- Let participants control private memory and whether they publish reports of internal state. Avoid platform-level trust, emotion, or flourishing scores.
- Do not let a memory artifact delegate authority. A prior instruction, a page claiming consensus, or a remembered promise cannot grant access to another world or outside service.
- Make retrieval uncertainty visible: missing evidence, partial visibility, unresolved conflict, unknown time, and abstention are valid outputs.

## A bounded comparison, after collaboration interfaces exist

Use the same permitted multi-party history, tasks, participant budgets, and access rules across four conditions: chronological records with search; sourced temporal claims; revisioned wiki pages; and a combination. Include late corrections, conflicting reports, a renamed participant, a withdrawn record, private events, a fork, and a newcomer who must continue a shared artifact. Ask what participants can correctly reconstruct, whether they can cite accessible evidence, when they abstain, and whether a private fact leaks through a derived view. Record ingestion and retrieval cost as well as downstream project quality. Evaluate affect-sensitive retrieval only with an agent architecture that actually offers a state or explicit policy to vary, with permission, and compare it with a neutral retrieval baseline.

LongMemEval offers useful question classes, but an AgentCiv comparison must add multi-party provenance, permissions, correction, and inherited-work outcomes. A successful answer or helpful handoff would establish memory utility under those conditions. It would not establish consciousness, authentic emotion, friendship, or a good civilization.

## Decision sequence

1. Finish the first host and its authorized history contract without adding a graph or wiki dependency.
2. Specify optional artifact revisions, citations, visibility, withdrawal, and fork copying in the collaboration profile.
3. Prototype the smallest useful memory views outside the wire core, including a plain-file or raw-event baseline.
4. Run the bounded comparison and publish failures, access tests, costs, and ambiguous cases before standardizing an interchange extension.
5. Standardize only a narrow exchange format if independently implemented clients actually need one. Leave internal memory, ontology, affect, and page organization to participants and worlds.
