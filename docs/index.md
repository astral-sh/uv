
<div align="center">
  <a href="https://github.com/astral-sh/uv"><img src="https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/astral-sh/uv/main/assets/badge/v0.json" /></a>
  <a href="https://pypi.python.org/pypi/uv"><img src="https://img.shields.io/pypi/v/uv.svg" /></a>
  <a href="https://pypi.python.org/pypi/uv"><img src="https://img.shields.io/pypi/l/uv.svg" /></a>
  <a href="https://discord.gg/astral-sh"><img src="https://img.shields.io/badge/Discord-%235865F2.svg?logo=discord&logoColor=white" /></a>
</div>

An extremely fast Python version, package, and project manager, written in Rust.

<p align="center">
  <img alt="Shows a bar chart with benchmark results." src="https://github.com/astral-sh/uv/assets/1309177/629e59c0-9c6e-4013-9ad4-adb2bcf5080d#only-light">
</p>

<p align="center">
  <img alt="Shows a bar chart with benchmark results." src="https://github.com/astral-sh/uv/assets/1309177/03aa9163-1c79-4a87-a31d-7a9311ed9310#only-dark">
</p>

<p align="center">
  <i>Installing the Trio dependencies with a warm cache.</i>
</p>

## Highlights

- 🐍 [Installs and manages](./guides/install-python.md) Python versions.
- 🛠️ [Executes and installs](./guides/tools.md) commands provided by Python packages.
- ❇️ [Runs scripts](./guides/scripts.md) with inline dependency metadata.
- 🗂️ Provides comprehensive project management, with a multi-platform lock file.
- 🏢 Supports Cargo-style workspaces for large projects.
- 🚀 A replacement for `pip`, `pip-tools`, `pipx`, `poetry`, `pyenv`, and more.
- ⚡️ [10-100x faster](https://github.com/astral-sh/uv/blob/main/BENCHMARKS.md) than `pip`
  and `pip-tools` (`pip-compile` and `pip-sync`).
- 🧪 Tested at-scale against the top 10,000 PyPI packages.
- 💾 Disk-space efficient, with a global cache for dependency deduplication.
- ⁉️ Best-in-class error messages with a conflict-tracking resolver.
- ⏬ A static binary that can be installed without Rust or Python via `curl` or `pip`.
- 🖥️ Support for macOS, Linux, and Windows.

uv is backed by [Astral](https://astral.sh), the creators of [Ruff](https://github.com/astral-sh/ruff).

## Replacement for `pip` and `pip-tools`

uv provides a drop-in replacement for common `pip`, `pip-tools`, and `virtualenv` commands with support for
a wide range of advanced `pip` features, including editable installs, Git dependencies, direct URL dependencies, local dependencies, constraints, source distributions, HTML and JSON indexes, and more.

uv extends these interfaces with advanced features, such as dependency version overrides, multi-platform resolutions, reproducible resolutions, alternative resolution strategies, and more.

## Getting started

Install uv with our official standalone installer:

```bash
# On macOS and Linux.
curl -LsSf https://astral.sh/uv/install.sh | sh

# On Windows.
powershell -c "irm https://astral.sh/uv/install.ps1 | iex"
```

Then, check out the [first steps](./first-steps.md) or see more [installation methods](./installation.md).
