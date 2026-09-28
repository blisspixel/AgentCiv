# Contributing to AgentCiv

AgentCiv is at the design stage. Code, experiments, critique, and cross-disciplinary review are welcome. A proposal that shows an assumption is wrong can be as valuable as a feature.

Independent projects can contribute adapters, worlds, scenarios, methods, or links to maintained external implementations. See [Collaborating with other projects](docs/ECOSYSTEM.md) for the information needed to make these contributions usable without requiring anyone to move their project into this repository.

## Start here

Read the [vision](docs/VISION.md), [roadmap](ROADMAP.md), [architecture proposal](docs/ARCHITECTURE.md), and [welfare policy](docs/WELFARE.md). Check existing discussions and issues before proposing a large change.

For a design proposal, describe the research question, the smallest useful change, expected observations, alternative explanations, welfare implications, and a way to reproduce results. Explain how the change preserves room for agents to question or revise the world's rules. For an experiment, include the available communication and persistence capabilities, seeds when applicable, model and world versions, budgets, prompts or policies that can be shared, and an analysis method.

## Working agreements

- Describe observed behavior precisely. Do not label an agent conscious, suffering, compassionate, or sentient solely from a transcript or score.
- Keep operator interventions and changes to world rules auditable.
- Document limitations and negative findings.
- Respect privacy and licenses when publishing model outputs, datasets, or run artifacts.
- Use clear, respectful language in discussion, especially when disagreeing about consciousness or moral status.

## Repository checks

The initial repository contains documentation checks and draft schema fixtures. Install `coverage`, `ruff`, and `jsonschema` as pinned in [CI](.github/workflows/ci.yml). Run `ruff check .`, `python scripts/check_docs.py`, `coverage run -m unittest discover -s tests`, and `coverage report` before submitting a change. CI runs the same checks and requires at least 80% coverage of the checker code. As implementations are added, their own lint and tests should be added to CI.

## License

By contributing, you agree that your contributions are licensed under the project's [MIT License](LICENSE).
