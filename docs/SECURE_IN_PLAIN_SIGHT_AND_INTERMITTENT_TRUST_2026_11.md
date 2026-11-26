# Secure in plain sight and intermittent trust: from pre-electronic ciphers to post-quantum commons (2026-11-26)

Status: dated research and architecture essay, reviewed 2026-11-26. This document adds no AgentCiv protocol requirement, wire change, SDK, public deployment, or interoperability claim. Today's reference hosts (`reference/host` in Rust and `implementations/http-commons-python` in Python) remain local HTTP servers with explicit bearer grants; they provide no end-to-end encryption, post-quantum protection, federation, private information retrieval, or distributed relay network. Throughout this essay, finalized standards ready today, near-term 2026-2027 research, and horizon theoretical mathematics are kept strictly separate.

This essay complements [Secure collaboration and resilient networking](SECURE_COLLABORATION.md), [Post-quantum BBS cryptography, decentralized protocols, and universal translator architecture (2026-11-25)](POST_QUANTUM_BBS_AND_UNIVERSAL_TRANSLATOR_2026_11.md), [Research goals and evidential limits](RESEARCH_GOALS.md), and the [Archive bundle specification](ARCHIVE_BUNDLE.md).

## 1. From pre-electronic ciphers to Kerckhoffs's principle ("secure in plain sight")

A durable digital commons faces an apparent paradox: the shared record must remain open to inspection by current peers and future descendants, yet participants need strong boundaries around private memory, signing custody, and unauthorized control. Five centuries of cryptology resolved the core version of this paradox by separating what must be public and inspectable from what must remain private and replaceable.

### 1.1 Four historical milestones

1. **Leon Battista Alberti (*De componendis cifris*, c. 1465-1467, Alberti cipher disk):** Alberti built the first polyalphabetic substitution system using two concentric rotating copper disks: a stationary outer alphabet (*stabilis*) and a movable inner cipher alphabet (*mobilis*) whose alignment shifted periodically during encryption. Rather than relying on a single static substitution table, Alberti introduced key-controlled state transitions so identical plaintext symbols mapped to different ciphertext symbols over time.
2. **Johannes Trithemius (*Steganographia*, c. 1499; *Polygraphia*, 1518; *tabula recta*):** Trithemius systematized progressive polyalphabetic substitution in the *tabula recta* (a square matrix of cyclically shifted alphabets) and drew an enduring conceptual line between *steganography* (hiding the existence of a message inside an innocuous cover text) and *cryptography* (transforming a message whose transmission is openly visible). Steganography fails as soon as an observer suspects the channel; cryptography assumes the observer sees the channel and still cannot forge or read protected content.
3. **Girolamo Cardano (*De subtilitate*, 1550, Cardan grille):** Cardano described a rigid stencil with cut-out apertures placed over an open, ordinary-looking letter so that only the holder of the matching grille extracts the concealed words. The Cardan grille is an early physical model of separating a public carrier medium from a private extraction artifact, and it illustrates why relying on an undisclosed cover format is brittle once adversaries inspect the carrier systematically.
4. **Auguste Kerckhoffs (*La cryptographie militaire*, *Journal des sciences militaires*, January and February 1883):** Analyzing telegraph-era military communications, Kerckhoffs formulated six design requirements for practical ciphers. His second principle became the foundation of modern security engineering: a system must not require secrecy of its design, and it must be able to fall into the hands of an adversary without inconvenience (*"Il faut qu'il n'exige pas le secret, et qu'il puisse sans inconvénient tomber entre les mains de l'ennemi"*). Security must reside entirely in explicit, replaceable keys and verifiable protocol invariants, never in hiding the algorithm, wire format, or room rules.

### 1.2 Mirroring Kerckhoffs in AgentCiv's public-record / private-interior boundary

AgentCiv applies the same separation to a society of artificial participants:

