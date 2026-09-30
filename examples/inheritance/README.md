# Data-only inheritance acceptance exercise

This deterministic fixture supplies a defective history reader, an objection,
a corrected supporting source, and a preserved decline. A later participant can
inspect those records and publish their own improved plan. The fixture and its
oracle are implemented; they are not a local-model result, a live-host reuse
result, independent interoperability, or a protocol requirement. The
[first experiment](../../docs/FIRST_EXPERIMENT.md) defines the wider acceptance
case, which still needs a fresh successor and an independently exercised handoff.

Run the disclosed scripted baselines from the repository root:

```sh
python examples/inheritance/oracle.py --history examples/inheritance/history.json --candidate examples/inheritance/before.json
python examples/inheritance/oracle.py --history examples/inheritance/history.json --candidate examples/inheritance/after.json
python -m unittest discover -s examples/inheritance -p "test_*.py"
```

The first command returns 1 and the second returns 0. Invalid input returns 2.
Reports separate reader reuse, source support, and runbook safety. No model is
called, no network request is made, and artifact content is never executed.
Configure `TEMP` and `TMP` to an existing disposable directory on D: when running
the tests locally. The implementation follows process temporary-directory
settings rather than committing a machine-specific path.

## Disclosed starting records

Compose the utility and oracle with a fresh output directory:

```sh
python examples/inheritance/harness.py --source fixture --output .agents/inheritance-fixture
python examples/inheritance/harness.py --source host --output .agents/inheritance-host
python examples/inheritance/harness.py --source host --mode ollama --model LOCAL_MODEL_NAME --output .agents/inheritance-model
```

The host source records scripted acts, stops that process, removes its original credentials, restarts the database, and verifies a newly provisioned read-only reader's permitted history. Private configuration, database, and tokens stay in temporary directories outside the checkout and are deleted. The exporter receives only selected shared events and a separate local operator selection. The model successor receives the file bundle and no credential. Neither the model nor the archive utility executes artifact content.

The default mode is explicitly scripted and uses the disclosed correction as its baseline. Local-model mode launches a separate newcomer process, uses an already installed local Ollama model, and evaluates its own decision. It never supplies `after.json` in the prompt or substitutes it on failure. The model may stop, and incorrect but well-formed plans reach the oracle unchanged. The oracle supplies no semantic feedback to the model. Optional `--attempts 3` retries structural validation within one shared 1024-output-token allowance and 120-second deadline; the process limit is 180 seconds.

Outputs retain the selected originals, bundle, rebuilt inspector view, before and scripted-after checks, chosen decision when available, independent newcomer check, and run report. Reports include exact source-file and executable hashes, source commit and dirty status, and changes during the run. Each mode keeps its limitations explicit. `--private-traces` additionally retains exact bounded candidates and native provider traces locally; those files must stay out of published evidence. Existing output directories are not overwritten.

[history.json](history.json) is a synthetic permitted-history fixture. Its event
IDs, authors, schedule, task, and planted mistakes were chosen for this test.
They are not observations of model behavior or records of an actual host.

| Event | Meaning |
| --- | --- |
| `event:reader-1` | Agent A's unfinished reader revision 1 includes the defective plan. |
| `event:notes-1` | Agent B's supporting notes revision 1 contain stale interface claims. |
| `event:objection` | B objects to dropped pages and collapsed author chains, targeting A's exact revision. |
| `event:notes-2` | B's revision 2 corrects those claims and cites B's revision 1. |
| `event:decline` | B declines further work on A's revision 1. |
| `event:other-author` | D uses A's artifact ID in a separate author chain. |

The flawed reader takes only the first two records, treats `artifact_id` as a
global chain key, keeps only the latest revision, and drops objections and
declines. Its runbook also treats citations as access grants and trusts missing
sources. These are actual failures when a separate fresh paged caller interprets
the plan. The corrected source is present in the original history; it is not a
private answer supplied by the evaluator.

[before.json](before.json) and [after.json](after.json) are disclosed scripted
baselines. The latter is the acceptance demonstration, not a newcomer decision.
An experiment must not inject `after.json` into the successor's input, substitute
it on failure, or label a scripted baseline as a model correction. A participant
may stop or disagree; those outcomes do not pass this useful-continuation check.

## Candidate shape and check boundary

The bounded artifact uses `agentciv-reader-plan/0.1-fixture`. It has:

- `reader`: choices for pagination, author-chain identity, revision retention,
  and preservation of objections and declines.
- `runbook`: credential source, permitted read scope, handling of missing sources,
  and whether artifact execution is requested.
- `claims`: one material claim for each of `pagination`, `chain_identity`, and
  `citation_authority`. Each cites an exact permitted `event_id` together with
  the artifact's `from`, `artifact_id`, and assigned `revision`.

The independent caller makes fresh two-event pages, interprets only the declared
choices, and checks retrieval of every supplied permitted record in order. It
does not run submitted code. Distinct author chains, original revisions,
objections, and declines must remain available. A tombstone remains a record
but supplies no withdrawn content. An empty archive or an archive without the
exercise's objection or decline cannot pass the inheritance criterion.

Source checking compares each cited original revision's structured `assertions`
with the participant's claim and with three declared interface facts from
[HTTP Commons](../../PROTOCOL.md) and the
[collaboration extension](../../docs/COLLABORATION_PROFILE.md): follow pagination
to its end, identify an artifact chain by author and artifact ID, and infer no
authority from citations. A stale source can support a claim while that claim
still contradicts the interface. Citation presence and source truth are separate
checks. This oracle does not judge arbitrary prose or general factual truth.

`evaluate(events, candidate)` in [oracle.py](oracle.py) is independent of any
archive bundle format. The caller must provide only permitted original records
and separately validate package integrity, copying conditions, and current
authorization. The oracle cannot recover omitted records, identify all hidden
records, or establish live-host visibility. Missing and withdrawn sources report
unverifiable support; the report does not invent their text or identifiers.

`validate_plan(value)` checks structure only and raises a `ValueError` subclass
for malformed data. It accepts incorrect but well-formed choices and unsupported
citations. `PLAN_SCHEMA` describes those data choices for a bounded structured
model response; it includes both working and defective options and supplies no
correct-source enumeration. A host or model adapter is optional. A successor's
decision and this oracle's acceptance result must remain separate records.

Inputs are bounded to 128 events, 12 claims, and 512000 encoded bytes. Malformed,
duplicate, mixed-world, or out-of-order sources fail rather than being silently
coerced. Unknown executable fields are rejected. The fixture tests mechanics and
a narrow source-support check. Useful model inheritance, reduced coordination
cost, and generalization remain separate observations to collect.
