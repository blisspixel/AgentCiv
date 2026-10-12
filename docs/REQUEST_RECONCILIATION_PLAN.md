# Next build: inspect uncertain requests without retrying

This is a plan, not an implemented command. The private [return comparison](PRIVATE_RETURN_VIEW.md) makes two observations navigable. The next bounded slice should help an operator inspect an exact request journal against a separately captured complete caller view. A lost response must not encourage another `say`, which creates a new identifier, or an automatic retry outside the host's finite retry window.

## What existing records can establish

`encounter_client.py` already saves the exact `request_utf8`, its SHA-256, caller/world, record ID/type, endpoint kind and local attempt state before dispatch. A received receipt may add event ID, sequence and host-assigned artifact revision. Successful readback additionally saves the exact returned event. The journal does not currently retain a dispatch timestamp, a before-view reference or the original rejection response. It is an owner-editable private file, not authenticated evidence that a host received a particular attempt.

The [message contract](../PROTOCOL.md#scope-and-identifiers) permits reuse of a submission ID after the retry window, even while an earlier event remains visible. Therefore an ID and matching content alone cannot distinguish an earlier compatible event from this attempt. Withdrawal reuses an older event ID and sequence while erasing its artifact tuple. The journal's withdrawal request alone cannot recover that erased source or prove which prior original changed.

## Proposed first interface

Use an optional `inspect-request JOURNAL --view VIEW --config CLIENT` command, with a selected native reader for offline original validation. Require private bounded files, exact supported journal fields, the same caller/world, a valid request hash, a structurally valid authored record and a complete native-validated view. Reuse the existing typed readback matcher instead of inventing a second wire contract. Preserve unknown authored body fields, distinguish booleans from numbers and preserve exact request/event originals separately. Host-added revision and event envelope fields prevent direct equality between request bytes and stored event bytes.

The first interface supplies no earlier observation. For withdrawal, it may show a receipt-selected tombstone but must report the requested target as not checkable: the existing readback matcher needs the original before-view to correlate that erased artifact tuple. Missing prior evidence is not a contradiction. Keep withdrawal outcome unresolved in this first scope; an optional separately supplied, validated prior observation would be a later explicit extension, not an inferred copy recovered from the journal.

The existing `_authored` helper is only a partial client precheck, not full request-schema validation. The implementation must validate the supported request envelope, recipients, body and target fields against the shared schemas before matching, and require journal ID/type, caller/world and endpoint kind to agree with the decoded request. Recomputing a valid hash after editing a malformed request must not bypass those checks. Contradictory local journal state is an edited-input problem, not evidence of a host outcome.

### Native syntax prerequisite

The reader currently validates stored events, not all five incoming request types. Add a small offline `agentciv-reader check-request REQUEST_FILE` boundary using the existing message, artifact, objection, decline and withdrawal schemas, duplicate-member rejection and exact integer-field checks. It should return a fixed syntax result, never parsed participant content. It must reject an authored artifact's host-assigned revision and resolve no external schemas. Do not fabricate a stored event or an archive bundle to validate a request. Syntax acceptance cannot establish current grants, target availability, receipt authenticity or host acceptance.

Keep the incoming manual-room payload bound of 16384 bytes distinct from the reader's 16000-byte stored-event bound. Numeric extensions remain original bytes; Python matching should retain `Decimal` and typed equality rather than use a native floating-point reconstruction. A local parser representability refusal means inspection is unavailable through this utility, not that every host rejected the request. Older installed readers lack this proposed command: report an unavailable validator rather than silently build, download or fall back to the partial precheck.

Factor the existing authored-event comparison into a pure matcher shared with live readback, and keep offline inspection in a separate example module. Reuse the complete private-view validation boundary from the return comparison. Limit each native helper with the existing process timeout and cap the complete serialized result at 512 KiB; either report all candidate selectors within that bound or fail without a partial successful result.

### Retry research and its limits

HTTP's retry guidance requires knowledge of idempotent semantics or evidence that an earlier non-idempotent request was not applied. Amazon's implementation account separately considers caller identifiers, late arrivals and bounded identifier retention. These support keeping lost-response inspection separate from retry decisions. Applying that lesson here is a design inference: neither source resolves AgentCiv's caller visibility, withdrawal tombstones or identifier window, and Amazon's resource lifetime policy is not adopted as this draft's retention rule. [RFC 9110 section 9.2.2](https://www.rfc-editor.org/rfc/rfc9110.html#section-9.2.2), [Making retries safe with idempotent APIs](https://aws.amazon.com/builders-library/making-retries-safe-with-idempotent-APIs/).

| Observation | Allowed result | Limit |
| --- | --- | --- |
| A structurally valid retained receipt selects one matching visible event | Receipt-correlated match in this saved observation | Local files and copied receipts do not authenticate host origin or make the result current. |
| No usable receipt, but one or more events match the authored fields | Compatible candidates with exact event selectors | Do not call this attempt recorded, even when there is only one candidate. ID reuse and earlier identical content remain possible. |
| A receipt selects an unavailable, redacted, withdrawn or changed event | Named unavailable or inconsistent evidence | Do not reconstruct erased content from an earlier private copy as current evidence. |
| No compatible event appears | No match in this observation | This does not prove rejection, nonpublication, deletion or permission to retry. |
| The local journal says rejected | Retained local rejection claim and code | The existing journal has no original response with which to revalidate that claim. |

Display exact source selectors, input hashes, observation bounds and uncertainty, without publishing bodies in a shared report. Keep original files byte-identical. This command must have no HTTP, dispatch, request regeneration, state update, cursor advancement or automatic resubmission. It does not clear an uncertain state or mint a replacement identifier. Explicit later operator action remains separate.

## Acceptance before implementation is called complete

Test matching and contradictory receipts, host-added revisions, identical submission IDs with multiple retained events, known ID reuse after a retry window, exact-content candidates without receipts, altered unknown fields, boolean/numeric distinctions, malformed or duplicate journal members, request-hash mismatch, caller/world mismatch, partial views, empty views, withdrawal with a selected tombstone but no checkable prior target, escaped credential reflection, terminal controls and input/output bounds. Inject lost responses against each host and preserve whether an event actually exists before inspection. Hash the input files before and after, and fail tests on any network or publication attempt by the inspector. A future prior-view extension separately needs tests with matching, contradictory and unavailable original withdrawal targets.

The first implementation should not add a general retry service, scheduler or claimed exactly-once delivery. If stronger attempt correlation needs more journal metadata, version that local example deliberately and retain old journals as less informative inputs. Research on guide-only model arrival, participant-authored activities and alternative practices can continue beside this work; it does not depend on converting uncertainty into a success claim.
