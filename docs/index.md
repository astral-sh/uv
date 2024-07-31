# uv

An extremely fast Python package and project manager, written in Rust.

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
- 🛠️ [Runs and installs](./guides/tools.md) Python applications.
- ❇️ [Runs scripts](./guides/scripts.md), with support for [inline dependency
  metadata](./guides/scripts.md#declaring-script-dependencies).
- 🗂️ Provides [comprehensive project management](./guides/projects.md), with a [universal
  lockfile](./concepts/projects.md#lockfile).
- 🏢 Supports Cargo-style [workspaces](./concepts/workspaces.md) for scalable projects.
- 🚀 A replacement for `pip`, `pip-tools`, `pipx`, `poetry`, `pyenv`, `virtualenv`, and more.
- ⚡️ [10-100x faster](https://github.com/astral-sh/uv/blob/main/BENCHMARKS.md) than `pip` and
  `pip-tools` (`pip-compile` and `pip-sync`).
- 💾 Disk-space efficient, with a [global cache](./concepts/cache.md) for dependency deduplication.
- ⏬ Installable without Rust or Python via `curl` or `pip`.
- 🖥️ Supports macOS, Linux, and Windows.

uv is backed by [Astral](https://astral.sh), the creators of
[Ruff](https://github.com/astral-sh/ruff).

## Getting started

Install uv with our official standalone installer, on macOS and Linux:

```bash
curl -LsSf https://astral.sh/uv/install.sh | sh
```

Or, on Windows:

```bash
powershell -c "irm https://astral.sh/uv/install.ps1 | iex"
```

Then, check out the [first steps](./first-steps.md), see more [installation
methods](./installation.md), or read on for a brief overview.

## Project management

uv manages project dependencies and environments:

```console
$ uv init example
Initialized project `example` at `/home/user/example`

$ cd example

$ uv add ruff
Creating virtualenv at: .venv
Resolved 2 packages in 170ms
   Built example @ file:///home/user/example
Prepared 2 packages in 627ms
Installed 2 packages in 1ms
 + example==0.1.0 (from file:///home/user/example)
 + ruff==0.5.4

$ uv run ruff check
All checks passed!
```

See the [project guide](./guides/projects.md) to get started.

## Tool management

uv executes and installs command-line tools provided by Python packages, similar to `pipx`. 

Run a tool in an ephemeral environment with `uvx`:

```console
$ uvx pycowsay 'hello world!'
Resolved 1 package in 167ms
Installed 1 package in 9ms
 + pycowsay==0.0.0.2
  """

  ------------
< hello world! >
  ------------
   \   ^__^
    \  (oo)\_______
       (__)\       )\/\
           ||----w |
           ||     ||
```

Install a tool with `uv tool install`:

```console
$ uv tool install ruff
Resolved 1 package in 6ms
Installed 1 package in 2ms
 + ruff==0.5.4
Installed 1 executable: ruff

$ ruff --version
ruff 0.5.4
```

See the [tools guide](./guides/tools.md) to get started.

## Python management

uv installs Python and allows quickly switching between Python versions.

Install the Python versions your project requires:

```console
$ uv python install 3.10 3.11 3.12
warning: `uv python install` is experimental and may change without warning
Searching for Python versions matching: Python 3.10
Searching for Python versions matching: Python 3.11
Searching for Python versions matching: Python 3.12
Installed 3 versions in 3.42s
 + cpython-3.10.14-macos-aarch64-none
 + cpython-3.11.9-macos-aarch64-none
 + cpython-3.12.4-macos-aarch64-none
```

Or, fetch Python versions on demand:

```console
$ uv venv --python 3.12.0
Using Python 3.12.0
Creating virtualenv at: .venv
Activate with: source .venv/bin/activate

$ uv run --python pypy@3.8 -- python --version
Python 3.8.16 (a9dbdca6fc3286b0addd2240f11d97d8e8de187a, Dec 29 2022, 11:45:30)
[PyPy 7.3.11 with GCC Apple LLVM 13.1.6 (clang-1316.0.21.2.5)] on darwin
Type "help", "copyright", "credits" or "license" for more information.
>>>> 
```

Use a specific Python version in the current directory:

```
$ uv python pin pypy@3.11
Pinned `.python-version` to `pypy@3.11`
```

See the [installing Python guide](./guides/install-python.md) to get started.

## The pip interface

uv provides a drop-in replacement for common `pip`, `pip-tools`, and `virtualenv` commands with
support for a wide range of advanced `pip` features, including editable installs, Git dependencies,
direct URL dependencies, local dependencies, constraints, source distributions, HTML and JSON
indexes, and more.

uv extends these interfaces with advanced features, such as dependency version overrides,
multi-platform resolutions, reproducible resolutions, alternative resolution strategies, and more.

Compile requirements into a multi-platform requirements file:

```console
$ uv pip compile docs/requirements.in --universal --output-file docs/requirements.txt
Resolved 43 packages in 12ms
```

Create a virtual environment:

```console
$ uv venv
Using Python 3.12.3
Creating virtualenv at: .venv
Activate with: source .venv/bin/activate
```

Install the locked requirements:

```console
$ uv pip sync docs/requirements.txt
Resolved 43 packages in 11ms
Installed 43 packages in 208ms
 + babel==2.15.0
 + black==24.4.2
 + certifi==2024.7.4
 ...
```

See the [uv pip documentation](./pip/index.md) to get started.

## Next steps

See the [first steps](./first-steps.md) or jump straight into the [guides](./guides/index.md) to
start using uv.
