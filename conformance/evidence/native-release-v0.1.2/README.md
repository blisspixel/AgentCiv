# Native utility v0.1.2 evidence packet

Released source: `c255dad79391386862e33da6efcddc7d9e090450`. Candidate run: `38264682874`. Published installation run: `38327809779`. Previous native generation: v0.1.1. Published at `2026-10-13T00:33:03Z`.

This packet keeps the published release identity files, the successful `gh release verify` attestation, the release API object, the installation-run metadata, and the six original public-install evidence directories. `SHA256SUMS` covers the other 33 assets and does not checksum itself. Its SHA-256 is `f81716e6b7d5ce714df24ee509cf1d4f2793db51e570ae5d4aa60bffd78b60c1`. Local review of the draft matched every manifest row before publication, and a fresh download of the published manifest matched those same bytes.

The packet does not repeat a static linkage inspection of all 24 binaries. The candidate workflow checked Windows imports for a separately installed MSVC runtime, and the six public jobs executed the installed tools. Registry dependencies, the pinned toolchain, and aggregate notice bytes are unchanged from v0.1.1, as recorded in [the lock review](../../../release/MAINTENANCE_LOCK_V0_1_2.json). Those comparisons do not replace a new linkage review. Older operating systems, OS signing, and independent distribution review remain unestablished.

Fetch metadata was reserialized by the GitHub CLI. Workflow evidence files retain the bytes downloaded from the run artifacts. Full logs, downloaded binaries, and duplicate notice documents stay outside this packet. See the [release record](../../../docs/NATIVE_RELEASE_V0_1_2.md).
