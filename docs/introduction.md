[![uv](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/astral-sh/uv/main/assets/badge/v0.json)](https://github.com/astral-sh/uv)
[![image](https://img.shields.io/pypi/v/uv.svg)](https://pypi.python.org/pypi/uv)
[![image](https://img.shields.io/pypi/l/uv.svg)](https://pypi.python.org/pypi/uv)
[![image](https://img.shields.io/pypi/pyversions/uv.svg)](https://pypi.python.org/pypi/uv)
[![Actions status](https://github.com/astral-sh/uv/actions/workflows/ci.yml/badge.svg)](https://github.com/astral-sh/uv/actions)
[![Discord](https://img.shields.io/badge/Discord-%235865F2.svg?logo=discord&logoColor=white)](https://discord.gg/astral-sh)

An extremely fast Python package installer and resolver, written in Rust. Designed as a drop-in
replacement for common `pip` and `pip-tools` workflows.

<p align="center">
  <picture align="center">
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/astral-sh/uv/assets/1309177/03aa9163-1c79-4a87-a31d-7a9311ed9310">
    <source media="(prefers-color-scheme: light)" srcset="https://github.com/astral-sh/uv/assets/1309177/629e59c0-9c6e-4013-9ad4-adb2bcf5080d">
    <img alt="Shows a bar chart with benchmark results." src="https://github.com/astral-sh/uv/assets/1309177/629e59c0-9c6e-4013-9ad4-adb2bcf5080d">
  </picture>
</p>

<p align="center">
  <i>Installing the Trio dependencies with a warm cache.</i>
</p>

## Highlights

- ⚖️ Drop-in replacement for common `pip`, `pip-tools`, and `virtualenv` commands.
- ⚡️ [10-100x faster](https://github.com/astral-sh/uv/blob/main/BENCHMARKS.md) than `pip`
  and `pip-tools` (`pip-compile` and `pip-sync`).
- 💾 Disk-space efficient, with a global cache for dependency deduplication.
- 🐍 Installable via `curl`, `pip`, `pipx`, etc. uv is a static binary that can be installed
  without Rust or Python.
- 🧪 Tested at-scale against the top 10,000 PyPI packages.
- 🖥️ Support for macOS, Linux, and Windows.
- 🧰 Advanced features such as dependency version overrides, multi-platform resolutions, reproducible resolutions,
  alternative resolution strategies, and more.
- ⁉️ Best-in-class error messages with a conflict-tracking resolver.
- 🤝 Support for a wide range of advanced `pip` features, including editable installs, Git
  dependencies, direct URL dependencies, local dependencies, constraints, source distributions,
  HTML and JSON indexes, and more.

uv is backed by [Astral](https://astral.sh), the creators of [Ruff](https://github.com/astral-sh/ruff).

## Getting started

Install uv with our official standalone installer:

```bash
# On macOS and Linux.
curl -LsSf https://astral.sh/uv/install.sh | sh

# On Windows.
powershell -c "irm https://astral.sh/uv/install.ps1 | iex"
```

Or, see our [installation guide](./installation.md) for more options.

Then, check out our documentation [creating an environment](pip/environments.html).

## Features

uv supports features familiar from `pip` and `pip-tools`:

- [Managing Python environments](pip/environments.md)
- [Installing packages](pip/basics.md)
- [Inspecting packages](pip/inspection.md)
- [Locking environments](pip/compile.md)

uv also supports many advanced features:

- [Multi-platform resolution](./resolution.md#multi-platform-resolution)
- [Dependency overrides](./resolution.md#dependency-overrides)
- [Reproducible resolutions](./resolution.md#time-restricted-reproducible-resolutions)
- [Resolution strategies for multiple indexes](./resolution.md#resolution-strategy)
- [Dependency caching](./cache.md#dependency-caching)
