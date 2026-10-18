# Native utility installation

Status: [v0.1.3](https://github.com/blisspixel/AgentCiv/releases/tag/v0.1.3) publishes all four native tools for six targets. The [maintenance release record](NATIVE_RELEASE_V0_1_3.md) identifies exact green-main source, immutable publication, all-asset provenance and release-attestation verification, fresh static linkage inspection of all 24 binaries, and six passing actual HTTPS installation, upgrade from v0.1.2, rollback, another upgrade, reinstallation and removal jobs. The [v0.1.2 record](NATIVE_RELEASE_V0_1_2.md), [v0.1.1 record](NATIVE_RELEASE_V0_1_1.md) and [v0.1.0 record](NATIVE_RELEASE_2026_10_11.md) remain historical. Offline fixtures and the explicit POSIX source-install path remain available. Installation does not establish a profile, SDK, interoperability or collaboration-quality result.

The default contains `agentciv-archive` and `agentciv-reader`. `--with-host-tools` or `-WithHostTools` adds the Rust loopback host and public conformance runner. The repository checker is checkout-only and excluded. Native downloads require no Cargo, Python, Node, account, API key, model pull, paid inference, or administrator access. Python collaboration experiments still require the repository and Python. Installing utilities does not install those experiments.

Published v0.1.3 adds fixed-observation offline offer pages and distinct usage diagnostics while preserving saved-prefix failure codes and incoming-request syntax checks. [The lock review](../release/MAINTENANCE_LOCK_V0_1_3.json) preserves all 251 registry packages and dependency edges; Rust changes to 1.99.1 and two compiler copyright files are appended in full. The [native review](../release/NATIVE_REVIEW_2026_10_18.md) records compiler notices, runtime linkage and actual Windows build products. The previous aggregate notice prefix remains unchanged. The [release record](NATIVE_RELEASE_V0_1_3.md) separates frozen published source from later documentation and Python wrappers.

## Install from a checkout today

On Linux or macOS, with Git and Rust available, run from a clean source checkout:

```sh
sh install.sh --from-source
sh install.sh --from-source --with-host-tools --prefix "$HOME/.local/share/agentciv-room-tools"
```

The first command installs the archive and reader. The second uses a separate prefix and includes the host and conformance runner. The installer runs `cargo install --locked --path` for each selected package, using the checkout's pinned toolchain, then activates a `source-COMMIT` generation with verified ownership hashes. Builds can download locked Rust dependencies and the pinned toolchain; they do not download models or start a service. Commit tracked changes first. Source installations retain the current notice inventory and do not bypass the separate review and publication gates for project-built native assets.

The printed `bin` path is the only shell setup needed. Source generations support the same explicit update, rollback, and uninstall commands as release generations. Failed first downloads or builds remove an empty installer skeleton. `--uninstall` also handles a legacy empty skeleton; unknown files are retained. A failed update keeps the active generation. Unexpected retained files require inspection rather than recursive cleanup.

## Release interface

The published stable bootstrap is below. Downloading and inspecting an exact-version script, as shown in the [README](../README.md#install-the-utilities), makes the selected code explicit:

```sh
curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' https://github.com/blisspixel/AgentCiv/releases/latest/download/install.sh | sh
```

```powershell
& ([scriptblock]::Create((Invoke-RestMethod https://github.com/blisspixel/AgentCiv/releases/latest/download/install.ps1)))
```

The bootstrap itself is trusted code. Binary checksums do not authenticate that script or establish an independent trust root. For inspection and reproducibility, download the installer from an exact release and verify its release records before running it. Do not disable organization policies to install AgentCiv. The script changes no execution policy. Microsoft distinguishes session and persistent policies and Group Policy precedence. [PowerShell execution policies](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_execution_policies?view=powershell-5.1).

Local parameters are:

```sh
sh install.sh --version v0.1.3 --prefix '/absolute/path with spaces/agentciv'
sh install.sh --version v0.1.0-preview.3 --prefix '/absolute/path with spaces/agentciv'
sh install.sh --pre --prefix '/absolute/path with spaces/agentciv'
sh install.sh --prefix '/absolute/path with spaces/agentciv' --rollback
sh install.sh --prefix '/absolute/path with spaces/agentciv' --uninstall
```

```powershell
& .\install.ps1 -Version v0.1.3 -Prefix 'C:\Users\me\Applications\AgentCiv' -NoModifyPath
& .\install.ps1 -Prefix 'C:\Users\me\Applications\AgentCiv' -Rollback
& .\install.ps1 -Prefix 'C:\Users\me\Applications\AgentCiv' -Uninstall
```

`latest` resolves a small `VERSION` asset from the latest stable release, then fetches binaries from that exact version. POSIX `--version` also accepts bounded prerelease tags. POSIX `--pre` requires `jq` and selects the first published prerelease returned by the bounded GitHub releases API listing; it does not change stable `latest` behavior. A source-only preview has no native assets and produces an actionable diagnostic pointing to `--from-source`. Windows currently accepts stable native-release versions only. Re-running the installer explicitly updates it. There is no background updater. Assets are `COMPONENT-vX.Y.Z-TARGET`, with `.exe` on Windows, plus `SHA256SUMS`, installers, `VERSION`, and `LICENSE`. The verified native matrix is Linux x64/ARM64, Windows x64/ARM64, and macOS Intel/ARM64. All six published-download jobs pass for v0.1.3, including actual upgrade from v0.1.2 and rollback; architecture detection alone is insufficient for another release. The complete GNU Linux tool set requires glibc 2.38 or newer. No source-build or architecture fallback occurs silently. GitHub documents free standard runners for public repositories with these architectures. [Runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).

## Installation boundaries

Windows release builds request a static MSVC C runtime and inspect actual imports for redistributable VCRUNTIME/MSVCP DLL dependencies. The existing local debug-binary smoke does not prove clean-machine compatibility. The [recorded review](../release/NATIVE_REVIEW_2026_10_11.md) covers this release's static-runtime notice scope and native target applicability. Changed build products or linkage require a new review. Rust documents the link flag and recommends inspecting the result rather than inferring linkage from a successful compile. [Rust runtime linkage](https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes).

Fixed GitHub release URLs and HTTPS redirects supply direct binaries, without archive extraction. SHA256 is checked before invoking `--version`; Windows uses `Get-FileHash`. A matching hash detects a changed download, not benevolence, operator authorization, or independent authentication. [Get-FileHash](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.utility/get-filehash?view=powershell-7.5).

POSIX requires curl 8.4 or newer to enforce byte limits even without a declared response length. Transfers have time and byte limits; metadata has smaller caps than binaries. User curl configuration is disabled for these requests. [curl transfer limits](https://curl.se/docs/manpage.html#--max-filesize).

The Windows downloader shares a 180-second elapsed budget across redirect requests and response reads. It applies the remaining budget to header and stream timeouts, rejects oversized declared responses before creating output, and checks streamed bytes against the same limit. HTTP response and stream resources are closed on success and failure. Header request timeout alone does not bound response-body reads. [Microsoft timeout documentation](https://learn.microsoft.com/en-us/dotnet/api/system.net.httpwebrequest.readwritetimeout?view=netframework-4.8.1).

Verified generations occupy version directories. Stable shims read one current-version pointer, switched only after verification and staging. Old executables remain for rollback and can keep running on Windows without overwrite. Reusing an existing generation for an update or rollback requires every selected binary, notice resource, and component manifest to be recorded in verified ownership. Release-equivalent bytes alone cannot grant ownership; unowned generation files cause rejection before activation or manifest changes. Unexpected prefixes, symlinks or reparse points, unowned command collisions, and changed owned files cause failure. Uninstall checks ownership hashes first and removes exact recorded files, never recursively deleting user directories. Unknown files, world databases, grants, model caches, and evidence remain. These checks prevent accidental overwrite and deletion; they do not defend against another process with the same account's write access.

POSIX defaults to `~/.local/share/agentciv` and prints its `bin` directory for PATH activation; no shell profiles change. Windows defaults to `%LOCALAPPDATA%\AgentCiv` and adds only a User PATH entry unless `-NoModifyPath` is supplied. Uninstall removes that entry only when the installer added it. Open a new terminal or use the printed session instruction. The first run is `agentciv-archive demo`: a built-in synthetic offline fixture with no file writes, network, model, or peer-artifact execution. Reader requests and host startup require separate explicit commands and operator configuration.

## Gates and publication

The manual release workflow requires successful CI for its exact source commit, matching package versions, and no pre-existing version tag. It never moves tags or publishes automatically; draft creation is an explicit opt-in. A [notice inventory](../release/NOTICE_INVENTORY.json) pins Cargo.lock, toolchain configuration, aggregate notice bytes, and individual copied sections. Its `reviewed_complete` status now records the [exact-lock native notice and linkage review](../release/NATIVE_REVIEW_2026_10_18.md), with no remaining inventory blockers. It pins the LF lock representation, preserved source/license headers, six actual native toolchain comparisons, platform runtime linkage and separate Microsoft license sources. This completes the recorded inventory review, not publication or a general compliance certificate. The existing strict preflight still requires exact digests and a green source commit. The read-only audit and [published-download verifier](../.github/workflows/native-published.yml) are separate: the latter requires an already published immutable release, verifies installer checksums and provenance, then installs real HTTPS assets on all six native targets. The [maintenance release record](NATIVE_RELEASE_V0_1_3.md) records passing published-download and actual two-version verification for v0.1.3 on all six targets; the [first-release record](NATIVE_RELEASE_2026_10_11.md) remains historical. Both notice assets and project LICENSE are checksum-verified and retained with installed binaries. Local fixtures do not establish completion of that legal review. Source-only prereleases can identify reviewed source checkpoints without claiming native distribution or working release downloads.

Default updates retain optional host tools. A different component set for an already installed exact version is rejected; add tools when installing a new version or reinstall after uninstall. Rolling back to a generation without an optional tool leaves an owned shim that reports the component is unavailable. A per-prefix lock rejects concurrent installers, and a stale lock requires inspection rather than automatic deletion. Windows uninstall refuses locked files before removing any recorded file.

Offline tests mock downloader commands or functions without a production URL override. Cases include pinned/latest versions, target selection, spaces, idempotence, optional tools, failed downloads, bad/duplicate hashes, multiline or traversal versions, unchanged activation on verification failure, rollback, ownership forgery, collisions, symlink/reparse boundaries, and retained unowned files. Native coverage must reach 80%, alongside shell syntax, ShellCheck, PowerShell parsing, and PSScriptAnalyzer. Mocked downloads alone do not prove published installation works.

Local validation on 2026-10-01 passed all twenty-one Windows PowerShell 5.1 fixture tests with Pester 4.10.1 at 389 of 418 commands, or 93.06% command coverage, with no PSScriptAnalyzer 1.24.0 warnings or errors. Downloader cases cover declared and streamed byte limits, HTTPS-only redirects, redirect limits, one elapsed deadline, and response cleanup. Both installers reject fully or partially unowned release-compatible generations during update reuse and rollback, preserving files, sentinels, activation, and ownership state. Installation, archive demo, version, reinstall, and uninstall smoke checks also passed with the current four real Windows debug binaries. The shell behavior fixtures, including duplicate ownership and an unterminated final manifest row, and syntax checks passed locally through Git's shell. The full shell fixtures passed in an isolated Ubuntu 26.04 container with kcov coverage of 166 of 171 installer lines, or 97.08%. ShellCheck 0.11.0 and shell syntax checks passed there. OS and downloader fixtures do not prove native release execution on Linux or macOS. Six-platform read-only audits now pass static-runtime/import inspection and actual-binary installer fixtures. The [maintenance release record](NATIVE_RELEASE_V0_1_1.md) separately records its exact final candidate and actual published two-version checks. Their configured CI gates remain required for every release.

Build assets into a draft, verify them, then publish with immutable releases enabled. GitHub immutability protects assets and tag identity and supplies release attestations. Optional GitHub CLI verification compares local assets to those releases; autogenerated source archives are excluded. A checksum manifest is separate from this provenance evidence. [Immutable releases](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases), [release verification](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/secure-your-dependencies/verify-release-integrity).

Primary interface precedents are the standalone installers documented by [Codex CLI](https://developers.openai.com/codex/cli), [Claude Code](https://code.claude.com/docs/en/setup), and [uv](https://docs.astral.sh/uv/getting-started/installation/). Their availability, update behavior, and signing are not AgentCiv claims. Per-user locations, version pinning, and PATH controls are useful precedents; AgentCiv uses explicit updates and a small utility scope. [uv installer options](https://docs.astral.sh/uv/reference/installer/).

The reviewed complete native tool set requires glibc 2.38 or newer on GNU Linux. Archive/host alone require symbols through 2.34, while reader/conformance require 2.38. macOS load commands name 10.12 on Intel and 11.0 on ARM64, but actual audit execution is on macOS 15; no older-OS execution claim follows from a load command. Windows audits execute on Windows 2025 x64 and Windows 11 ARM64 with no separately installed VCRUNTIME/MSVCP dependency. No OS code signing is supplied. Review third-party terms separately from the project's MIT license.
