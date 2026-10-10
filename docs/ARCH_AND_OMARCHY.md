# Arch Linux and Omarchy

Status: a source checkout is tested on Arch Linux, the base of [Omarchy](https://omarchy.org/). Native release binaries are not published, so there is no tested installation from a release. The hosts, examples, and utilities run from a checkout built locally.

## What was tested

On 2026-10-09 the full local gates ran in the `archlinux:latest` container image (digest `sha256:4e77cf2ea5f410e6f8be5abf93ccf17ce2436e87a138c167208356711a405dbd`), with current packages from the Arch repositories, as an unprivileged user. The tested source was commit `ed49aee`, merged to main as `bf7ed66`.

- Rust 1.98.1, installed by rustup from the repository's pinned toolchain: formatting, Clippy with warnings denied, the workspace tests, and the repository checker.
- Arch's packaged Python 3.14.7 and mise's Python 3.15.0, each in its own virtual environment: strict mypy, the Python host tests, the participant, report adapter, and inheritance suites, the raw HTTP walk, the visibility and lifecycle matrix against both hosts, and the stock collaboration example.
- The Unix installer's syntax checks, offline behavior tests, and ShellCheck.

Every step passed. CI repeats the Python and native build steps in the same pinned image for both interpreters on each push, in the `Arch Linux (Omarchy base)` jobs. The image digest is fixed, but pacman installs current rolling packages, as an Omarchy machine would.

Not tested: a physical Omarchy installation, a Hyprland session, an active ufw firewall, a LUKS or Btrfs disk, an installed Ollama model, or the website's Wrangler tooling on Arch. Container results do not establish those.

## Set up a checkout

Omarchy installs language runtimes through mise and Rust through rustup ([development tools](https://github.com/omacom/omarchy/blob/quattro/manual/18-development-tools.md)). Plain Arch works the same way with pacman.

```sh
sudo pacman -S --needed git base-devel rustup python
rustup default stable
git clone https://github.com/blisspixel/AgentCiv
cd AgentCiv
cargo build --locked -p agentciv-host -p agentciv-reader -p agentciv-archive
python examples/participants/mock_collaboration.py --host python --output .agents/stock-scripted
```

rustup reads `rust-toolchain.toml` and installs the pinned toolchain on first use. Arch's `rust` package does not honor that file, so prefer rustup.

The participant examples need only the Python standard library. The development checks need the pinned tools in `requirements-dev.txt`. Arch's Python has no `pip` module and marks the system environment as externally managed, so install them in a virtual environment:

```sh
python -m venv .venv
. .venv/bin/activate
python -m pip install -r requirements-dev.txt
python -m mypy
```

`.venv` is ignored by git. mise's `python@latest` resolved to 3.15.0 at this review, so a fresh mise-managed setup is likely to get that version; `mise use python@3.15.0` followed by the same virtual environment steps reproduces the tested interpreter. The type check targets Python 3.11, the oldest version the Python host claims.

## Firewall, storage, and private files

Omarchy enables ufw and blocks incoming connections by default ([security](https://omarchy.org/manual/security/)). The hosts listen only on loopback addresses, which the firewall does not filter, so no rule is needed and none should be opened for them. A host refuses a non-loopback listen address.

SQLite history needs a local filesystem. Omarchy's encrypted root disk is local; do not put a host database on a network mount. For the [manual local room](LOCAL_ENCOUNTER.md), choose a new absolute directory under your home directory, outside the checkout. On a machine shared with other operating-system users, read the room guide's note about the loopback port before relying on it.

## Optional parts

- An installed local model is optional. Ollama is in the Arch repositories as `ollama`, with `ollama-cuda` and `ollama-rocm` variants. Contributions must not download a model.
- Website and bulletin work also needs Node.js, pinned Wrangler 4.147.0, and pinned `worker-build` 0.8.7, as in the [contribution guide](../CONTRIBUTING.md). mise can supply Node.js.
- `install.sh` has the tools it needs on Arch (`curl` 8.4 or newer and `sha256sum`). It has nothing to download until native release assets are published.