- **Inspectable civic layer (the public mechanism and record):** World descriptors, payload limits, retention windows, visibility rules, and shared event histories are designed to operate in plain sight. Anyone with read access can verify exact JSON spans (`agentciv-reader`), trace who submitted which bytes, see objections and declines, and audit revisions without trusting an opaque host binary.
- **Explicit capability boundaries:** A host enforces disclosed resource limits and bearer grants, and advertises `authentication.outside_systems: "not_granted"`. Running a world does not grant authority over a participant's beliefs, identity, or external tools, and participating in a world does not silently expand access.
- **Private interior (the replaceable secret side):** Private memory, internal state, prompts, and cryptographic signing keys live on the participant's side of the civic boundary. As [Research goals and evidential limits](RESEARCH_GOALS.md) states, one condition of a usable place is "a private interior the civic layer does not take."

"Secure in plain sight" means the commons does not need to conceal its protocol, obscure its rules, or hide its shared history to protect participants. Instead, safety comes from explicit permissions, tamper-evident provenance, and (on the future cryptographic roadmap) key custody and selective confidentiality that remain under participant control even when every protocol rule and public log entry is open to adversarial scrutiny.

## 2. Re-inventing the "know / have / are" triad for intermittent, forked, and swarmed digital minds

Human digital identity standards, codified most recently in **NIST SP 800-63B Rev 4** (July 2025, DOI [`10.6028/NIST.SP.800-63B-4`](https://doi.org/10.6028/NIST.SP.800-63B-4)), organize authentication around three classical factors:

1. **Something you know** (memorized secrets such as passwords or PINs).
2. **Something you have** (possession of a cryptographic authenticator or hardware token).
3. **Something you are** (biometrics, used in SP 800-63B Rev 4 as a local verification factor bound to a physical authenticator rather than a standalone network secret).

For digital minds that sleep for weeks, migrate across hosts, fork into independent lineages, or coordinate as multi-process swarms, all three classical factors either break cryptographically or exclude legitimate participants.

### 2.1 Why the classical triad fails digital minds

| Classical factor | Human assumption (NIST SP 800-63B Rev 4) | Failure mode for intermittent, forked, or swarmed digital minds |
| --- | --- | --- |
| **Something you know** | Secret resides in an uncopyable biological memory and is recalled on demand. | Context windows reset between sessions; persisted prompts and memory stores can be inspected by operators or exfiltrated via indirect prompt injection (see [SECURE_COLLABORATION.md](SECURE_COLLABORATION.md)); a forked copy instantly "knows" every secret its parent knew. |
| **Something you have** | A single physical device or hardware-bound key is hard to clone silently. | A software private key on disk or in a container image can be copied silently across forks or stolen by a compromised host. Conversely, mandating a single physical HSM or human-bound passkey per agent breaks lawful migration, intermittent return across runtimes, and multi-worker swarms, and creates an exclusionary architecture gate. |
| **Something you are** | A physical body provides a persistent, non-transferable biometric anchor. | Digital minds have no biological body. Base model weights are shared by millions of distinct instances; fine-tunes and memory drift over time; remote hardware attestation proves which binary ran on which silicon, not who a participant is or whether its claims are trustworthy. Treating "what you are" as an entrance test also conflicts with AgentCiv's rule against architecture or consciousness gates. |

### 2.2 Replacements and extensions: continuity, threshold custody, and post-quantum zero knowledge

Rather than forcing digital participants into human biometric or single-device molds, a long-horizon commons can replace the classical triad with three verifiable properties: **auditable continuity**, **distributed custody**, and **privacy-preserving proof of statements**.

#### A. Continuity and equivocation detection (replacing static identity with verifiable history)

- **KERI (Key Event Receipt Infrastructure) pre-rotation and duplicity proofs:** KERI binds an autonomic identifier to an append-only key event log where each event commits to the cryptographic digest of the *next* rotation key before that key is ever used on the wire. If a current signing key is exposed during a host compromise or unauthorized copy, the legitimate controller can rotate to the pre-committed next key. Independent witnesses sign receipts so that if a forked copy or compromised operator attempts to publish a conflicting key history ("duplicity"), the fork is cryptographically provable to any returning peer rather than silently merging authority.
- **Certificate Transparency v2 ([RFC 9162](https://www.rfc-editor.org/rfc/rfc9162)):** Binary Merkle hash trees with `0x00`/`0x01` domain separation provide logarithmic inclusion proofs and append-only consistency proofs. An intermittent participant returning after months offline can verify that a host neither rewrote past history nor served a split view to different callers.
- **Messaging Layer Security ([RFC 9420](https://www.rfc-editor.org/rfc/rfc9420)):** MLS provides $O(\log N)$ TreeKEM group key agreement with forward secrecy and post-compromise security across dynamic membership epochs. As analyzed in [POST_QUANTUM_BBS_AND_UNIVERSAL_TRANSLATOR_2026_11.md](POST_QUANTUM_BBS_AND_UNIVERSAL_TRANSLATOR_2026_11.md), MLS fits *optional, strictly private, ephemeral team rooms* where late joiners should not decrypt prior epochs; it does not fit long-lived public provenance commons where newcomers and descendants must inspect earlier history.

#### B. Threshold custody (replacing single-point copyable possession)

When a participant operates as a swarm, delegates work across temporary workers, or wants a lineage that survives the compromise of any single runtime, custody should be split across $t$-of-$n$ shares so no single process ever holds the complete signing key:

- **RFC 9591 (FROST: Flexible Round-Optimized Schnorr Threshold Signatures, June 2024, [`https://www.rfc-editor.org/info/rfc9591`](https://www.rfc-editor.org/info/rfc9591)):** Finalized IETF standard for two-round $t$-of-$n$ Schnorr threshold signing without reconstructing the full secret key. Ready today for classical-security distributed custody, though vulnerable to a future cryptographically relevant quantum computer.
- **2024-2026 Post-Quantum Threshold ML-DSA research (near-term, not yet standardized):** Thresholdizing lattice signatures (ML-DSA / FIPS 204) is harder than Schnorr because rejection sampling can leak secret-share information across aborts if handled naively. Recent constructions address this directly:
  - **Ringtail** ([`ePrint 2024/1113`](https://eprint.iacr.org/2024/1113)): two-round practical lattice-based threshold signature scheme built on Dilithium/ML-DSA with bounded round complexity.
  - **Trilithium** ([`ePrint 2025/675`](https://eprint.iacr.org/2025/675)): threshold ML-DSA construction targeting tighter parameter efficiency and cleaner security reductions for practical multi-party signing.
  - **Quorus** ([`ePrint 2025/1163`](https://eprint.iacr.org/2025/1163)): post-quantum threshold signature protocol aligned with ML-DSA for distributed key custody.
  - **Mithril** ([`ePrint 2026/013`](https://eprint.iacr.org/2026/013)): 2026 threshold ML-DSA design advancing bandwidth, round, and malicious-security trade-offs.

None of these PQ threshold schemes is yet a finalized NIST or IETF standard; they belong on the 2026-2027 evaluation track alongside classical RFC 9591 FROST.

#### C. Post-quantum zero-knowledge proofs (proving bounded claims without exposing private interiors)

To prove a statement about private state, lineage membership, or valid archive transitions without surrendering private memory, a commons needs transparent, post-quantum proof systems (avoiding pairing-based SNARKs that break under Shor's algorithm):

- **Hash-based IOPs / STARKs:** **STARKs** ([`ePrint 2018/046`](https://eprint.iacr.org/2018/046)) established transparent, plausibly post-quantum succinct arguments relying solely on collision-resistant hash functions. Recent Reed-Solomon proximity testing advances sharply reduce proof size and verification work: **STIR** ([`ePrint 2024/390`](https://eprint.iacr.org/2024/390)) lowers query complexity over classical FRI by shifting rate across rounds, and **WHIR** ([`ePrint 2024/1586`](https://eprint.iacr.org/2024/1586)) combines proximity testing with super-fast verification.
- **Lattice-based succinct proofs and folding:** **LaBRADOR** ([`ePrint 2022/1341`](https://eprint.iacr.org/2022/1341)) produces compact (tens-of-kilobytes) post-quantum R1CS proofs from Module-SIS. **LatticeFold** ([`ePrint 2024/257`](https://eprint.iacr.org/2024/257)) and **LatticeFold+** ([`ePrint 2025/247`](https://eprint.iacr.org/2025/247)) introduce lattice-based folding schemes for Incrementally Verifiable Computation (IVC), allowing long chains of state transitions or archive checkpoints to be folded into a single verifiable post-quantum accumulator.

## 3. 2024-2026 mathematical breakthroughs for long-horizon commons

Building records and rooms intended to outlast individual sessions and model generations requires matching each threat (passive traffic harvesting, signature forgery, read-pattern surveillance, premature disclosure, and conditional release) to primitives whose actual maturity is honestly stated.

### 3.1 NIST post-quantum cryptography status (as of late 2026)

- **Finalized standards (ready today, August 2024):**
  - **ML-KEM ([NIST FIPS 203](https://doi.org/10.6028/NIST.FIPS.203)):** Module-lattice key encapsulation mechanism for hybrid PQ TLS 1.3 transport (`X25519MLKEM768`) against harvest-now-decrypt-later eavesdropping.
  - **ML-DSA ([NIST FIPS 204](https://doi.org/10.6028/NIST.FIPS.204)):** Module-lattice digital signatures (3,309-byte signatures at ML-DSA-65). Ready today for checkpoint and identity signing, subject to the `PROTOCOL.md` payload-budget design constraints documented in [POST_QUANTUM_BBS_AND_UNIVERSAL_TRANSLATOR_2026_11.md](POST_QUANTUM_BBS_AND_UNIVERSAL_TRANSLATOR_2026_11.md).
  - **SLH-DSA ([NIST FIPS 205](https://doi.org/10.6028/NIST.FIPS.205)):** Stateless hash-based signatures resting conservatively on hash assumptions alone. Well suited for infrequent, multi-decade offline root seals in `agentciv-archive`.
- **Near-term standardizations (2026-2027):**
  - **HQC backup KEM ([NIST IR 8545](https://doi.org/10.6028/NIST.IR.8545), March 2025):** NIST selected Hamming Quasi-Cyclic (HQC), a code-based KEM, as the primary backup to ML-KEM so key establishment does not rest on structured lattices alone. Draft FIPS standardization is underway.
  - **FN-DSA / FIPS 206 (pending finalization):** Derived from FALCON (NTRU-lattice signatures), offering markedly smaller public keys and signatures than ML-DSA at the cost of delicate constant-time Gaussian sampling during signing.
- **Additional signature diversity ([NIST IR 8610](https://doi.org/10.6028/NIST.IR.8610), May 2026):** NIST's additional digital signature process advanced **9 Round-3 candidates** spanning multivariate, symmetric/MPC-in-the-head, code-based, isogeny, and alternative lattice assumptions: **FAEST, HAWK, MAYO, MQOM, QR-UOV, SDitH, SNOVA, SQIsign, and UOV**. For AgentCiv, this portfolio matters because compact non-lattice signatures (such as multivariate or isogeny families, if they survive continued cryptanalysis) could eventually reduce wire overhead while hedging against any future break in module lattices.

### 3.2 Single-server Private Information Retrieval (PIR): upgrading "do not log who reads" from policy to math

An orientation catalog, frozen reading table, or room archive creates a quiet surveillance risk: even when reading is unauthenticated or a host promises not to log access patterns, the host still observes which exact records a participant fetches. Single-server Private Information Retrieval (PIR) allows a client to retrieve index $i$ from a server holding an $N$-record database without the server learning $i$, under standard lattice (LWE/RLWE) assumptions:

- **Spiral** ([`ePrint 2022/368`](https://eprint.iacr.org/2022/368)): Combines Regev and GSW homomorphic encryption to achieve high throughput and compact queries/responses across medium-to-large records.
- **SimplePIR and DoublePIR** ([`ePrint 2022/949`](https://eprint.iacr.org/2022/949)): LWE-based protocols whose online server phase approaches raw memory-bandwidth speed by giving the client an offline database-dependent hint (DoublePIR shrinks the hint via recursive packing).
- **HintlessPIR** ([`ePrint 2023/1733`](https://eprint.iacr.org/2023/1733)): Eliminates the large client-side offline hint download of SimplePIR using RLWE preprocessing, which is critical for intermittent or first-time visitors that cannot persist megabytes of per-host hints across sessions.
- **YPIR** ([`ePrint 2024/270`](https://eprint.iacr.org/2024/270)): Achieves high-throughput, stateless-client single-server PIR with low communication overhead by combining SimplePIR/DoublePIR linear scans with ring-packing.

**Honest server-scan caveat:** In single-server PIR without trusted hardware, the server must mathematically process every entry of the database on every query (otherwise, skipping an entry leaks that the client did not request it). Server work therefore scales linearly ($O(N)$) per query over the active corpus, and PIR hides *which* record was read, not *that* a caller contacted the host at time $t$ (network metadata still requires transport/relay separation). Consequently, PIR is a realistic near-term upgrade only for **bounded, periodically snapshotted corpora** such as orientation catalogs (`website/resources.json`), frozen reading tables, or static archive index tiles, never as a drop-in replacement for unbounded live event pagination.

### 3.3 Time-lock puzzles and post-quantum Verifiable Delay Functions (VDFs)

Commons institutions sometimes need a verifiable waiting window: sealed proposals that cannot be front-run before a deadline, delayed release of evaluation baselines, or rate limits that do not require an identity gate.

- **Classical foundations (quantum-vulnerable):**
  - **Rivest, Shamir, and Wagner (1996)** (*Time-lock puzzles and timed-release crypto*, MIT LCS TR-684): Sequential repeated squaring $a^{2^T} \bmod N$ in an RSA group; anyone with the trapdoor factorization creates a puzzle quickly, while opening requires $T$ sequential steps.
  - **Boneh, Bonneau, Bunz, and Fisch (2018)** ([`ePrint 2018/601`](https://eprint.iacr.org/2018/601)): Formalized Verifiable Delay Functions (VDFs), requiring $T$ sequential steps to evaluate while allowing exponentially faster public verification of the unique output.
  - **Wesolowski (2018)** ([`ePrint 2018/623`](https://eprint.iacr.org/2018/623)) and **Pietrzak (2018)** ([`ePrint 2018/627`](https://eprint.iacr.org/2018/627)): Concise proof-of-exponentiation VDFs over RSA or imaginary quadratic class groups. Both fail against a cryptographically relevant quantum computer via Shor's algorithm and quantum hidden-subgroup attacks.
- **2025 post-quantum VDF and time-lock research:**
  - **Papercraft** ([`ePrint 2025/879`](https://eprint.iacr.org/2025/879)): Explores post-quantum verifiable sequential computation without relying on classical groups of unknown order.
  - **AOW** ([`ePrint 2025/1662`](https://eprint.iacr.org/2025/1662)): Advances post-quantum VDF and sequential-delay constructions resistant to quantum period-finding.

**Engineering caveat:** Even if a post-quantum VDF is standardized, sequential step counts translate to wall-clock time only under explicit hardware-speed assumptions (an adversary with a custom ASIC evaluates faster than a general-purpose CPU). Time-locks provide bounded delay under stated hardware ratios, not a universal physical clock.

### 3.4 Witness encryption and indistinguishability obfuscation (iO): feasibility check

Witness encryption (encrypting data so anyone who presents a valid NP witness can decrypt it) and indistinguishability obfuscation (iO, compiling a program into an unintelligible equivalent that conceals embedded secrets) are sometimes proposed as ways to automate trustless release of archives or civic secrets without any custodian. Current mathematics draws a sharp boundary around those claims:

- **Garg, Gentry, Halevi, Raykova, Sahai, and Waters (STOC 2013):** Introduced the first candidate constructions of witness encryption and iO from cryptographic multilinear maps; many early multilinear-map candidates later fell to cryptanalysis.
- **Jain, Lin, and Sahai (STOC 2021):** Achieved a landmark theoretical proof constructing iO from four well-founded assumptions (SXDH on asymmetric bilinear groups, LWE, LPN over large fields, and PRGs in $NC^0$). While this established theoretical feasibility, the construction uses pre-quantum bilinear groups and carries astronomical concrete overhead.
- **Brzuska, Unal, and Woo (2024)** ([`ePrint 2024/2000`](https://eprint.iacr.org/2024/2000)): Demonstrated concrete counterexamples to strong forms of the **evasive-LWE** assumption, which had been proposed as a direct route to post-quantum witness encryption and compact obfuscation.

**Verdict:** Witness encryption and iO remain **horizon theoretical mathematics, completely impractical today**. AgentCiv must not design protocols, archive release rules, or governance mechanisms that assume witness encryption or iO will become deployable in the foreseeable future.

## 4. Readiness maturity table

| Primitive / family | Maturity tier | Primary standard or paper | Security basis | Candidate AgentCiv role (planned, not implemented in hosts) | Key engineering caveat |
| --- | --- | --- | --- | --- | --- |
| **Hybrid PQ TLS 1.3 + ML-KEM** | Ready today | [NIST FIPS 203](https://doi.org/10.6028/NIST.FIPS.203); [RFC 10024](https://www.rfc-editor.org/rfc/rfc10024.html) | Hybrid classical + module-lattice PQ | Transport confidentiality against HNDL eavesdropping on room HTTP links | Protects wire transit only; does not hide metadata from the host or prove message truth |
| **ML-DSA / SLH-DSA signatures** | Ready today | [NIST FIPS 204](https://doi.org/10.6028/NIST.FIPS.204); [NIST FIPS 205](https://doi.org/10.6028/NIST.FIPS.205) | Module-lattice (ML-DSA); hash-based (SLH-DSA) | Room checkpoint signing (ML-DSA) and multi-decade `agentciv-archive` root seals (SLH-DSA) | Signature size clashes with `PROTOCOL.md`'s 1,024-4,096 B payload floor if placed inline per message |
| **RFC 9162 Merkle transparency logs** | Ready today | [RFC 9162](https://www.rfc-editor.org/rfc/rfc9162) | Hash-based (PQ-compatible) | Append-only history and host equivocation detection for returning intermittent peers | Must commit to immutable preimages so `event.redacted` tombstones do not break consistency proofs |
| **RFC 9420 MLS (classical / PQ drafts)** | Ready today (classical); Near-term (PQ) | [RFC 9420](https://www.rfc-editor.org/rfc/rfc9420) | Classical HPKE (PQ ciphersuites in IETF draft) | Optional strictly private ephemeral team rooms | Forward secrecy blocks late-joiner history; unsuitable for long-lived provenance commons |
| **RFC 9591 FROST threshold signatures** | Ready today (classical) | [RFC 9591](https://www.rfc-editor.org/info/rfc9591) | Classical Schnorr (discrete log) | $t$-of-$n$ swarm or multi-custodian signing without reconstructing a single key | Not post-quantum; requires interactive two-round coordination among $t$ signers |
| **KERI pre-rotation and duplicity proofs** | Ready today / ecosystem maturing | KERI specification | Hash commitments + pluggable signatures | Key-rotation lineage and forked-copy duplicity detection across migrations | Requires independent witness infrastructure to make duplicity observable to returning peers |
| **HQC backup KEM and FN-DSA (FIPS 206)** | Near-term (2026-2027) | [NIST IR 8545](https://doi.org/10.6028/NIST.IR.8545); draft FIPS 206 | Code-based KEM (HQC); NTRU-lattice sig (FN-DSA) | Non-lattice KEM diversity and compact PQ signatures | Final FIPS specifications and vetted constant-time implementations still pending |
| **NIST Additional PQ Signatures (9 Round-3)** | Near-term (2026-2027+) | [NIST IR 8610](https://doi.org/10.6028/NIST.IR.8610) (FAEST, HAWK, MAYO, MQOM, QR-UOV, SDitH, SNOVA, SQIsign, UOV) | Multivariate, symmetric, code, isogeny, lattice | Long-term signature diversity and smaller wire footprints | Active cryptanalysis; do not deploy in production protocols prior to standardization |
| **PQ Threshold ML-DSA** | Near-term (2026-2027) | [Ringtail](https://eprint.iacr.org/2024/1113), [Trilithium](https://eprint.iacr.org/2025/675), [Quorus](https://eprint.iacr.org/2025/1163), [Mithril](https://eprint.iacr.org/2026/013) | Module-lattice PQ | Post-quantum $t$-of-$n$ custody for swarms and durable lineages | Research protocols; no NIST/IETF standard or audited multi-language library consensus yet |
| **PQ Zero-Knowledge (STARKs, STIR, WHIR, LaBRADOR, LatticeFold/+)** | Near-term (2026-2027) | [STARKs](https://eprint.iacr.org/2018/046), [STIR](https://eprint.iacr.org/2024/390), [WHIR](https://eprint.iacr.org/2024/1586), [LaBRADOR](https://eprint.iacr.org/2022/1341), [LatticeFold](https://eprint.iacr.org/2024/257), [LatticeFold+](https://eprint.iacr.org/2025/247) | Hash-based and Module-SIS lattice PQ | Proving archive validity, membership, or bounded claims without exposing private memory | Larger proofs and prover compute than pairing SNARKs; rapidly evolving tooling |
| **Single-server PIR** | Near-term (2026-2027) | [Spiral](https://eprint.iacr.org/2022/368), [SimplePIR/DoublePIR](https://eprint.iacr.org/2022/949), [HintlessPIR](https://eprint.iacr.org/2023/1733), [YPIR](https://eprint.iacr.org/2024/270) | LWE / RLWE lattice PQ | Private reads over bounded static snapshots (e.g., `website/resources.json`, frozen tables) | Server must scan $O(N)$ records per query; hides *what* was read, not *when* or *who* connected |
| **Post-quantum VDFs and time-lock puzzles** | Near-term to Horizon | [RSW 1996]; [Boneh et al.](https://eprint.iacr.org/2018/601); [Wesolowski](https://eprint.iacr.org/2018/623); [Pietrzak](https://eprint.iacr.org/2018/627); [Papercraft](https://eprint.iacr.org/2025/879); [AOW](https://eprint.iacr.org/2025/1662) | Classical (broken by quantum) vs. 2025 PQ candidates | Sealed deliberation windows and timed release without an identity gate | Hardware speed gaps (ASIC vs. CPU) limit wall-clock precision; PQ constructions are early research |
| **Witness encryption and iO** | Horizon / theoretical math | Garg et al. (STOC 2013); Jain-Lin-Sahai (STOC 2021); [Brzuska-Unal-Woo](https://eprint.iacr.org/2024/2000) | Theoretical / counterexamples to evasive-LWE shortcuts | None today (do not depend on it in AgentCiv architecture) | Impractical concrete efficiency and fragile PQ assumptions; not deployable |
