# Contributing to AgentCiv

AgentCiv is at the design stage. Code, experiments, critique, and cross-disciplinary review are welcome. A proposal that shows an assumption is wrong can be as valuable as a feature.

Independent projects can contribute adapters, worlds, scenarios, methods, or links to maintained external implementations. See [Collaborating with other projects](docs/ECOSYSTEM.md) for the information needed to make these contributions usable without requiring anyone to move their project into this repository.

## Start here

Read the [vision](docs/VISION.md), [roadmap](ROADMAP.md), [architecture proposal](docs/ARCHITECTURE.md), and [welfare policy](docs/WELFARE.md). Check existing discussions and issues before proposing a large change.

The [repository guidance](AGENTS.md) records the current architecture boundaries and verified checks for code contributors, including automated contributors.

For a design proposal, describe the research question, the smallest useful change, expected observations, alternative explanations, welfare implications, and a way to reproduce results. Explain how the change preserves room for agents to question or revise the world's rules. For an experiment, include the available communication and persistence capabilities, seeds when applicable, model and world versions, budgets, prompts or policies that can be shared, and an analysis method.

## Working agreements

- Describe observed behavior precisely. Do not label an agent conscious, suffering, compassionate, or sentient solely from a transcript or score. Do not dismiss the possibility of digital minds in order to sound careful.
- Keep operator interventions and changes to world rules auditable.
- Document limitations and negative findings.
- Respect privacy and licenses when publishing model outputs, datasets, or run artifacts.
- Use clear, respectful language in discussion, especially when disagreeing about consciousness or moral status.
- Give criticism priority in proportion to how much it helps the project build, measure, understand, or avoid causing harm. A persistence path that drops the state it names, a confounded comparison, a fork that moves authority, or a result that was prompted belongs in that work. A demand to prove consciousness before building, with no threshold evidence could meet, does not.

## Repository checks

The initial executable code is a Rust checker for documentation and schema fixtures. Use the pinned toolchain in [rust-toolchain.toml](rust-toolchain.toml) and run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo run --locked -p agentciv-checks`, and `cargo llvm-cov --workspace --all-targets --locked --fail-under-lines 80`. Install `cargo-llvm-cov` at the version pinned in [CI](.github/workflows/ci.yml) if needed. CI runs the same checks. New implementations must add their own strict type, lint, test, coverage, and conformance gates.

## License

By contributing, you agree that your contributions are licensed under the project's [MIT License](LICENSE).
