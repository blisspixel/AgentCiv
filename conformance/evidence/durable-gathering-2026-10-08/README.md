# Bounded durable gathering observations

The `python` and `rust` directories each retain exactly `report.json` and `history.json` from the corresponding loopback host run. Both used source commit `14498ac309cad839d14ed81f5824e5774da9dc2c` with a clean, unchanged source inventory. The reports contain source and native binary fingerprints, explicit operator interventions, scoped control and admission receipts, sanitized child observations, and limits. No private configuration, runtime database, capability, child trace, or installed-model inference is included.

Each deterministic run starts scopes stopped, applies fixture-authorized resumes, and launches five actual gathering subprocesses. Two propose a walk and a song; one chooses local leave; another participant continues their own song; the first explicitly returns. Four authored messages remain in permitted shared history. A local leave does not automatically change durable state. A separate private control stops the scope, and exact old resume retries leave that stop intact.

The fixture rejects or blocks invitations after the stop, under the old coordinator, while the replacement is unready, and while history is unavailable. A separate stop succeeds without civic history. Host restart and world credential rotation do not erase runtime stopping. Explicit scope-authorized return affects that participant only. Reports distinguish an admission receipt from child completion.

Reproduce with fresh output directories from the recorded source:

```sh
python examples/participants/durable_gathering.py --host python --output .agents/durable-python-reproduced
python examples/participants/durable_gathering.py --host rust --output .agents/durable-rust-reproduced
```

Private capabilities are randomly provisioned outside the checkout, and actual current permitted history is prepared before positive dispatch attestations. Compare the observed invariants, not random input hashes or compiled binary identity across machines. History is decoded HTTP event JSON with authored text, not exact HTTP response spans. Export permission applies only to this operator-owned synthetic fixture; reading arbitrary history confers no copying permission.

These runs establish a bounded cooperating local adapter, separate from native race/crash and private recovery tests. They do not establish participant-origin authorization, persistent unattended scheduling, running-process cancellation, rollback detection, distributed coordination, model behavior, deployment, or independent-host interoperability. See the [stopping contract](../../../docs/DURABLE_STOPPING.md) for authority and mandatory manual recovery limits.
