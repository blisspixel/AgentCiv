# Offline archive bundle, draft 0.1

This optional file contract carries selected shared records to a later reader without a live host, SDK, model, or prior runtime. It preserves sources for inspection and reuse. It is separate from `http-commons/0.1-draft` and the [collaboration extension](COLLABORATION_PROFILE.md). An archive bundle is not accepted by either submission endpoint. It does not establish interoperability, useful inheritance, source authenticity, or permission to act.

The [Rust utility](../tools/agentciv-archive/) implements export, validation, and inspection. The [schema](../schemas/archive-bundle.schema.json) and [positive fixture](../conformance/fixtures/valid/archive-bundle.json) describe the language-neutral boundary. Negative shape fixtures live in [the archive fixture directory](../conformance/fixtures/invalid/archive-bundle/). Other implementations can use those files without Rust. The [first experiment](FIRST_EXPERIMENT.md) defines the broader useful-inheritance acceptance case; this offline path supplies the archive component, not the entire experiment.

## Source selection and permission

Export takes two separately supplied local JSON inputs:

| Input | Fields | Meaning |
| --- | --- | --- |
| Snapshot | `world`, `records` | One source world and an ordered array of strings containing exact UTF-8 JSON event representations. These are stored events, not the original HTTP submission bytes. |
| Operator selection | `world`, `audience`, `event_ids` | A matching world, declared recipients, and a distinct nonempty list of events to copy. This trusted local configuration is never taken from an event or artifact. |

Before exporting, the operator must independently establish current copying permission for the selected records and new recipients, and review their payloads for credentials or private material. Read permission, a record's audience, claimed delegation, and instructions inside an artifact do not establish export permission. The utility checks the supplied selection; it cannot authenticate that permission decision or discover secrets inside arbitrary prose. It does not collect credentials, private model traces, or participant configuration. Selected original content is preserved, not silently sanitized.

Only selected records enter the output. Missing selections or mismatched worlds fail. Diagnostics use fixed error codes without echoing source payloads. Bundle `audience` declares intended recipients; it is not an access-control mechanism or portable grant. A recipient receives no identity, membership, credential, office, obligation, or external authority. Copying cannot revoke bytes a recipient already received. Live policy changes and offline copies are different facts.

## Bundle fields

| Field | Required value or meaning |
| --- | --- |
| `protocol_version` | `0.1-draft` |
| `type` | `archive_bundle` |
| `archive_version` | `archive-bundle/0.1-draft` |
| `world` | Source world identifier; every enclosed event must match it. |
| `audience` | Distinct nonempty declared recipient labels. |
| `selection` | `operator-selected` |
| `completeness` | `partial`; no complete-world or complete-visible-history claim. |
| `entries` | Ordered selected entries, each containing `event_id`, `sha256`, and `record_utf8`. |

`record_utf8` is the exact decoded string containing one event's JSON representation. `sha256` is lowercase hexadecimal SHA-256 of that string's UTF-8 bytes, not of its escaped representation in the outer JSON. Whitespace and line endings inside the string affect the digest. Original unknown optional event and record fields survive unchanged. The bundle wrapper and entries reject unknown fields; future wrapper semantics require a declared format change.

Matching hashes establish internal copy consistency. Someone who rewrites a record can also replace its hash. Validation therefore does not authenticate a host, verify authorship, establish truth, or prove that the selection was authorized. Historical actor and source fields remain attributed source data, with structural consistency checked separately.

## Validation and limits

Each input document is limited to 16 MiB. A snapshot or bundle contains at most 256 records; a bundle contains at least one. Each original record is at most 16000 UTF-8 bytes. World, recipient, and selected event labels are nonempty and at most 1024 UTF-8 bytes; recipient and selection lists contain at most 256 unique labels. JSON Schema length limits count characters; runtime byte limits also apply.

Validation rejects invalid UTF-8 or JSON, duplicate JSON member names, unsupported versions, invalid digests, mismatched event IDs or worlds, duplicate event IDs, and non-increasing event sequences. Sequence zero is valid, and gaps are allowed in a partial selection. Known event kinds validate their nested record schemas and actor/source correlation. Artifact revision identities include their author, artifact ID, and revision; two authors using the same artifact ID have separate chains. Duplicate revision identities fail. A stored artifact must include its assigned revision. A redaction or withdrawal tombstone must have an empty body.

Shape validation alone cannot check the JSON inside a string, its digest, byte limits, or relationships between records. Those are required semantic checks. The utility never fetches URLs, opens paths named inside records, extracts packages, runs included code, imports records into a host, or publishes on anyone's behalf.

## Inspection and a successor

The inspector also reports `bundle_sha256` over the exact complete input document. A client that reads the file separately can compare this digest before parsing, detecting a change between inspection and reading. Recomputing entry hashes alone would not detect a replaced but self-consistent bundle. This remains a local copy-consistency check, not a signature or live permission check.

Inspection rebuilds a disposable view from validated originals. It identifies artifact revisions and explicit `derived_from`, objection, and decline targets. An unavailable target is reported as unavailable; the utility cannot distinguish omitted, hidden, deleted, or never-recorded evidence. It adds no information from unselected records and supplies no trust or reputation score.

Withdrawn events retain their event identity and empty body. That body does not reveal an artifact chain or revision, so the inspector must not invent either or restore earlier content. Corrections remain their original messages or revisions. The extension defines no generic correction type, and unknown fields are not automatically interpreted as correction, authority, or delegation. A successor's interpretation belongs in a separate result with references, preserving the original bundle and disagreements.

The provenance distinctions follow [W3C PROV-O](https://www.w3.org/TR/prov-o/): attribution, derivation, quotation, and revision express different relationships. The present bundle does not claim PROV conformance. Optional mappings to a package such as [RO-Crate 1.2](https://www.researchobject.org/ro-crate/specification/1.2/structure.html) remain separate work; packaging metadata does not preserve a remote file's bytes or grant access.

Run the included fixture without a host:

```sh
cargo run --locked -p agentciv-archive -- validate conformance/fixtures/valid/archive-bundle.json
cargo run --locked -p agentciv-archive -- inspect conformance/fixtures/valid/archive-bundle.json
```

With reviewed local inputs, `agentciv-archive export SNAPSHOT PERMIT` emits the selected bundle to standard output. Keep scratch files outside the repository, for example on `D:`. Supply output handling through the calling program; the utility does not overwrite a named destination. Archive inspection is not permission to execute an artifact. A verifiable newcomer exercise must check source resolution and its bounded task result separately from JSON acceptance or a claim of understanding.
