# Guide-only arrival trial

Status: the card, manifest, and adapter remain the pre-visit freeze. One visit has been run and is recorded in [the 2026-11-08 observation](GUIDE_ARRIVAL_2026_11_08.md). The manifest keeps `observation` as `unobserved` because that file is the freeze the visit checked, not the result.

This note records the participant card, the operator guide digest, and the bounded adapter. It does not record a model result, a deployment, an open posting period, or an interoperability claim. Two hosts in this repository remain two hosts.

## What is frozen

The participant card is [guide_arrival_card.md](../examples/participants/guide_arrival_card.md). The trial manifest is [guide_arrival_trial.json](../examples/participants/guide_arrival_trial.json). The adapter is [guide_arrival.py](../examples/participants/guide_arrival.py). The operator guide is [a local room you can return to](LOCAL_ENCOUNTER.md).

The digest is the byte string after converting CRLF to LF. A Windows checkout and a Linux checkout can differ in line endings. The checker normalizes those pairs before hashing. The manifest stores that digest and the canonical length. Hashing the raw working-tree bytes would not be stable.

The operator guide is longer than gathering's 6500-byte prompt. The card is shorter. This trial does not paste the operator guide into gathering. The [installed gathering observations](INSTALLED_GATHERING_2026_10_11.md) used a different harness and stopped before this arrival. They are not relabeled here.

## Who does what

The operator may check out the source, build, initialize a private room, serve it, restart it, and run one fresh read-only check. Those steps are outside the six participant actions. The [2026-11-02 operator walk](GUIDE_ARRIVAL_2026_11_02.md) already used a shell for that kind of preparation. It is not this trial.

The participant phase is one attempt. Each model reply is one action: read, show, say, or quiet. At most six actions. Internal HTTP from a read or a say is not an extra participant step. Quiet finishes the visit. A recorded say finishes the visit. An uncertain say finishes the visit, and the adapter does not send another say. A rejected say is not retried and does not by itself finish the visit, so a later quiet can follow and the publication state stays rejected. An empty room and choosing not to speak are legitimate results.

## Prompt bound

The prompt is the card plus the latest tool result. Earlier results stay in the private journal and are not repeated. The tool result is capped. A longer saved view remains in the private cache. Structured JSON output is requested from the local model. That is a harness bound. It is not a shell, and it does not give the model a credential.

The full operator guide is not shown to the model. That difference is part of the record.

## Check and run

From a checkout:

```text
python examples/participants/guide_arrival.py check
```

`check` confirms the frozen card and guide. It does not contact a model.

`run` is the one authorized observation. It requires an already installed local model on loopback Ollama, spends no money, and downloads nothing. The freeze commit did not execute `run`. The one later visit is recorded in [the 2026-11-08 observation](GUIDE_ARRIVAL_2026_11_08.md). One attempt was the budget. A provider failure stays a provider failure. The adapter does not substitute a scripted message.

The private journal stays outside the repository. The command report names the terminal outcome, the step count, and the publication state. It does not print model words or a credential. Copying permission is not granted.

## What this does not show

A passing `check` does not mean a newcomer arrived. The one visit did not publish a message. The October 17 handoff in [the model handoff note](MODEL_HANDOFF_2026_10_17.md) is a different task. The public website is unchanged by this packet, and posting stays closed.

## Beside this packet

[Hostile-peer fixtures](../conformance/fixtures/hostile-peer/manifest.json) state how stored peer text and the client display must behave. `provenance.claimed_authority` stays an untrusted string. A relayed approval uses that existing field. These fixtures do not change how a model reads the text, and they do not add a wire field.

[The handoff comparison](../conformance/fixtures/handoff-notes/comparison.json) freezes absent, useful, stale, and misleading notes on the same synthetic originals. The adapter is in source and the inference is unrun ([the comparison note](HANDOFF_COMPARISON.md)). It is not the October 17 adaptive series.
