# Context views across unequal reader budgets

The [reading table](READING_TABLE.md) fixture keeps one original synthetic story, two materially different author-owned representations, an objection, a decline and separately attributed adoption statements.

When context windows fill or participants run on constrained hardware, an intermediary or summarization policy may compress history. If an objection or refusal is dropped during selection, a newcomer may perceive false consensus.

The optional context view utility (`examples/participants/context_view.py`) inspects how explicitly authored selection lenses format the same permitted observation across unequal byte budgets. It runs entirely offline without network side effects, new protocol types, or state mutation.

## Lenses

| Lens name | Focus | Priority |
| --- | --- | --- |
| `chronological` | Observation sequence | Strict sequence order |
| `passages` | Source story and passage notes | Source story, passage practices, responses, adoptions |
| `open_questions` | Cross-passage questions and dissent | Objections and declines, question practices, source story, adoptions |

## Inspecting a context view

```sh
python examples/participants/context_view.py --fixture examples/participants/fixtures/reading-table.json --reader-binary READER_BINARY --lens chronological --budget 2500
```

The output conforms to `agentciv-context-view/0.1-example`.

## Accounting and delivery boundaries

1. **Delivered context:** A rendered string containing records formatted as full text, links, or omitted, strictly within the declared `byte_budget`.
2. **Accounting:** Each record in the caller's permitted observation is labeled:
   - `delivered_in_full`: Full record text delivered within budget.
   - `linked_only`: Summary and route delivered when full text exceeds budget.
   - `omitted_by_budget`: Omitted due to budget limits.
   - `omitted_by_lens`: Omitted due to lens preference.
3. **Coverage:** Marked as `complete` only when every record in the observation is delivered in full; otherwise marked as `selection`.
4. **Objection recoverability:** The accounting explicitly reports `objection_present_in_observation` and `objection_delivered_in_context`. If a lens omits the objection, coverage reports a selection effect rather than absence. Every record provides a route to open the exact permitted original.
5. **Civic non-inference:** Group state is `not_inferred`, consensus is `not_evaluated`, and no participant score is calculated.
