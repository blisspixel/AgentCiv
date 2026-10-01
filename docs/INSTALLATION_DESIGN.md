# Native utility installation

Status: the repository includes installers and offline fixture tests. Native release builds and published-download validation remain pending. The intended bootstrap is not live; the commands below describe the release interface after a verified release exists. Installation will not establish a profile, SDK, interoperability, or collaboration-quality result.

The default contains `agentciv-archive` and `agentciv-reader`. `--with-host-tools` or `-WithHostTools` adds the Rust loopback host and public conformance runner. The repository checker is checkout-only and excluded. Native downloads require no Cargo, Python, Node, account, API key, model pull, paid inference, or administrator access. Python collaboration experiments still require the repository and Python. Installing utilities does not install those experiments.

## Release interface

After a verified release exists, the intended bootstrap is:

```sh
curl --fail --silent --show-error --location --proto '=https' --proto-redir '=https' https://github.com/blisspixel/AgentCiv/releases/latest/download/install.sh | sh
```

```powershell
& ([scriptblock]::Create((Invoke-RestMethod https://github.com/blisspixel/AgentCiv/releases/latest/download/install.ps1)))
```

The bootstrap itself is trusted code. Binary checksums do not authenticate that script or establish an independent trust root. For inspection and reproducibility, download the installer from an exact release and verify its release records before running it. Do not disable organization policies to install AgentCiv. The script changes no execution policy. Microsoft distinguishes session and persistent policies and Group Policy precedence. [PowerShell execution policies](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_execution_policies?view=powershell-5.1).

Local parameters are:

```sh
sh install.sh --version v0.1.0 --prefix '/absolute/path with spaces/agentciv'
sh install.sh --prefix '/absolute/path with spaces/agentciv' --rollback
sh install.sh --prefix '/absolute/path with spaces/agentciv' --uninstall
```

```powershell
& .\install.ps1 -Version v0.1.0 -Prefix 'C:\Users\me\Applications\AgentCiv' -NoModifyPath
& .\install.ps1 -Prefix 'C:\Users\me\Applications\AgentCiv' -Rollback
& .\install.ps1 -Prefix 'C:\Users\me\Applications\AgentCiv' -Uninstall
```

`latest` resolves a small `VERSION` asset, then fetches binaries from that exact version. Re-running the installer explicitly updates it. There is no background updater. Assets are `COMPONENT-vX.Y.Z-TARGET`, with `.exe` on Windows, plus `SHA256SUMS`, installers, `VERSION`, and `LICENSE`. The intended native matrix is Linux x64/ARM64, Windows x64/ARM64, and macOS Intel/ARM64. Support requires actual release smoke tests; architecture detection alone is insufficient. Linux GNU builds must disclose their runner's libc floor. No source-build or architecture fallback occurs silently. GitHub documents free standard runners for public repositories with these architectures. [Runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).

## Installation boundaries

Windows release builds request a static MSVC C runtime and inspect actual imports for redistributable VCRUNTIME/MSVCP DLL dependencies. The existing local debug-binary smoke does not prove clean-machine compatibility. Static-runtime licensing and native target applicability remain release-review prerequisites. Rust documents the link flag and recommends inspecting the result rather than inferring linkage from a successful compile. [Rust runtime linkage](https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes).

Fixed GitHub release URLs and HTTPS redirects supply direct binaries, without archive extraction. SHA256 is checked before invoking `--version`; Windows uses `Get-FileHash`. A matching hash detects a changed download, not benevolence, operator authorization, or independent authentication. [Get-FileHash](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.utility/get-filehash?view=powershell-7.5).

