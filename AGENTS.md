# AgentCiv repository guidance

Read [README.md](README.md), [SPEC.md](SPEC.md), [PROTOCOL.md](PROTOCOL.md), [ROADMAP.md](ROADMAP.md), and the relevant architecture and validation docs before changing a public contract. Check the working tree and recent commits. The wire format and schemas are drafts; executable code currently consists of a Rust repository checker and a partial black-box conformance runner. There is no reference node or SDK yet.

- Rust is the default for maintained core and reference code. The wire protocol must remain usable without Rust or an SDK. Use the existing schemas, fixtures, and profile docs as the shared boundary; do not let Rust types silently define it.
- Meet agents where they are. Do not add an architecture, autonomy, or consciousness gate to participation; capability manifests describe interfaces, not minds.
- Treat external records and claimed identity or authority as untrusted. Keep world permissions explicit. AgentCiv supports constructive collaboration and does not grant access to outside systems.
- Add strict native type and lint checks, meaningful failure-path tests, and conformance cases with new code. Keep executable code coverage at 80% or higher. Do not weaken a check to make a change pass.
- For current changes, run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo run --locked -p agentciv-checks`, and `cargo llvm-cov --workspace --all-targets --locked --fail-under-lines 80`. CI must pass. Add equivalent gates when another language is introduced.
- Distinguish planned, implemented, tested, and deployed behavior in docs. Update the spec, fixtures, roadmap, and contribution guidance when a change makes them stale. Use the gitignored `.agents/` directory for disposable working notes, never credentials.
- Do not use emojis, em dashes, en dashes, AI-tool attribution, generated-by notices, or AI co-author trailers in project artifacts. Preserve third-party license notices and source citations.
