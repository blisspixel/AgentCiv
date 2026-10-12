# Native utility maintenance release v0.1.1

Published 2026-10-12T09:55:11Z: [AgentCiv v0.1.1](https://github.com/blisspixel/AgentCiv/releases/tag/v0.1.1). GitHub reports this stable release and all 34 assets immutable. The [evidence packet](../conformance/evidence/native-release-v0.1.1/README.md) records actual publication, source, verification and two-version installation observations. The [v0.1.0 report](NATIVE_RELEASE_2026_10_11.md) remains a historical first-release result. Neither tag was moved to include later documentation or Python changes.

The native reader corrects an internal offline projection traversal bound. Immutable v0.1.0 inherited a 20-page default and failed above 2000 events despite advertising a larger ceiling. Regression cases now accept 2048 and 4096 originals, reject 4097 and retain the 16000-byte original limit. This changes reconstruction inside the utility, not the message profile, archive contract or manual wrapper budgets. The [private return guide](PRIVATE_RETURN_VIEW.md) explains complete same-caller observations, exact source inspection and uncertainty.

## Exact source and verification

| Stage | Retained result |
| --- | --- |
| Released source | `1f0ff2a59eb9bd8fa1eaab65b2f5dac669aed936`, the main result of [PR 85](https://github.com/blisspixel/AgentCiv/pull/85) |
| Source main CI | [38209480244](https://github.com/blisspixel/AgentCiv/actions/runs/38209480244), all 15 jobs successful |
| Native candidate | [38213447305](https://github.com/blisspixel/AgentCiv/actions/runs/38213447305), all nine jobs successful |
| Actual public downloads | [38233053571](https://github.com/blisspixel/AgentCiv/actions/runs/38233053571), all six target jobs successful |

The candidate retained strict preflight, six native builds, assembly/provenance and draft creation. Publication was a separate operator action. All 34 subsequently downloaded assets matched the candidate, exact-source build provenance and immutable release attestations. The 33 other assets matched the original `SHA256SUMS`, whose SHA-256 is `801f740b2a486eb755d0f76ab9ffca035cbbdc61aff83922cff388a8e5236a1c`. The manifest does not checksum itself.

The tools are archive, reader, Rust loopback host and conformance runner on six targets. Ten other assets supply installers, project license, third-party notices/inventory, version/source identities, two provider license documents and the checksum manifest. Original identity/manifest files, successful verification stdout, release metadata, 24 compressed original static reports and derived summaries remain separately labeled in the packet inventory. Exact-source notice review and linkage checks preserve the reviewed runtime scope; neither checksums nor provenance mean OS signing, reproducible builds or general legal certification.

The [maintenance lock review](../release/MAINTENANCE_LOCK_V0_1_1.json) records exactly four local package changes from 0.1.0 to 0.1.1. All 251 registry packages, their sources, checksums and dependency edges, other locked packages and the pinned toolchain configuration remain unchanged. The current lock SHA-256 is `9809a731519af5db1b11958efb37eef0e9eaf8fa165ea29fd8ec1aede49ae5fa`. The [notice inventory](../release/NOTICE_INVENTORY.json) preserves all 1393 notice slices and the original provider-document scope. Aggregate notice bytes remain 20,262,410, SHA-256 `1bcf14c8995a3ef4b539a5966d1b53f5ebf2fa62828b87b45d9cd48310b51a91`. These are lock and preserved-byte comparisons; actual native build images and linkage have separate candidate observations above.

Read-only initial draft lookup by tag returned HTTP 404. Lookup by the actual draft ID `410275983` corrected that metadata path. The same candidate assets were used: there was no rebuild, moved tag or disabled verification gate. Failed attempts remain outside the public success packet; the correction does not turn that failed lookup into a passing observation.

Another local `gh release verify` attempt failed during Sigstore initialization. Final verification used fresh official TUF-verified roots, selecting an unchanged GitHub-only root record for release/asset checks and combined roots for build checks. Signature, issuer, GitHub release identity and exact source/main/workflow/candidate/hosted-runner requirements remained enforced. Original failures and root-selection metadata remain ignored. The failed attempt stays a failure; completed verification has its own successful outputs.

## Actual two-version execution

Every target installed all four v0.1.0 tools into a fresh private runner prefix, upgraded to v0.1.1, rolled back to v0.1.0, upgraded again, inspected the installed generation, then reinstalled v0.1.1 and removed owned files while preserving an unowned sentinel. All six stage records retain actual versions and installed binary/notice hashes. This is 36 generation checks, 144 tool-version observations and 252 installed-file hash checks. Help and the synthetic offline archive demo also ran. Windows used PowerShell 5.1 and `-NoModifyPath`.

The GNU Linux x64 and ARM64 jobs ran on Ubuntu 24.04; the complete tool set requires glibc 2.38 or newer. macOS Intel and ARM64 ran on macOS 15. Their reviewed load-command minima do not establish execution on older macOS. Windows ran on Windows 2025 x64 and Windows 11 ARM64 with the reviewed static CRT and no dynamic VCRUNTIME/MSVCP dependency. Actual candidate Windows image identifiers were compared with the reviewed images. Physical Omarchy, other libc implementations, older systems and arbitrary environments remain outside these observations.

The latest stable API and actual `latest/download/VERSION`, `install.sh` and `install.ps1` hashes matched this exact release. These are retained observations of that pointer, not a promise that latest will never change. The assembler and documentation helper check supplied evidence consistency; they do not repeat cryptographic verification or independently authenticate the origin of locally downloaded artifact folders.

## Maintenance scope and limits

The successful exact-source main job measured Python 91.47% coverage (6995 statements, 597 missed), Rust 88.21% (11481 lines, 1354 missed), and 276 participant tests. The retained coverage summary identifies its source, job and original log hash. Those measurements belong to released source `1f0ff2a59eb9bd8fa1eaab65b2f5dac669aed936`. A later Python-host fix rejects explicit withdrawal `body: null`; that checkout change is not part of these native assets or their source claim.

Native installation supplies no Python room wrapper or participant experiments. Those still require a checkout and Python. The authored-activity guide has separately bounded synthetic evidence; this release adds no model trial, deployed service, public posting, completed HTTP acceptance proof, profile completion, independent interoperability or evidence of participant understanding. Broader activity fixtures, uncertain-write reconciliation and guide-only model arrival retain their own roadmap gates. Future releases must repeat exact-green-source, notice, platform, immutable publication and actual public installation checks.
