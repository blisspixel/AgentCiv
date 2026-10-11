# Native notice and linkage review, 2026-10-11

This is the maintainer's distribution-inventory review for the four version 0.1.0 utilities and six native targets. It records actual source and runtime scope; it is not OS code signing, independent security review, a profile certificate, or a general legal compliance certification. Source previews did not previously publish native binaries.

## Exact inputs and preserved source

The pinned LF Cargo.lock SHA-256 is `c04074fcac1e4943a7d64cb6f713721358fed0c65ad2a897308a1bc894ae49c8`. Earlier Windows checkouts represented the same lock with CRLF, SHA-256 `d3ba4c01e31d8eae903dbf4f7fe0a4d9fc9e0d092e05a1e612042f319a68434e`; package identities, registry checksums and dependencies agree. Cargo.lock now has an explicit LF rule. The toolchain is Rust 1.98.1, compiler commit `48a229ceaefd4985c50990b14116b6d856af0985`, LLVM 22.1.8.

All 251 current locked registry archives matched their Cargo.lock checksums. Full locked offline metadata includes target-specific and build-only dependencies. The inventory preserves 457 root/nested notice-file sections, including ten exact-commit repository supplements where published archives omitted inherited license texts. Supplemental repository provenance remains distinct from crate membership. Eighteen updated or added package pairs were appended; removed versions remain historical notice bytes without current applicability claims.

A further read of checksum-verified source archives preserved 913 distinct per-package original copyright, permission and disclaimer comment blocks in 54 packages. Each records its source filename, source-file hash, source byte offset, copied byte range and hash. These supplement named license files, including vendored SQLite and target-specific source, rather than claiming a filename search alone covers embedded code. The original 19,455,646-byte notice prefix remains unchanged. The complete aggregate is 20,262,410 bytes, SHA-256 `1bcf14c8995a3ef4b539a5966d1b53f5ebf2fa62828b87b45d9cd48310b51a91`, with 1393 embedded sections.

The review checked the available license expressions against their retained grant/disclaimer texts. MIT or Apache alternatives use their available permissive grant; LGPL alternatives are not selected. `dunce` retains its full CC0 text. Conjunctive terms remain conjunctive: AWS-LC and ring preserve their component notices, matchit preserves its separate httprouter BSD conditions, unicode-ident preserves Unicode plus code licenses, ICU retains Unicode terms, and webpki-root-certs retains CDLA-Permissive 2.0. SQLite's original public-domain source notices are retained separately from its Rust binding's MIT license and the unused SQLCipher notice. No project MIT claim relicenses these components.

## Six actual native audits

