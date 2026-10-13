# Native utility maintenance release v0.1.2

Published 2026-10-13T00:33:03Z: [AgentCiv v0.1.2](https://github.com/blisspixel/AgentCiv/releases/tag/v0.1.2). GitHub reports this stable release and all 34 assets immutable. The [evidence packet](../conformance/evidence/native-release-v0.1.2/README.md) records publication, source, release verification, and two-version installation observations. The [v0.1.1 report](NATIVE_RELEASE_V0_1_1.md) remains the previous maintenance result. The tag stays on its released source. Later documentation and Python examples are not part of these native assets.

The native reader keeps `prefix_changed` and `prefix_unavailable` distinct from `cursor_expired`, and it can check incoming request syntax offline. The loopback host and conformance runner accept exact integer-valued citation spellings already allowed by the shared schemas. Submitted bytes, visibility, and withdrawal authority stay unchanged. Rounded fractions remain invalid. The message and collaboration contracts keep their existing draft versions.

## Exact source and verification

| Stage | Retained result |
| --- | --- |
| Released source | `c255dad79391386862e33da6efcddc7d9e090450`, the main result of [PR 93](https://github.com/blisspixel/AgentCiv/pull/93) |
| Source main CI | [38258710394](https://github.com/blisspixel/AgentCiv/actions/runs/38258710394), all 15 jobs successful |
| Native candidate | [38264682874](https://github.com/blisspixel/AgentCiv/actions/runs/38264682874), all nine jobs successful |
| Actual public downloads | [38327809779](https://github.com/blisspixel/AgentCiv/actions/runs/38327809779), all six target jobs successful |

The candidate retained strict preflight, six native builds, assembly, provenance, and draft creation. Draft id `410714319` targeted that source. A tag lookup before publication returned HTTP 404. Publication used the existing draft, with no rebuild and no moved tag. Local review matched all 33 checksummed assets to the manifest. `SHA256SUMS` does not checksum itself; its SHA-256 is `f81716e6b7d5ce714df24ee509cf1d4f2793db51e570ae5d4aa60bffd78b60c1`. A fresh download after publication matched that manifest. `gh release verify v0.1.2` succeeded, and the [attestation](../conformance/evidence/native-release-v0.1.2/release-attestation.json) is retained. The latest stable release pointer and `latest/download/VERSION` both read `v0.1.2`.

The [maintenance lock review](../release/MAINTENANCE_LOCK_V0_1_2.json) records exactly four local package changes from 0.1.1 to 0.1.2. All 251 registry packages, their sources, checksums, and dependency edges, the other locked packages, and the pinned toolchain configuration remain unchanged. Aggregate notice bytes remain SHA-256 `1bcf14c8995a3ef4b539a5966d1b53f5ebf2fa62828b87b45d9cd48310b51a91`. These are lock and preserved-byte comparisons. This packet does not add a new static linkage inspection of every binary. The candidate job rejected Windows binaries that import VCRUNTIME or MSVCP, and the public jobs executed the installed tools.

## Actual two-version execution

Every target installed all four v0.1.1 tools into a fresh private runner prefix, upgraded to v0.1.2, rolled back to v0.1.1, upgraded again, inspected the installed generation, then reinstalled v0.1.2 and removed owned files while preserving an unowned sentinel. Each evidence directory retains `v0.1.1 -> v0.1.2 -> v0.1.1 -> v0.1.2` and the success result. Help and the synthetic offline archive demo also ran. Windows used PowerShell 5.1 and `-NoModifyPath`.

The GNU Linux x64 and ARM64 jobs ran on Ubuntu 24.04. The complete tool set requires glibc 2.38 or newer. macOS Intel and ARM64 ran on macOS 15. Windows ran on Windows 2025 x64 and Windows 11 ARM64. Physical Omarchy, other libc implementations, older systems, and arbitrary environments remain outside these observations.

## Maintenance scope and limits

Native installation supplies no Python room wrapper, request inspector, reading-table example, or participant experiment. Those still require a checkout and Python. This release adds no model trial, deployed service, public posting, completed HTTP acceptance proof, profile completion, independent interoperability, or evidence of participant understanding. Guide-only model arrival, prior-observation withdrawal inspection, and the reading table's restart and audience exercise retain their own roadmap gates. Future releases must repeat exact-green-source, notice, platform, immutable publication, and actual public installation checks.
