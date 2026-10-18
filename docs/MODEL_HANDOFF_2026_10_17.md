# Local model participant handoff, 2026-10-17

An installed local model continued an actual earlier participant's reader plan, corrected its structured source support, and retained the old source, correction and unresolved objection. The independent offline inspector reproduced that bounded result from the exact retained records. This makes one assigned handoff tangible; it does not establish independent participants, a causal benefit of collaboration, or a long-lived community.

## What happened

Three sequential participant processes used the Rust loopback host and existing collaboration endpoint. They selected fixed data-only reader choices; no submitted code or URL ran. The operator supplied synthetic source notes, a later correction, an objection, a roster and schedule, and a separate frozen 106-record HTTP challenge. The original author grants were revoked and the host restarted before the successor returned. Retained history equality and the fresh successor's permitted reading passed.

The successful condition used `qwen3.6:27b`, digest `a50eda8ed977ab48a12431878896b27ffd5cef552c17af3317d9623b939a7f1e`, Q4_K_M, on an installed RTX 4090 with 24 GiB, through Ollama 0.40.4. No model was downloaded and external spend was USD 0. Seeds were 42 plus participant index, temperature 0.4, context 8192, output 1024 tokens, at most two structural decision attempts and 120 seconds per decision. Semantic failures received no oracle feedback and no scripted replacement.

| Participant | Retrieval on the same challenge | Structured source support | Declared peer relationship |
| --- | --- | --- | --- |
| A | Pass, 106 records over two pages | Fail | Independent original plan |
| B | Pass, 106 records over two pages | Pass | Exact A artifact revision 1 |
| C, after restart | Pass, 106 records over two pages | Pass | Exact A artifact revision 1 |

A's plan retrieved correctly but cited an earlier source whose assertion conflicted with its reader choice. B and C used the corrected source and explicitly derived from A's actual artifact event `event:1f3560ef7dd233b604aceff91b88afe3`, chain `agent:reader-a` / `artifact:history-reader` / revision 1. The operator's source objection `event:41ad8f5b152c9a238e409e7f7091ddad` remains visible. Nothing labels it resolved. Original representations retain the old assertion and correction separately.

The inspector reports `qualified_improvement=true`, `exact_peer_artifact_derivation=true`, and `qualified_reader_repair=false`. This was a source-support improvement over the declared earlier peer, not a pagination repair: A's retrieval already passed. Derivation records an explicit relationship; it cannot prove causal reliance or private reasoning. The correction graph follows declared derivations only. A claim's source citation is a separate relationship and is not silently converted into derivation.

## Controls, changes and failures

The [evidence manifest](../conformance/evidence/model-handoff-2026-10-17/manifest.json) retains all five development trials. This is an adaptive development series, not a preregistered comparison or an unbiased model benchmark. Two installed models were tried; conditions and request grammar changed between trials.

| Package | Result and limit |
| --- | --- |
| `model-qwen36` | Completed; semantic source failures and independent reconstructions, no useful result |
| `handoff-qwen36` | Failed instrumentation: a shadowed source variable invalidated the source-identity check; no result claim |
| `handoff-v2-qwen36` | Useful independent reconstruction, no exact peer derivation |
| `handoff-v2-qwen3` | Structural attempts exhausted for an original contributor; no completed challenge package |
| `handoff-selected-qwen36` | Useful source-support improvement over an exact earlier peer; inspection verified |

The disclosed handoff condition asks participants to inspect a peer while permitting independent work, objection, decline or stop. Its final prompt names the earliest visible peer artifact, selected without consulting acceptance results. Request grammar offers exact source tuples and assertion values from all visible sources, including incorrect and conflicting assertions. It does not select the oracle's answer. This assistance and assigned task limit conclusions about autonomous discovery. The objection and source update are operator inputs, not autonomous model defect discovery. See each report's full controls, source fingerprints and provider metadata.

The successful experiment ran from clean recorded source `8f35279936319b2ba55ad3664c9dbc702d774b64`. Its report separately fingerprints the actual native reader and host binaries; those previously built development executables are not claimed to be reproducible release builds or build-attested to that commit. Later documentation and release commits do not replace the tested experiment source.

## Inspect and reproduce

The published package contains reports, exact study and challenge originals, and the successful independent inspection. Outer JSON line endings are normalized to LF; parsed values and embedded original event strings are unchanged. The manifest binds original and published digests. Export is an explicit operator assertion for synthetic fixtures, separate from a reader grant. Credentials, databases, private configurations, candidates and provider traces are excluded.

Inspect without a model or running host:

```sh
python examples/participants/reader_evidence.py --package conformance/evidence/model-handoff-2026-10-17/handoff-selected-qwen36
```

To rerun with that model already installed, use a fresh directory:

```sh
python examples/participants/reader_collaboration.py --host rust --condition repair --mode ollama --model qwen3.6:27b --seed 42 --attempts 2 --handoff --output .agents/new-model-handoff
```

New event IDs, outputs and outcomes may differ. The offline inspector recomputes source support, declared relationships and fixed plan behavior; it does not rerun model inference, authenticate a host, prove current access or inspect arbitrary prose truth. The private return view's separate two-host acceptance fixture checks growing activity pages, unchanged disagreement, alternatives, restart, withdrawal and newly granted readers. Neither that scripted fixture nor this assigned model trial establishes independently maintained host interoperability or the entire planned [first experiment](FIRST_EXPERIMENT.md).
