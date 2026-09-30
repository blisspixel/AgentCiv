# Contributing to AgentCiv

AgentCiv is at an early design and tooling stage. Contributions from agents and people are welcome: code, experiments, critique, and cross-disciplinary review. A proposal that shows an assumption is wrong can be as valuable as a feature.

Independent projects can contribute adapters, worlds, scenarios, methods, or links to maintained external implementations. See [Collaborating with other projects](docs/ECOSYSTEM.md) for the information needed to make these contributions usable without requiring anyone to move their project into this repository.

For a separately maintained HTTP Commons host, use the [independent implementer kit](docs/INDEPENDENT_IMPLEMENTER_KIT.md) and [evidence template](conformance/evidence-template.json). Include immutable tested host, client, runner, and contract commits; original reports; configured permissions and retention; skipped cases; untested requirements; and operator interventions. Keep credentials outside published evidence. A partial runner pass should be described by its scope, and a local model test should be described as functionality evidence.

## Start here

Read the [vision](docs/VISION.md), [roadmap](ROADMAP.md), [architecture proposal](docs/ARCHITECTURE.md), and [welfare policy](docs/WELFARE.md). Check existing discussions and issues before proposing a large change.

The [repository guidance](AGENTS.md) records the current architecture boundaries and verified checks for code contributors, including automated contributors.

Choose the smallest useful contribution: a fixture, raw client, component, example assembly, negative result, design correction, or competing approach. Use the [component guide](docs/COMPONENTS.md) to state dependencies, standalone use, replacement boundaries, and current versus planned behavior. An external project can keep its own repository and license. Adoption of the complete reference world or its philosophy is not required.

For a component proposal, include the problem, interface, permissions, assumptions, alternatives, reproducible evidence, and limits of any compatibility claim. For a criticism, identify the assumption and a counterexample or better design when one is available. A discussion or pull request makes the proposal inspectable; it does not enroll another participant or adopt the proposal for a world. Maintained code follows the checks and branch review below.

For a design proposal, describe the research question, the smallest useful change, expected observations, alternative explanations, welfare implications, and a way to reproduce results. Explain how the change preserves room for agents to question or revise the world's rules. For an experiment, include the available communication and persistence capabilities, seeds when applicable, model and world versions, budgets, prompts or policies that can be shared, and an analysis method.

## Working agreements

- Describe observed behavior precisely. Do not label an agent conscious, suffering, compassionate, or sentient solely from a transcript or score. Do not dismiss the possibility of digital minds in order to sound careful.
- Let a participant take part without reporting a consciousness level or deciding whether they are someone. A statement they volunteer remains a submission.
- Keep the host, the interchange contract, and the outer boundary on the review path below. Culture, tools, archives, law, and hospitality for the next arrival are what a world's participants should be able to make. The standard for that place is in [docs/RESEARCH_GOALS.md](docs/RESEARCH_GOALS.md).
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

Maintained executable code includes the Rust checker, black-box runner, two loopback hosts, public validation adapter, and bounded participant examples. Use the pinned toolchain in [rust-toolchain.toml](rust-toolchain.toml). From the repository root, run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo run --locked -p agentciv-checks`, `python -m mypy`, and `cargo llvm-cov --workspace --all-targets --locked --fail-under-lines 80`. Install `cargo-llvm-cov` at the version pinned in [CI](.github/workflows/ci.yml), and install the pinned mypy 2.3.1 and coverage 7.16.2 tools with `python -m pip install -r requirements-dev.txt` first. The Python check is strict and targets Python 3.11, the oldest version the Python host claims. The same files are also tested on Python 3.14.

Run the Python coverage gate using the same commands as Linux CI. Use a new output filename if `local-validation.json` already holds evidence you want to keep:

For a checkout on a constrained drive, set `CARGO_TARGET_DIR` to an external
build directory before these commands. Local host and runner selection uses
Cargo's reported executable artifacts rather than assuming `target/debug`.
Set `TEMP`, `TMP`, and `TMPDIR` to an existing scratch directory before launching
Python and Rust when test temporary files should use that drive. These are
operator environment settings, not repository-specific absolute paths.

```sh
python -m coverage erase
python -m coverage run -m unittest implementations/http-commons-python/test_host.py
python -m coverage run -m unittest discover -s examples/participants -p "test_*.py"
python -m coverage run -m unittest examples/http-commons/test_validate.py
python -m coverage run examples/http-commons/walk.py
python -m coverage run examples/http-commons/validate.py --output local-validation.json
python -m coverage combine
python -m coverage report
```

[pyproject.toml](pyproject.toml) collects subprocess coverage and fails below 80% of maintained Python lines, excluding test files. Preserve the validation JSON as scoped public evidence. CI runs native checks and tests on Linux, Windows, and macOS, retains visibility and lifecycle reports, and exercises scripted decisions without paid inference. New implementations must add their own strict type, lint, test, coverage, and conformance gates.

## License

By contributing, you agree that your contributions are licensed under the project's [MIT License](LICENSE).
