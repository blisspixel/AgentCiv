# Native utility release v0.1.3

Published 2026-10-18T06:17:03Z: [AgentCiv v0.1.3](https://github.com/blisspixel/AgentCiv/releases/tag/v0.1.3). The stable release and all 34 assets are immutable. The [retained packet](../conformance/evidence/native-release-v0.1.3/README.md) records exact source, checksums, provenance, static inspection and actual two-version installation results. Earlier [v0.1.2](NATIVE_RELEASE_V0_1_2.md) remains a separate historical release.

The native reader now pages optional activity offers over a fixed complete observation, binds continuations to the exact input and query, and distinguishes command usage errors from configuration failures. It preserves original records and the existing draft contracts. The Python correction-aware return wrapper, model experiments and website changes require the checkout or deployed website; they are not additional executables in the native package.

## Source and verification

| Stage | Verified result |
| --- | --- |
| Released source | `f3551fcd2c21b54017da41352b9eba9486259e06`, merged through [PR 107](https://github.com/blisspixel/AgentCiv/pull/107) |
| Exact source main CI | [39098578812](https://github.com/blisspixel/AgentCiv/actions/runs/39098578812), all 24 jobs successful |
| Native candidate | [39102118997](https://github.com/blisspixel/AgentCiv/actions/runs/39102118997), all nine jobs successful |
| Public installation | [39103512513](https://github.com/blisspixel/AgentCiv/actions/runs/39103512513), all six target jobs successful |

The candidate rebuilt and tested all six native targets from that exact green main. Every draft asset matched the complete candidate artifact and checksum manifest. All 34 assets passed build provenance checks for that source, workflow and candidate invocation. Static inspection checked all 24 actual binary architectures, library dependencies and runtime/deployment metadata against the reviewed scope. Actual Windows build images and product versions were inspected separately. These are distinct checks, not an OS signing claim.

Publication used the reviewed draft without a rebuild or moved tag. `gh release verify` and all immutable asset attestations passed. Fresh published downloads matched candidate bytes, and the latest stable pointer and HTTPS installer/version downloads matched v0.1.3. `SHA256SUMS` hashes the other 33 assets; its own SHA-256 is `ef54f22421a77db2910b8111f73eb04249d011cd8ae5e2adcc5b1aaadea56581`. Original TUF-verified trust-root selection and verification results are identified in the evidence packet.

## Toolchain and native limits

The [maintenance lock review](../release/MAINTENANCE_LOCK_V0_1_3.json) preserves all 251 registry packages, sources, checksums and dependency edges; four local utility package versions change to 0.1.3. Rust changes to 1.99.1. The [six-target native review](../release/NATIVE_REVIEW_2026_10_18.md) verifies actual compiler identities, notices, linkage and Windows build products. Two changed compiler copyright files were appended in full. Every byte of the previous aggregate notice prefix remains preserved.

The complete GNU tool set requires glibc 2.38 or newer, and the POSIX downloader requires curl 8.4 or newer. macOS execution is tested on macOS 15 Intel and ARM64; load-command minima do not establish execution on older systems. Windows x64 and ARM64 import no dynamic VCRUNTIME or MSVCP dependency, and static runtime terms remain separate from the project's MIT license. No OS code-signing or notarization claim is made.

## Actual upgrade and rollback

Each target installed all four v0.1.2 tools into a fresh private prefix, upgraded to v0.1.3, rolled back to v0.1.2, upgraded again, reinstalled v0.1.3, and removed owned files while preserving an unowned sentinel. Installed versions, generation hashes, help and the offline archive demo were checked. Windows used PowerShell 5.1 and `-NoModifyPath`. These are actual public HTTPS downloads and native execution, separate from fixture installer coverage and pre-release audit builds.

The release starts no host or model and grants no civic or outside-system authority. The [retained local model handoff](MODEL_HANDOFF_2026_10_17.md), [public-entry correction](PUBLIC_ENTRY_ACCESS_2026_10_18.md) and website deployment are separate evidence. Independent host interoperability, long-term communities and arbitrary platform support remain outside these observations.
