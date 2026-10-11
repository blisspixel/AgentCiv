# Native utility release v0.1.0

Published 2026-10-11T18:52:27Z: [AgentCiv v0.1.0](https://github.com/blisspixel/AgentCiv/releases/tag/v0.1.0). GitHub reports the stable release and all 34 assets immutable. This is the first native utility release. The message, collaboration, prefix and website contracts retain their separately named draft or experimental versions. This utility version supplies no SDK, completed profile claim, independent interoperability result, successful installed-model arrival, or long-term community evidence.

## Exact source and checks

| Stage | Retained result |
| --- | --- |
| Reviewed source | `5e734c758d038b395995840da7e15280df38f86e`, merged through [PR 82](https://github.com/blisspixel/AgentCiv/pull/82) after all 15 exact-head checks passed |
| Resulting main CI | [38161518723](https://github.com/blisspixel/AgentCiv/actions/runs/38161518723), all 15 jobs passed |
| Native candidate | [38164466990](https://github.com/blisspixel/AgentCiv/actions/runs/38164466990), all nine jobs passed: strict preflight, six native builds and real offline installer exercises, assembly/provenance, and concrete draft creation |
| Actual public downloads | [38165643322](https://github.com/blisspixel/AgentCiv/actions/runs/38165643322), all six target jobs passed against the published immutable release |
| Tag readback | Fetched `v0.1.0` resolves to the exact reviewed source above; no existing tag moved |
| Fresh local public downloads | All 34 files downloaded again after publication; all 33 manifest entries, exact source copies, build provenance and immutable release asset attestations verified |
| Latest pointer | GitHub's latest stable API reports `v0.1.0`; actual `latest/download/VERSION`, `install.sh` and `install.ps1` bytes match the exact published version |

The [evidence directory](../conformance/evidence/native-release-2026-10-11/) preserves the [candidate run](../conformance/evidence/native-release-2026-10-11/candidate-run.json), [six published jobs](../conformance/evidence/native-release-2026-10-11/published-installation-run.json), [release metadata](../conformance/evidence/native-release-2026-10-11/release.json), [verified release attestation](../conformance/evidence/native-release-2026-10-11/release-attestation.json), [draft-asset verification](../conformance/evidence/native-release-2026-10-11/candidate-assets.json), [published-asset verification](../conformance/evidence/native-release-2026-10-11/published-assets.json), and [latest download checks](../conformance/evidence/native-release-2026-10-11/latest-downloads.json). Each target's original result, help, demo, public release metadata and installer provenance are retained under `published-installation/`. These are installer and utility observations, separate from the HTTP acceptance reports.

The 34 assets comprise four tools on six targets, two installers, LICENSE, THIRD_PARTY_NOTICES.txt, NOTICE_INVENTORY.json, VERSION, SOURCE_COMMIT, two original provider DOCX documents, and SHA256SUMS. The manifest covers the other 33 assets; its own SHA-256 is `dedc766210eae6bcce9bdf598b42b63c7c12389233626664a17c32b306a2ed5a`. Exact package versions are 0.1.0. Build provenance verification requires this repository's `release.yml`, this exact main source, `refs/heads/main`, and GitHub-hosted runners. Release attestations are checked separately. Neither mechanism is OS code signing or proof that a build is reproducible.

## Actual native execution scope

| Target | Execution and runtime boundary |
| --- | --- |
| GNU Linux x64 | Ubuntu 24.04 runner; full four-tool set requires glibc 2.38 or newer |
| GNU Linux ARM64 | Ubuntu 24.04 ARM runner; full four-tool set requires glibc 2.38 or newer |
| macOS Intel | macOS 15 Intel runner; load-command minimum 10.12, SDK 15.5; older OS execution not established |
| macOS ARM64 | macOS 15 ARM runner; load-command minimum 11.0, SDK 15.5; older OS execution not established |
| Windows x64 | Windows 2025 runner, static CRT, no dynamic VCRUNTIME/MSVCP dependency |
| Windows ARM64 | Windows 11 ARM runner, static CRT, no dynamic VCRUNTIME/MSVCP dependency |

All six published jobs verify release immutability and release attestation, installer checksums and exact-source build provenance before executing the real public HTTPS installer. Each installs all four tools into a fresh private runner prefix, checks exact versions and help, runs the offline archive demo, reinstalls without dropping optional tools, and removes owned files while preserving an unowned sentinel. Windows uses PowerShell 5.1 and `-NoModifyPath`. A first-version reinstall is not a real cross-version upgrade or rollback. Installer failure/update/rollback fixtures and source installation remain separate evidence. Physical Omarchy installation, older macOS execution, other Linux libc implementations, OS signing, and arbitrary environments are not established by this matrix.

All 24 downloaded binaries also have a [static inspection record](../conformance/evidence/native-release-2026-10-11/linkage/verification.json) using the pinned Rust LLVM inspector. Architectures, actual imports, Linux symbol floors and macOS load metadata agree with the reviewed distribution scope. Original inspector reports are losslessly retained as `.txt.gz`; decompression yields the exact bytes identified by `raw_report_sha256`. The [Windows build-image readback](../conformance/evidence/native-release-2026-10-11/build-images.json) matches the actual images whose provider products and terms were reviewed: `windows-2025-vs2026` version `20260925.250.1`, and `windows-11-vs2026-arm64` version `20261004.176.1`.

The final comparison caught an omission in the earlier derived linkage summary: seven Linux tool rows omitted their glibc loader. Original earlier binaries and dynamic reports already included `ld-linux-x86-64.so.2` or `ld-linux-aarch64.so.1`. The new inspection records each omission, checks the retained earlier binary against its original checksum manifest, and confirms identical actual library sets. [Earlier dynamic-report originals](../conformance/evidence/native-release-2026-10-11/earlier-linkage-originals/) and manifests are retained. The historical summary is preserved; the omission is not treated as a new dependency or silently erased.

Local verification initially encountered Sigstore verifier-initialization failures. Those failed attempts remain in ignored working evidence. Official TUF roots were fetched and verified, build checks used that verified read-only root file, and release-asset checks ran serially. Every final check passed with source, issuer, signer and hosted-runner requirements intact. No verification requirement was disabled.

## Notices and broader validation

The [notice and linkage review](../release/NATIVE_REVIEW_2026_10_11.md) pins the exact LF Cargo.lock SHA-256 `c04074fcac1e4943a7d64cb6f713721358fed0c65ad2a897308a1bc894ae49c8`, Rust 1.98.1, and reviewed native toolchain/runtime scope. All 251 registry archives matched locked checksums. The aggregate preserves 1393 hashed sections: 457 notice-file sections, 913 original source headers, 21 toolchain notices and two readable provider-license extracts. Its 20,262,410 original bytes have SHA-256 `1bcf14c8995a3ef4b539a5966d1b53f5ebf2fa62828b87b45d9cd48310b51a91`. Original provider documents are distributed separately from derived readable extracts. No existing notice bytes were changed for publication. This records the reviewed distribution scope, not a general legal certification or review of future changed dependencies.

The [three original green-main HTTP acceptance matrices](../conformance/evidence/native-release-2026-10-11/acceptance-summary.json) each preserve 48 scoped reports, 586 passing observations and 12 explicitly optional skips, covering both local hosts and three visibility conditions. Their original bytes and line endings remain unchanged. Bounded retention, shared retry IDs, credential and access transitions, same-ID winners after killed-process restart, and acknowledged/unacknowledged write observations remain narrower than every clock boundary, fault interleaving or power-loss guarantee. `profile_complete` remains false.

Local checks pass formatting, warnings-denied locked workspace Clippy, the repository checker and pinned mypy 2.3.1 over 47 files. Retained suites pass 263 participant, 37 Python host, 54 inheritance and 11 report-adapter tests. The final documentation-stage Rust coverage rerun is 88.21% (11,480 lines, 1,353 missed), compared with the earlier 88.22% run; combined Python coverage is 90.82% (6,860 statements, 630 missed). Both exceed the 80% requirement. The documentation PR and its resulting main CI are separate from the already verified released-code CI above.

The [final website deployment](WEBSITE_DEPLOYMENT_2026_10_08.md#final-2026-10-11-deployment) separately records source, Worker version, exact artifact checks and 18 successful HTTPS representations. Posting remains closed on Workers Free with future operating decisions explicit. The [installed-model failures](INSTALLED_GATHERING_2026_10_11.md) remain failures. The [next research plan](NEXT_RESEARCH_PLAN_2026_10_11.md) prioritizes precise feedback, guide-only arrival and inspectable continuation without turning utility publication into evidence of shared life.
