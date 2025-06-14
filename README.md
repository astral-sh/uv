# uv ⚡

<div align="center">

![uv Logo](https://img.shields.io/badge/⚡-UV-orange?style=for-the-badge&labelColor=black&color=orange)

**An extremely fast Python package and project manager, written in Rust.**

[![uv](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/astral-sh/uv/main/assets/badge/v0.json&style=for-the-badge)](https://github.com/astral-sh/uv)
[![PyPI version](https://img.shields.io/pypi/v/uv.svg?style=for-the-badge&color=blue)](https://pypi.python.org/pypi/uv)
[![License](https://img.shields.io/pypi/l/uv.svg?style=for-the-badge&color=green)](https://pypi.python.org/pypi/uv)
[![Python versions](https://img.shields.io/pypi/pyversions/uv.svg?style=for-the-badge&color=yellow)](https://pypi.python.org/pypi/uv)

[![CI Status](https://img.shields.io/github/actions/workflow/status/astral-sh/uv/ci.yml?style=for-the-badge&logo=github)](https://github.com/astral-sh/uv/actions)
[![Discord](https://img.shields.io/discord/1073807493058719994?style=for-the-badge&logo=discord&logoColor=white&color=5865F2)](https://discord.gg/astral-sh)
[![Downloads](https://img.shields.io/pypi/dm/uv?style=for-the-badge&color=brightgreen)](https://pypi.org/project/uv/)

![Rust](https://img.shields.io/badge/Built_with-Rust-orange?style=for-the-badge&logo=rust&logoColor=white)
![Speed](https://img.shields.io/badge/Speed-10--100x_Faster-red?style=for-the-badge&logo=rocket)
![Cross Platform](https://img.shields.io/badge/Platform-macOS%20|%20Linux%20|%20Windows-blue?style=for-the-badge&logo=windows)

</div>

---

## ⚡ Performance Benchmark

<div align="center">

<picture align="center">
  <source media="(prefers-color-scheme: dark)" srcset="https://github.com/astral-sh/uv/assets/1309177/03aa9163-1c79-4a87-a31d-7a9311ed9310">
  <source media="(prefers-color-scheme: light)" srcset="https://github.com/astral-sh/uv/assets/1309177/629e59c0-9c6e-4013-9ad4-adb2bcf5080d">
  <img alt="Shows a bar chart with benchmark results." src="https://github.com/astral-sh/uv/assets/1309177/629e59c0-9c6e-4013-9ad4-adb2bcf5080d">
</picture>

*Installing [Trio](https://trio.readthedocs.io/)'s dependencies with a warm cache.*

![Benchmark](https://img.shields.io/badge/🏆-10--100x%20Faster%20than%20pip-gold?style=for-the-badge)

</div>

---

## ✨ Highlights

<div align="center">

| 🎯 Feature | 📝 Description |
|------------|----------------|
| **🚀 All-in-One Tool** | Replace `pip`, `pip-tools`, `pipx`, `poetry`, `pyenv`, `twine`, `virtualenv`, and more |
| **⚡ Lightning Fast** | [10-100x faster](https://github.com/astral-sh/uv/blob/main/BENCHMARKS.md) than `pip` |
| **🗂️ Project Management** | Comprehensive project management with [universal lockfile](https://docs.astral.sh/uv/concepts/projects/layout#the-lockfile) |
| **❇️ Script Execution** | Run scripts with [inline dependency metadata](https://docs.astral.sh/uv/guides/scripts#declaring-script-dependencies) |
| **🐍 Python Management** | Install and manage Python versions seamlessly |
| **🛠️ Tool Management** | Run and install tools published as Python packages |
| **🔩 pip Compatible** | Drop-in replacement with familiar CLI interface |
| **🏢 Workspace Support** | Cargo-style [workspaces](https://docs.astral.sh/uv/concepts/projects/workspaces) for scalable projects |
| **💾 Efficient Storage** | Global cache for dependency deduplication |
| **⏬ Easy Installation** | Installable without Rust or Python via `curl` or `pip` |

</div>

<div align="center">

![Backed by Astral](https://img.shields.io/badge/🌟-Backed%20by%20Astral-purple?style=for-the-badge)

*uv is backed by [Astral](https://astral.sh), the creators of [Ruff](https://github.com/astral-sh/ruff).*

</div>

---

## 🚀 Quick Installation

<div align="center">

![Installation](https://img.shields.io/badge/⚡-Install%20in%20Seconds-brightgreen?style=for-the-badge)

</div>

### 🔥 Standalone Installers (Recommended)

<div align="center">

| 🖥️ Platform | 📋 Command |
|-------------|-------------|
| **🍎 macOS & 🐧 Linux** | `curl -LsSf https://astral.sh/uv/install.sh \| sh` |
| **🪟 Windows** | `powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 \| iex"` |

</div>

### 📦 Package Managers

```bash
# With pip
pip install uv

# With pipx
pipx install uv

# With homebrew (macOS)
brew install uv

# With scoop (Windows)
scoop install uv
```

### 🔄 Self-Update

```bash
uv self update
```

<div align="center">

[![Installation Guide](https://img.shields.io/badge/📖-Full%20Installation%20Guide-blue?style=for-the-badge)](https://docs.astral.sh/uv/getting-started/installation/)

</div>

---

## 📚 Documentation

<div align="center">

[![Documentation](https://img.shields.io/badge/📖-Read%20the%20Docs-blue?style=for-the-badge&logo=readthedocs)](https://docs.astral.sh/uv)
[![CLI Reference](https://img.shields.io/badge/💻-CLI%20Reference-green?style=for-the-badge&logo=terminal)](https://docs.astral.sh/uv/reference/cli/)

</div>

Access comprehensive documentation at **[docs.astral.sh/uv](https://docs.astral.sh/uv)**

For quick help, use: `uv help`

---

## 🎯 Core Features

### 📦 Projects

<div align="center">

![Projects](https://img.shields.io/badge/🏗️-Project%20Management-blue?style=for-the-badge)

</div>

Manage project dependencies and environments with lockfiles and workspaces, similar to `rye` or `poetry`:

```console
$ uv init example
Initialized project `example` at `/home/user/example`

$ cd example

$ uv add ruff
Creating virtual environment at: .venv
Resolved 2 packages in 170ms
   Built example @ file:///home/user/example
Prepared 2 packages in 627ms
Installed 2 packages in 1ms
 + example==0.1.0 (from file:///home/user/example)
 + ruff==0.5.0

$ uv run ruff check
All checks passed!

$ uv lock
Resolved 2 packages in 0.33ms

$ uv sync
Resolved 2 packages in 0.70ms
Audited 1 package in 0.02ms
```

<div align="center">

[![Project Guide](https://img.shields.io/badge/📖-Project%20Guide-blue?style=flat-square)](https://docs.astral.sh/uv/guides/projects/)
[![Publish Guide](https://img.shields.io/badge/📤-Publishing%20Guide-green?style=flat-square)](https://docs.astral.sh/uv/guides/publish/)

</div>

### 📜 Scripts

<div align="center">

![Scripts](https://img.shields.io/badge/🎬-Script%20Management-orange?style=for-the-badge)

</div>

Manage dependencies and environments for single-file scripts:

```console
$ echo 'import requests; print(requests.get("https://astral.sh"))' > example.py

$ uv add --script example.py requests
Updated `example.py`

$ uv run example.py
Reading inline script metadata from: example.py
Installed 5 packages in 12ms
<Response [200]>
```

<div align="center">

[![Scripts Guide](https://img.shields.io/badge/📖-Scripts%20Guide-orange?style=flat-square)](https://docs.astral.sh/uv/guides/scripts/)

</div>

### 🛠️ Tools

<div align="center">

![Tools](https://img.shields.io/badge/⚒️-Tool%20Management-purple?style=for-the-badge)

</div>

Execute and install command-line tools, similar to `pipx`:

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

$ uv tool install ruff
Resolved 1 package in 6ms
Installed 1 package in 2ms
 + ruff==0.5.0
Installed 1 executable: ruff

$ ruff --version
ruff 0.5.0
```

<div align="center">

[![Tools Guide](https://img.shields.io/badge/📖-Tools%20Guide-purple?style=flat-square)](https://docs.astral.sh/uv/guides/tools/)

</div>

### 🐍 Python Versions

<div align="center">

![Python](https://img.shields.io/badge/🐍-Python%20Management-yellow?style=for-the-badge)

</div>

Install Python and switch between versions effortlessly:

```console
$ uv python install 3.10 3.11 3.12
Searching for Python versions matching: Python 3.10
Searching for Python versions matching: Python 3.11
Searching for Python versions matching: Python 3.12
Installed 3 versions in 3.42s
 + cpython-3.10.14-macos-aarch64-none
 + cpython-3.11.9-macos-aarch64-none
 + cpython-3.12.4-macos-aarch64-none

$ uv venv --python 3.12.0
Using Python 3.12.0
Creating virtual environment at: .venv
Activate with: source .venv/bin/activate

$ uv python pin 3.11
Pinned `.python-version` to `3.11`
```

<div align="center">

[![Python Guide](https://img.shields.io/badge/📖-Python%20Installation%20Guide-yellow?style=flat-square)](https://docs.astral.sh/uv/guides/install-python/)

</div>

### 🔩 The pip Interface

<div align="center">

![pip Interface](https://img.shields.io/badge/🔄-pip%20Compatible-red?style=for-the-badge)

</div>

Drop-in replacement for `pip`, `pip-tools`, and `virtualenv` with 10-100x speedup:

```console
$ uv pip compile docs/requirements.in \
   --universal \
   --output-file docs/requirements.txt
Resolved 43 packages in 12ms

$ uv venv
Using Python 3.12.3
Creating virtual environment at: .venv
Activate with: source .venv/bin/activate

$ uv pip sync docs/requirements.txt
Resolved 43 packages in 11ms
Installed 43 packages in 208ms
 + babel==2.15.0
 + black==24.4.2
 + certifi==2024.7.4
 ...
```

<div align="center">

[![pip Interface Guide](https://img.shields.io/badge/📖-pip%20Interface%20Guide-red?style=flat-square)](https://docs.astral.sh/uv/pip/index/)

</div>

---

## 🌐 Platform Support

<div align="center">

![Platform Support](https://img.shields.io/badge/🌍-Universal%20Platform%20Support-brightgreen?style=for-the-badge)

| 🖥️ Platform | ✅ Status | 🏗️ Architecture |
|-------------|-----------|------------------|
| **🍎 macOS** | ✅ Supported | Intel, Apple Silicon |
| **🐧 Linux** | ✅ Supported | x86_64, ARM64 |
| **🪟 Windows** | ✅ Supported | x86_64 |

[![Platform Details](https://img.shields.io/badge/📖-Platform%20Support%20Details-blue?style=flat-square)](https://docs.astral.sh/uv/reference/platforms/)

</div>

---

## 📈 Project Stats

<div align="center">

![GitHub Repo stars](https://img.shields.io/github/stars/astral-sh/uv?style=for-the-badge&color=gold)
![GitHub forks](https://img.shields.io/github/forks/astral-sh/uv?style=for-the-badge&color=blue)
![GitHub issues](https://img.shields.io/github/issues/astral-sh/uv?style=for-the-badge&color=red)
![GitHub pull requests](https://img.shields.io/github/issues-pr/astral-sh/uv?style=for-the-badge&color=green)

![GitHub repo size](https://img.shields.io/github/repo-size/astral-sh/uv?style=for-the-badge&color=purple)
![Lines of code](https://img.shields.io/tokei/lines/github/astral-sh/uv?style=for-the-badge&color=orange)
![GitHub last commit](https://img.shields.io/github/last-commit/astral-sh/uv?style=for-the-badge&color=brightgreen)

</div>

---

## 🤝 Contributing

<div align="center">

![Contributing](https://img.shields.io/badge/🤝-We%20Love%20Contributors-red?style=for-the-badge)

</div>

We are passionate about supporting contributors of all levels of experience and would love to see you get involved in the project.

<div align="center">

[![Contributing Guide](https://img.shields.io/badge/📖-Contributing%20Guide-blue?style=for-the-badge)](https://github.com/astral-sh/uv/blob/main/CONTRIBUTING.md)
[![Good First Issues](https://img.shields.io/github/issues/astral-sh/uv/good%20first%20issue?style=for-the-badge&color=green&label=Good%20First%20Issues)](https://github.com/astral-sh/uv/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)
[![Help Wanted](https://img.shields.io/github/issues/astral-sh/uv/help%20wanted?style=for-the-badge&color=purple&label=Help%20Wanted)](https://github.com/astral-sh/uv/issues?q=is%3Aissue+is%3Aopen+label%3A%22help+wanted%22)

</div>

---

## ❓ FAQ

<div align="center">

![FAQ](https://img.shields.io/badge/❓-Frequently%20Asked%20Questions-blue?style=for-the-badge)

</div>

### 🔤 How do you pronounce uv?

It's pronounced as **"you - vee"** ([`/juː viː/`](https://en.wikipedia.org/wiki/Help:IPA/English#Key))

### ✏️ How should I stylize uv?

Just **"uv"**, please. See the [style guide](./STYLE.md#styling-uv) for details.

---

## 🙏 Acknowledgements

<div align="center">

![Acknowledgements](https://img.shields.io/badge/🙏-Built%20on%20Shoulders%20of%20Giants-purple?style=for-the-badge)

</div>

uv's success is built upon the amazing work of many open source projects:

- **[PubGrub](https://github.com/pubgrub-rs/pubgrub)** - Dependency resolver (special thanks to [Jacob Finkelman](https://github.com/Eh2406))
- **[Cargo](https://github.com/rust-lang/cargo)** - Git implementation inspiration
- **[pnpm](https://pnpm.io/)**, **[Orogene](https://github.com/orogene/orogene)**, and **[Bun](https://github.com/oven-sh/bun)** - Optimization inspirations
- **[Posy](https://github.com/njsmith/posy)** by Nathaniel J. Smith - Trampoline adaptations

<div align="center">

[![Dependencies](https://img.shields.io/badge/🔗-View%20All%20Dependencies-blue?style=flat-square)](https://github.com/astral-sh/uv/blob/main/Cargo.toml)

</div>

---

## 📜 License

<div align="center">

![License](https://img.shields.io/badge/📜-Dual%20Licensed-green?style=for-the-badge)

</div>

uv is licensed under either of:

- **Apache License, Version 2.0** ([LICENSE-APACHE](LICENSE-APACHE) or [apache.org/licenses/LICENSE-2.0](https://www.apache.org/licenses/LICENSE-2.0))
- **MIT License** ([LICENSE-MIT](LICENSE-MIT) or [opensource.org/licenses/MIT](https://opensource.org/licenses/MIT))

at your option.

<div align="center">

[![Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue?style=flat-square)](https://www.apache.org/licenses/LICENSE-2.0)
[![MIT](https://img.shields.io/badge/License-MIT-yellow?style=flat-square)](https://opensource.org/licenses/MIT)

</div>

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in uv by you, as defined in the Apache-2.0 license, shall be dually licensed as above, without any additional terms or conditions.

---

## 🏢 Made by Astral

<div align="center">

<a href="https://astral.sh" target="_blank">
  <img src="https://raw.githubusercontent.com/astral-sh/uv/main/assets/svg/Astral.svg" alt="Made by Astral" width="200">
</a>

[![Website](https://img.shields.io/badge/🌐-astral.sh-blue?style=for-the-badge)](https://astral.sh)
[![Twitter](https://img.shields.io/badge/Twitter-1DA1F2?style=for-the-badge&logo=twitter&logoColor=white)](https://twitter.com/astral_sh)
[![GitHub](https://img.shields.io/badge/GitHub-100000?style=for-the-badge&logo=github&logoColor=white)](https://github.com/astral-sh)

**Also check out [Ruff](https://github.com/astral-sh/ruff) - An extremely fast Python linter and code formatter**

</div>

---

<div align="center">

### 💝 Show Your Support

If you find uv useful, please consider giving it a ⭐ on GitHub!

[![Star History Chart](https://api.star-history.com/svg?repos=astral-sh/uv&type=Date)](https://star-history.com/#astral-sh/uv&Date)

**Built with ❤️ by the Astral team and contributors**

---

*"The fastest way to manage Python projects and dependencies"*

</div>  
