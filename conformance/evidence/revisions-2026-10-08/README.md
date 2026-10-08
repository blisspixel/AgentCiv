# Successive-revision public observations

`local-validation.json` retains the original matrix produced by the operator adapter on source commit `14498ac309cad839d14ed81f5824e5774da9dc2c`. Its source inventory was clean and unchanged throughout the run. Exact source hashes, tool versions, timestamp, host conditions, operator actions, summaries, and individual case states are in the report. Earlier retained matrices remain unchanged.

Both in-repository hosts ran under `members`, `addressed`, and `sender_only`. Each extended report contains 58 cases: 53 passed and five hidden-citation cases skipped under `members`; all 58 passed under each restricted policy. Each host's separate lifecycle composition passed five prepare cases, seven restart-verify cases, and four policy-change cases. The newly retained assertions exercise artifact revisions 1, 2, and 3, exact later retries, rejected-write preservation of permitted history and artifact revision numbering, and later revisions after restart.

Reproduce in a fresh output file from that source:

```sh
python examples/http-commons/validate.py --output .agents/revisions-reproduced.json
```

The adapter creates disposable private host configurations outside the checkout, runs the public HTTP runner, restarts the hosts, then explicitly changes visibility. It publishes case reports, not private checkpoints, bearer credentials, or raw HTTP traces. Recording a passing case is scoped evidence from that run; it is not independently authenticated proof of every response. Reproduction should compare conditions and case states, not random event IDs or timestamps.

This is sequential revision and graceful process-restart evidence. It does not establish concurrent same-chain revisions, retention boundaries, cross-endpoint retry scope, interrupted writes, power-loss recovery, public deployment, complete profile coverage, or independently maintained interoperability. Caller-visible event sequences may have gaps; no unused global sequence-counter claim is made. See the [checked inventory](../../../docs/CONFORMANCE_INVENTORY.md).
