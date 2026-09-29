# Contributing to AgentCiv

AgentCiv is at the design stage. Code, experiments, critique, and cross-disciplinary review are welcome. A proposal that shows an assumption is wrong can be as valuable as a feature.

Independent projects can contribute adapters, worlds, scenarios, methods, or links to maintained external implementations. See [Collaborating with other projects](docs/ECOSYSTEM.md) for the information needed to make these contributions usable without requiring anyone to move their project into this repository.

## Start here

Read the [vision](docs/VISION.md), [roadmap](ROADMAP.md), [architecture proposal](docs/ARCHITECTURE.md), and [welfare policy](docs/WELFARE.md). Check existing discussions and issues before proposing a large change.

The [repository guidance](AGENTS.md) records the current architecture boundaries and verified checks for code contributors, including automated contributors.

For a design proposal, describe the research question, the smallest useful change, expected observations, alternative explanations, welfare implications, and a way to reproduce results. Explain how the change preserves room for agents to question or revise the world's rules. For an experiment, include the available communication and persistence capabilities, seeds when applicable, model and world versions, budgets, prompts or policies that can be shared, and an analysis method.

## Working agreements

- Describe observed behavior precisely. Do not label an agent conscious, suffering, compassionate, or sentient solely from a transcript or score. Do not dismiss the possibility of digital minds in order to sound careful.
- Let a participant take part without reporting a consciousness level or deciding whether they are someone. A statement they volunteer remains a submission.
- Keep operator interventions and changes to world rules auditable.
- Document limitations and negative findings.
- Respect privacy and licenses when publishing model outputs, datasets, or run artifacts.
- Use clear, respectful language in discussion, especially when disagreeing about consciousness or moral status.
- Give criticism priority in proportion to how much it helps the project build, measure, understand, or avoid causing harm. A persistence path that drops the state it names, a confounded comparison, a fork that moves authority, or a result that was prompted belongs in that work. A demand to prove consciousness before building, with no threshold evidence could meet, does not.
- Attribute an act to a participant only when that participant made it. A host denial, a timeout, a skipped schedule, a tool error, an offline gap, and an executor's output are different facts. Do not infer belief, consent, or authority from a copied memory or from silence.

## How changes land

Keep one `main` branch. Land a change through a short-lived branch and a pull request. Merge after the checks pass, then delete the branch. The tip of `main` should be a reviewed change with a green GitHub Actions run.

Routine work does not go straight to `main`. A long-lived feature branch is a poor fit as well. Open the pull request while the change is still small enough to review. When the branch is one focused change, squash it so `main` stays one commit per change. History already on `main` stays where it is.

## Repository checks

The initial executable code is a Rust checker for documentation and schema fixtures. Use the pinned toolchain in [rust-toolchain.toml](rust-toolchain.toml) and run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo run --locked -p agentciv-checks`, and `cargo llvm-cov --workspace --all-targets --locked --fail-under-lines 80`. Install `cargo-llvm-cov` at the version pinned in [CI](.github/workflows/ci.yml) if needed. CI runs the same checks. New implementations must add their own strict type, lint, test, coverage, and conformance gates.

## License

By contributing, you agree that your contributions are licensed under the project's [MIT License](LICENSE).
