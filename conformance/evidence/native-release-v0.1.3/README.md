# Native utility v0.1.3 evidence packet

Released source: `f3551fcd2c21b54017da41352b9eba9486259e06`. Candidate run: `39102118997`. Published installation run: `39103512513`. Previous native generation: v0.1.2 at `c255dad79391386862e33da6efcddc7d9e090450`.

This packet copies validated outputs from separate candidate, published download/provenance, static linkage, Windows image, latest URL and six-target public installer stages. `packet-inventory.json` records each preserved file's representation, byte count and SHA-256. The inventory excludes itself to avoid a recursive hash claim.

Fetch/latest GitHub metadata was parsed and reserialized by its verifier; it is not original API response bytes. Stage run/API metadata and successful build/release verification stdout are original captured command bytes. Workflow artifact files and release identity/manifest files retain original encoding and line endings. Compressed static reports decompress to the exact hashes in linkage/verification.json. Derived summaries state bounded observations; their booleans do not substitute for original report bytes or independent verification.

The assembler performs local consistency checks only. It makes no network calls, downloads nothing, executes no release binary, and does not repeat cryptographic attestation verification. Stage metadata binding does not independently prove local folders' artifact archive origin. Original verifier success and actual workflow observations remain separate trust inputs. Full logs, downloaded binaries, duplicate notice/provider originals, signed redirect URLs and failed attempts remain outside this packet.

All six target artifacts retain previous/upgraded/rolled-back/upgraded-again/installed/reinstalled versions and installed hashes. These runner observations do not establish older OS support, OS signing, independent interoperability, model arrival, society or deployed website behavior. Preserve the v0.1.2 release and prior evidence unchanged. See the [release record](../../../docs/NATIVE_RELEASE_V0_1_3.md) for actual release scope and related evidence.

## Retained trust and download bindings

The `trust/` files preserve the successful official CLI TUF root output and one unchanged GitHub-only record selected for immutable release verification. Build provenance used the combined roots; release and asset signatures used the GitHub-only record. Selection retains timestamp authorities and does not change certificates or relax source, issuer, signer, invocation or asset checks. The selection metadata records original byte hashes and successful CLI status.

`published-archive-verification.json` separately records all six operator-downloaded installation artifact ZIPs matching authenticated API sizes and SHA-256 digests for the actual source/run. Original ZIPs remain ignored. Those API/download bindings are separate from released-asset signatures and native runner execution.

The complete candidate artifact download first exceeded the CLI's 300-second timeout. A direct API transfer also reached that original deadline; its remaining bytes were fetched using an exact HTTP 206 Content-Range under a longer bounded timeout. The completed original ZIP matched the authenticated API's full size and SHA-256, then all 34 safely extracted files matched the reviewed release assets and committed source copies. No partial download was accepted. `candidate-artifact-metadata.json` and `candidate-archive-verification.json` retain the final binding; failed attempts remain ignored. Published release assets were downloaded fresh, with the complete candidate archive revalidated against fresh API metadata. Neither the draft nor tag was rebuilt or moved.
