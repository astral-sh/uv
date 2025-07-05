# uv

[![uv](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/astral-sh/uv/main/assets/badge/v0.json)](https://github.com/astral-sh/uv)
[![image](https://img.shields.io/pypi/v/uv.svg)](https://pypi.python.org/pypi/uv)
[![image](https://img.shields.io/pypi/l/uv.svg)](https://pypi.python.org/pypi/uv)
[![image](https://img.shields.io/pypi/pyversions/uv.svg)](https://pypi.python.org/pypi/uv)
[![Actions status](https://github.com/astral-sh/uv/actions/workflows/ci.yml/badge.svg)](https://github.com/astral-sh/uv/actions)
[![Discord](https://img.shields.io/badge/Discord-%235865F2.svg?logo=discord&logoColor=white)](https://discord.gg/astral-sh)

一个用 Rust 编写的极其快速的 Python 包和项目管理器。

<p align="center">
  <picture align="center">
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/astral-sh/uv/assets/1309177/03aa9163-1c79-4a87-a31d-7a9311ed9310">
    <source media="(prefers-color-scheme: light)" srcset="https://github.com/astral-sh/uv/assets/1309177/629e59c0-9c6e-4013-9ad4-adb2bcf5080d">
    <img alt="显示基准测试结果的柱状图。" src="https://github.com/astral-sh/uv/assets/1309177/629e59c0-9c6e-4013-9ad4-adb2bcf5080d">
  </picture>
</p>

<p align="center">
  <i>使用热缓存安装 <a href="https://trio.readthedocs.io/">Trio</a> 的依赖项。</i>
</p>

## 亮点

- 🚀 一个工具替代 `pip`、`pip-tools`、`pipx`、`poetry`、`pyenv`、`twine`、`virtualenv` 等等。
- ⚡️ 比 `pip` [快 10-100 倍](https://github.com/astral-sh/uv/blob/main/BENCHMARKS.md)。
- 🗂️ 提供[全面的项目管理](#项目)，配备[通用锁文件](https://docs.astral.sh/uv/concepts/projects/layout#the-lockfile)。
- ❇️ [运行脚本](#脚本)，支持[内联依赖元数据](https://docs.astral.sh/uv/guides/scripts#declaring-script-dependencies)。
- 🐍 [安装和管理](#python-版本) Python 版本。
- 🛠️ [运行和安装](#工具)作为 Python 包发布的工具。
- 🔩 包含[兼容 pip 的接口](#pip-接口)，在熟悉的 CLI 中提供性能提升。
- 🏢 支持 Cargo 风格的[工作空间](https://docs.astral.sh/uv/concepts/projects/workspaces)，适用于可扩展项目。
- 💾 磁盘空间高效，具有用于依赖去重的[全局缓存](https://docs.astral.sh/uv/concepts/cache)。
- ⏬ 无需 Rust 或 Python 即可通过 `curl` 或 `pip` 安装。
- 🖥️ 支持 macOS、Linux 和 Windows。

uv 由 [Astral](https://astral.sh) 支持，[Ruff](https://github.com/astral-sh/ruff) 的创造者。

## 安装

使用我们的独立安装程序安装 uv：

```bash
# 在 macOS 和 Linux 上。
curl -LsSf https://astral.sh/uv/install.sh | sh
```

```bash
# 在 Windows 上。
powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"
```

或者，从 [PyPI](https://pypi.org/project/uv/) 安装：

```bash
# 使用 pip。
pip install uv
```

```bash
# 或者 pipx。
pipx install uv
```

如果通过独立安装程序安装，uv 可以将自己更新到最新版本：

```bash
uv self update
```

有关详细信息和其他安装方法，请参阅[安装文档](https://docs.astral.sh/uv/getting-started/installation/)。

## 文档

uv 的文档可在 [docs.astral.sh/uv](https://docs.astral.sh/uv) 获取。

此外，可以使用 `uv help` 查看命令行参考文档。

## 功能

### 项目

uv 管理项目依赖和环境，支持锁文件、工作空间等，类似于 `rye` 或 `poetry`：

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

请参阅[项目文档](https://docs.astral.sh/uv/guides/projects/)开始使用。

uv 还支持构建和发布项目，即使它们不是用 uv 管理的。请参阅[发布指南](https://docs.astral.sh/uv/guides/publish/)了解更多信息。

### 脚本

uv 管理单文件脚本的依赖和环境。

创建一个新脚本并添加声明其依赖的内联元数据：

```console
$ echo 'import requests; print(requests.get("https://astral.sh"))' > example.py

$ uv add --script example.py requests
Updated `example.py`
```

然后，在隔离的虚拟环境中运行脚本：

```console
$ uv run example.py
Reading inline script metadata from: example.py
Installed 5 packages in 12ms
<Response [200]>
```

请参阅[脚本文档](https://docs.astral.sh/uv/guides/scripts/)开始使用。

### 工具

uv 执行和安装由 Python 包提供的命令行工具，类似于 `pipx`。

使用 `uvx`（`uv tool run` 的别名）在临时环境中运行工具：

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

使用 `uv tool install` 安装工具：

```console
$ uv tool install ruff
Resolved 1 package in 6ms
Installed 1 package in 2ms
 + ruff==0.5.0
Installed 1 executable: ruff

$ ruff --version
ruff 0.5.0
```

请参阅[工具文档](https://docs.astral.sh/uv/guides/tools/)开始使用。

### Python 版本

uv 安装 Python 并允许快速切换版本。

安装多个 Python 版本：

```console
$ uv python install 3.10 3.11 3.12
Searching for Python versions matching: Python 3.10
Searching for Python versions matching: Python 3.11
Searching for Python versions matching: Python 3.12
Installed 3 versions in 3.42s
 + cpython-3.10.14-macos-aarch64-none
 + cpython-3.11.9-macos-aarch64-none
 + cpython-3.12.4-macos-aarch64-none
```

根据需要下载 Python 版本：

```console
$ uv venv --python 3.12.0
Using Python 3.12.0
Creating virtual environment at: .venv
Activate with: source .venv/bin/activate

$ uv run --python pypy@3.8 -- python --version
Python 3.8.16 (a9dbdca6fc3286b0addd2240f11d97d8e8de187a, Dec 29 2022, 11:45:30)
[PyPy 7.3.11 with GCC Apple LLVM 13.1.6 (clang-1316.0.21.2.5)] on darwin
Type "help", "copyright", "credits" or "license" for more information.
>>>>
```

在当前目录中使用特定的 Python 版本：

```console
$ uv python pin 3.11
Pinned `.python-version` to `3.11`
```

请参阅 [Python 安装文档](https://docs.astral.sh/uv/guides/install-python/)开始使用。

### pip 接口

uv 为常见的 `pip`、`pip-tools` 和 `virtualenv` 命令提供直接替换。

uv 通过高级功能扩展了它们的接口，如依赖版本覆盖、平台无关解析、可重现解析、替代解析策略等。

迁移到 uv 而无需更改现有工作流程——并体验 10-100 倍的速度提升——使用 `uv pip` 接口。

将需求编译为平台无关的需求文件：

```console
$ uv pip compile docs/requirements.in \
   --universal \
   --output-file docs/requirements.txt
Resolved 43 packages in 12ms
```

创建虚拟环境：

```console
$ uv venv
Using Python 3.12.3
Creating virtual environment at: .venv
Activate with: source .venv/bin/activate
```

安装锁定的需求：

```console
$ uv pip sync docs/requirements.txt
Resolved 43 packages in 11ms
Installed 43 packages in 208ms
 + babel==2.15.0
 + black==24.4.2
 + certifi==2024.7.4
 ...
```

请参阅 [pip 接口文档](https://docs.astral.sh/uv/pip/index/)开始使用。

## 平台支持

请参阅 uv 的[平台支持](https://docs.astral.sh/uv/reference/platforms/)文档。

## 版本策略

请参阅 uv 的[版本策略](https://docs.astral.sh/uv/reference/versioning/)文档。

## 贡献

我们热衷于支持各个经验水平的贡献者，并希望看到您参与项目。请参阅[贡献指南](https://github.com/astral-sh/uv/blob/main/CONTRIBUTING.md)开始使用。

## 常见问题

#### 如何发音 uv？

发音为 "you - vee"（[`/juː viː/`](https://en.wikipedia.org/wiki/Help:IPA/English#Key)）

#### 应该如何书写 uv？

请使用 "uv"。有关详细信息，请参阅[样式指南](./STYLE.md#styling-uv)。

## 致谢

uv 的依赖解析器在底层使用 [PubGrub](https://github.com/pubgrub-rs/pubgrub)。我们感谢 PubGrub 维护者，特别是 [Jacob Finkelman](https://github.com/Eh2406) 的支持。

uv 的 Git 实现基于 [Cargo](https://github.com/rust-lang/cargo)。

uv 的一些优化受到我们在 [pnpm](https://pnpm.io/)、[Orogene](https://github.com/orogene/orogene) 和 [Bun](https://github.com/oven-sh/bun) 中看到的出色工作的启发。我们还从 Nathaniel J. Smith 的 [Posy](https://github.com/njsmith/posy) 中学到了很多，并为 Windows 支持改编了其 [trampoline](https://github.com/njsmith/posy/tree/main/src/trampolines/windows-trampolines/posy-trampoline)。

## 许可证

uv 采用以下任一许可证：

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) 或
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) 或 <https://opensource.org/licenses/MIT>)

由您选择。

除非您明确声明，否则您有意提交给 uv 的任何贡献，如 Apache-2.0 许可证中定义的，应按上述方式双重许可，不附加任何额外条款或条件。

<div align="center">
  <a target="_blank" href="https://astral.sh" style="background:none">
    <img src="https://raw.githubusercontent.com/astral-sh/uv/main/assets/svg/Astral.svg" alt="Made by Astral">
  </a>
</div>