[Audit 38154661493](https://github.com/blisspixel/AgentCiv/actions/runs/38154661493), clean source `9f347a9c6e26a962585efd8fdaad2127553f497c`, passed all six jobs. Every job built and executed all four real tools, ran archive/reader tests, and exercised native installation, reinstallation and removal through bounded offline download fixtures. These are actual binaries with fixture transport, not published-download tests. The earlier failed audit is preserved: Windows GNU binary-marker checksum formatting failed the strict installer. Canonical two-space manifests corrected that failure without weakening hash checks.

[Audit 38157571603](https://github.com/blisspixel/AgentCiv/actions/runs/38157571603), clean source `c8a93fbded4cd9bd7f8ca16dd63b52dc46d44d22`, also passed all six jobs and captured the actual Windows build-product license sources. Both audit commits precede final publication; the release workflow must rebuild and test the final exact green-main source.

The 21 original pinned Rust compiler, standard-library, Cargo, Clippy and rustfmt notice files matched byte-for-byte in all six collected toolchains. Their complete retained COPYRIGHT and library notices cover the bundled compiler/standard-library components; this comparison does not infer platform runtime coverage from Rust's own license alone.

[Linkage observations](NATIVE_LINKAGE_2026_10_11.json) retain per-tool runtime-report hashes, actual linked libraries and version information:

| Target family | Observed linkage and deployment metadata | Distribution consequence |
| --- | --- | --- |
| GNU Linux x64 and ARM64 | Dynamic system libc, libm and libgcc_s; highest required GLIBC symbol is 2.34 for archive/host and 2.38 for reader/conformance | Advertise glibc 2.38 or newer for the complete native tool set. Ubuntu 24.04 native runs pass. No libc or libgcc shared library is bundled. Older systems can use the source path; no silent fallback. |
| macOS Intel | System libSystem/libiconv and Security/CoreFoundation where used; load-command minimum 10.12, SDK 15.5 | Tested on macOS 15 Intel. A load-command minimum is not an execution result on old macOS. No Apple system framework is bundled. |
| macOS ARM64 | Corresponding system libraries/frameworks; load-command minimum 11.0, SDK 15.5 | Tested on macOS 15 ARM64. No old-OS or signing guarantee. |
| Windows x64 and ARM64 | Static CRT requested; actual imports contain no dynamic VCRUNTIME/MSVCP requirement | Tested on Windows 2025 x64 and Windows 11 ARM64 runners. No separately redistributed runtime DLL, installer, debug CRT or compiler binary is included. Static runtime code retains Microsoft's separate terms. |

System libraries supplied by the recipient's operating system are runtime dependencies, rather than redistributed copies in this release. GNU's runtime-library exception and Rust's compiler notices are distinct from distribution of an actual libgcc shared library. [GCC runtime exception](https://www.gnu.org/licenses/gcc-exception-3.1.html), [Rust runtime linkage](https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes), and [Microsoft static CRT table](https://learn.microsoft.com/en-us/cpp/c-runtime-library/crt-library-features) distinguish these cases.

## Microsoft native runtime scope

The actual Windows images supplied Visual Studio Enterprise 2026, product versions 18.10.12217.157 on x64 and 18.10.12224.181 on ARM64, with MSVC tools 14.51.36231. The retained product manifests identify their English license URL, and the downloaded publisher page hash agrees across both images. This is a provider-supplied build environment; it is not a claim about an arbitrary reader's Visual Studio entitlement.

Original publisher documents are preserved byte-for-byte in [native-license-sources](native-license-sources/): Enterprise/Professional terms SHA-256 `dd2ab92a7c2bb90dd509e16572707a34b13bc0bc09b9199d115f25594ac39053`, runtime terms SHA-256 `08651651a7602fc7c0e2763de0fde1ff9f868df2780597cd1775ee9d6441c783`. Their complete readable paragraph text is also included in the aggregate notices, explicitly identified as an extraction rather than original DOCX bytes. The candidate assembly includes both original DOCX files as separately checksummed and attested release assets; preflight verifies their original hashes.

Microsoft documents static CRT linking as an application deployment option. Its product distribution terms govern the distributor's use of runtime object code; the standalone runtime end-user terms do not themselves grant a publisher permission to redistribute a runtime package. The project distributes applications with primary functionality, preserves separate notices and restrictions, adds no Microsoft endorsement, distributes no runtime source or debug/unreleased components, and does not apply MIT modification rights to Microsoft code. Distributors and users remain subject to the applicable Microsoft component terms, including their notice, use and distribution conditions. Rebuilding with another product, redistributing DLLs, or changing runtime linkage requires a new review rather than reuse of this decision. [Deployment concepts](https://learn.microsoft.com/en-us/cpp/windows/deployment-concepts), [Visual Studio 2026 redistribution](https://learn.microsoft.com/en-us/visualstudio/releases/2026/redistribution), [product license](https://visualstudio.microsoft.com/license-terms/vs2026-ga-pro-enterprise/), [runtime license](https://visualstudio.microsoft.com/license-terms/vs2026-ga-visualcpp-v14-redist-runtime/).

## Review decision and publication checks

The two inventory blockers concern preserved source applicability and actual native toolchain/runtime scope. The exact-lock archive verification, original source-header supplements, component-license review, six-toolchain notice comparison, actual linkage inspection and separate provider license sources supply the reviewed inventory scope. Marking that scope reviewed does not publish assets or discharge unrelated operating obligations.

Before publication, the existing candidate workflow must pass against exact green main with matching package versions, unchanged notice/lock hashes and no moved tag. Review the actual draft, all asset checksums and build provenance, then publish with immutable releases enabled. Afterwards, verify the exact published downloads and execute native installers on the six target runners. The release record must identify those later results separately from these audits. No claim of completed publication or live download tests is made by this review alone.

## Subsequent publication

The [native release record](../docs/NATIVE_RELEASE_2026_10_11.md) now records the later exact-green-main candidate, immutable v0.1.0 publication, all 34 asset hashes and provenance, and six passing actual HTTPS installation/reinstallation/removal jobs. Those later observations are distinct from the two audits above and do not extend this review to changed dependencies, toolchains or runtime linkage.