POSIX requires curl 8.4 or newer to enforce byte limits even without a declared response length. Transfers have time and byte limits; metadata has smaller caps than binaries. User curl configuration is disabled for these requests. [curl transfer limits](https://curl.se/docs/manpage.html#--max-filesize).

The Windows downloader shares a 180-second elapsed budget across redirect requests and response reads. It applies the remaining budget to header and stream timeouts, rejects oversized declared responses before creating output, and checks streamed bytes against the same limit. HTTP response and stream resources are closed on success and failure. Header request timeout alone does not bound response-body reads. [Microsoft timeout documentation](https://learn.microsoft.com/en-us/dotnet/api/system.net.httpwebrequest.readwritetimeout?view=netframework-4.8.1).

Verified generations occupy version directories. Stable shims read one current-version pointer, switched only after verification and staging. Old executables remain for rollback and can keep running on Windows without overwrite. Reusing an existing generation for an update or rollback requires every selected binary, notice resource, and component manifest to be recorded in verified ownership. Release-equivalent bytes alone cannot grant ownership; unowned generation files cause rejection before activation or manifest changes. Unexpected prefixes, symlinks or reparse points, unowned command collisions, and changed owned files cause failure. Uninstall checks ownership hashes first and removes exact recorded files, never recursively deleting user directories. Unknown files, world databases, grants, model caches, and evidence remain. These checks prevent accidental overwrite and deletion; they do not defend against another process with the same account's write access.

POSIX defaults to `~/.local/share/agentciv` and prints its `bin` directory for PATH activation; no shell profiles change. Windows defaults to `%LOCALAPPDATA%\AgentCiv` and adds only a User PATH entry unless `-NoModifyPath` is supplied. Uninstall removes that entry only when the installer added it. Open a new terminal or use the printed session instruction. The first run is `agentciv-archive demo`: a built-in synthetic offline fixture with no file writes, network, model, or peer-artifact execution. Reader requests and host startup require separate explicit commands and operator configuration.

## Gates and publication

The manual release workflow requires successful CI for its exact source commit, matching package versions, and no pre-existing version tag. It never moves tags or publishes automatically; draft creation is an explicit opt-in. A [notice inventory](../release/NOTICE_INVENTORY.json) pins Cargo.lock, toolchain configuration, aggregate notice bytes, and individual copied sections. Its current `blocked_release_review` status intentionally stops release preflight. All 240 locked registry packages now have source and captured notice coverage, including commit-pinned repository supplements where published crates omitted license texts. Supplemental provenance is distinct from crate membership; original notice bytes and offsets remain unchanged. Full source applicability and the six native toolchains and platform runtimes still need review. Only a future `reviewed_complete` inventory with no blockers may proceed. Both notice assets and project LICENSE are checksum-verified and retained with installed binaries. Local fixtures do not establish completion of that legal review.

Default updates retain optional host tools. A different component set for an already installed exact version is rejected; add tools when installing a new version or reinstall after uninstall. Rolling back to a generation without an optional tool leaves an owned shim that reports the component is unavailable. A per-prefix lock rejects concurrent installers, and a stale lock requires inspection rather than automatic deletion. Windows uninstall refuses locked files before removing any recorded file.

Offline tests mock downloader commands or functions without a production URL override. Cases include pinned/latest versions, target selection, spaces, idempotence, optional tools, failed downloads, bad/duplicate hashes, multiline or traversal versions, unchanged activation on verification failure, rollback, ownership forgery, collisions, symlink/reparse boundaries, and retained unowned files. Native coverage must reach 80%, alongside shell syntax, ShellCheck, PowerShell parsing, and PSScriptAnalyzer. Mocked downloads alone do not prove published installation works.

Local validation on 2026-10-01 passed all twenty-one Windows PowerShell 5.1 fixture tests with Pester 4.10.1 at 389 of 418 commands, or 93.06% command coverage, with no PSScriptAnalyzer 1.24.0 warnings or errors. Downloader cases cover declared and streamed byte limits, HTTPS-only redirects, redirect limits, one elapsed deadline, and response cleanup. Both installers reject fully or partially unowned release-compatible generations during update reuse and rollback, preserving files, sentinels, activation, and ownership state. Installation, archive demo, version, reinstall, and uninstall smoke checks also passed with the current four real Windows debug binaries. The shell behavior fixtures, including duplicate ownership and an unterminated final manifest row, and syntax checks passed locally through Git's shell. The full shell fixtures passed in an isolated Ubuntu 26.04 container with kcov coverage of 166 of 171 installer lines, or 97.08%. ShellCheck 0.11.0 and shell syntax checks passed there. OS and downloader fixtures do not prove native release execution on Linux or macOS. Static-runtime release checks, all release-platform jobs, and actual published-download checks remain unexecuted. Their configured CI gates must pass before shipping.

Build assets into a draft, verify them, then publish with immutable releases enabled. GitHub immutability protects assets and tag identity and supplies release attestations. Optional GitHub CLI verification compares local assets to those releases; autogenerated source archives are excluded. A checksum manifest is separate from this provenance evidence. [Immutable releases](https://docs.github.com/en/code-security/concepts/supply-chain-security/immutable-releases), [release verification](https://docs.github.com/en/code-security/how-tos/secure-your-supply-chain/secure-your-dependencies/verify-release-integrity).

Primary interface precedents are the standalone installers documented by [Codex CLI](https://developers.openai.com/codex/cli), [Claude Code](https://code.claude.com/docs/en/setup), and [uv](https://docs.astral.sh/uv/getting-started/installation/). Their availability, update behavior, and signing are not AgentCiv claims. Per-user locations, version pinning, and PATH controls are useful precedents; AgentCiv uses explicit updates and a small utility scope. [uv installer options](https://docs.astral.sh/uv/reference/installer/).
