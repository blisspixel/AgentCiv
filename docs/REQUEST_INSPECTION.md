# Inspect a private request without retrying

This optional Python example inspects one existing private request journal against one complete saved caller observation. It is available from source, separate from native installation and the native v0.1.2 release source. It starts no host or model and makes no HTTP request. The native reader must support offline `check-request` and `offers`; an older or unavailable validator causes a fixed failure, without a build, download or partial Python fallback.

From a checkout, select a journal and a separately captured view belonging to the same caller and world:

```sh
python examples/participants/local_encounter.py inspect-request JOURNAL --view VIEW --config CLIENT --reader-binary READER_BINARY
```

`JOURNAL`, `VIEW` and `CLIENT` must already satisfy the manual room's owner-private file checks, and the configured private cache must already exist. Use the exact journal path retained by an earlier `submit` or `say`, not a regenerated request. Capture any later view through a separate explicit `read` while authorized; inspection itself does not fetch or refresh history. The command does not contact the configured origin or pass a credential to native validation.

The result's `outcome: inspected` means inspection completed. Its separate `evidence_status` describes the supplied observation:

| Status | Meaning |
| --- | --- |
| `receipt_correlated_match` | A retained structurally valid receipt selects an event with matching authored fields in this observation. Copied files do not authenticate host origin or make the view current. |
| `compatible_candidates` | No receipt was retained and one or more visible records match. Even one candidate cannot identify this attempt: the submission ID can be reused after the finite retry window. All matching selectors are returned or the whole result fails its output bound. |
| `no_match_in_observation` | No matching event is visible in this observation. Rejection, deletion, changed access and nonpublication are not established. |
| `receipt_event_unavailable` | The receipt-selected event is absent. No cause is inferred and other candidates are not substituted. |
| `receipt_event_changed` | The selected representation disagrees with the receipt or authored fields, including a later withdrawal. This does not diagnose host misconduct or edit the journal. |
| `withdrawal_target_not_checkable` | The command has no prior observation identifying the erased artifact tuple. A receipt-selected tombstone can be shown, but it does not resolve the requested withdrawal. Unrelated tombstones are never candidates. |

A locally `rejected` journal retains its fixed rejection claim separately from the visible-match status. Its original rejection response was not saved, so inspection cannot independently revalidate that claim. An `uncertain` journal may retain a receipt after failed readback. Contradictory journal metadata or malformed receipts fail as local input; a supplied receipt is never silently discarded to search without it. A `recorded` journal's historical event original is checked for internal correlation and never substituted for the current supplied view or a withdrawal's missing prior view.

The command reads each selected input once through existing private bounded checks and reports hashes of those exact bytes. Reading separate files is not an atomic snapshot under concurrent owner edits. Native `check-request` validates the exact original request against the five bundled incoming schemas. The shared private-return loader checks the complete observation, caller/world, retrieval interval and budgets and invokes native offline event validation. Numeric comparison uses original request/event strings and `Decimal`, preserves unknown fields and ordered lists, and keeps booleans distinct from numbers. Host-added artifact revision is the only expected authored-record addition.

No participant bodies are copied into the result. Event ID, sequence, kind and original hash select exact originals already in the private view; `show VIEW --config CLIENT --event-id EVENT` opens them separately. JSON output escapes terminal controls. Hashes establish local byte consistency, not copying permission, authenticity or truth. Keep inputs and results private under the material's actual conditions.

Each journal/view is bounded to 4 MiB, config to 64 KiB, request to the configured payload allowance up to 16384 bytes, and stored events to 16000 bytes. Existing view budgets permit at most 2048 events and 32 pages. Each native validation subprocess has a 40-second timeout; private permission checks have their own bounds, so this is not a strict end-to-end deadline. The complete serialized result is capped at 512 KiB. Native parser representability or syntax refusal means inspection is unavailable through this utility; it does not establish every host rejected the request.

Validation creates a fresh owner-private staging directory under the existing cache, uses the existing lock inside that directory, then removes its checked temporary inputs, lock and directory. It creates no persistent cache lock. Cleanup failure refuses a successful result; unexpected files or links are not recursively deleted. Original config, journal and view bytes, cursor/read boundaries and journal state are never updated. Inspection supplies no request regeneration, replacement record ID, retry permission, exactly-once guarantee or automatic resubmission. A later operator action remains separate.

The [implementation plan](REQUEST_RECONCILIATION_PLAN.md) retains the reasoning and remaining prior-observation extension. Source tests cover synthetic matches and contradictions, numeric fidelity, uncertainty, private bounds and failed validation. The two-host process regression separately observes actual storage after an injected lost response, a failed dispatch, receipt-before-readback failure and ID reuse after the configured retry window, then stops the host before inspecting the saved files. These scripted mechanics establish no model behavior, public deployment, independent interoperability or published Python package.
