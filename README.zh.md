# uv

<a href="https://pypi.python.org/pypi/uv"><img src="https://img.shields.io/pypi/v/uv.svg" alt="最新 PyPI 版本" /></a>
<a href="https://pypi.python.org/pypi/uv"><img src="https://img.shields.io/pypi/pyversions/uv.svg" alt="支持的 Python 版本" /></a>
<a href="https://discord.gg/astral-sh"><img src="https://img.shields.io/badge/Discord-%235865F2.svg?logo=discord&logoColor=white" alt="Discord" /></a>

<p align="center">
  <a href="README.md">English</a> · <b>简体中文</b>
</p>

基于 Rust 编写的极速 Python 包与项目管理工具。

<p align="center">
  <picture align="center">
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/astral-sh/uv/assets/1309177/03aa9163-1c79-4a87-a31d-7a9311ed9310">
    <source media="(prefers-color-scheme: light)" srcset="https://github.com/astral-sh/uv/assets/1309177/629e59c0-9c6e-4013-9ad4-adb2bcf5080d">
    <img alt="展示基准测试结果的条形图" src="https://github.com/astral-sh/uv/assets/1309177/629e59c0-9c6e-4013-9ad4-adb2bcf5080d">
  </picture>
</p>

<p align="center">
  <i>在热缓存状态下安装 <a href="https://trio.readthedocs.io/">Trio</a> 的依赖项。</i>
</p>

## 核心亮点

- **All-in-One 统一工具**：单一工具即可全面替代
  `pip`、`pip-tools`、`pipx`、`poetry`、`pyenv`、`twine`、`virtualenv` 等繁杂工具链。
- **极致性能**：安装与解析速度比传统 `pip`
  [快 10 到 100 倍](https://github.com/astral-sh/uv/blob/main/BENCHMARKS.md)。
- **全功能项目管理**：提供[完善的项目管理能力](#项目管理-projects)，配合跨平台的[通用 Lockfile 锁定文件](https://docs.astral.sh/uv/concepts/projects/layout#the-lockfile)。
- **单文件脚本即开即用**：支持[直接运行单文件脚本](#单文件脚本-scripts)，并原生支持[内联依赖元数据 (PEP 723)](https://docs.astral.sh/uv/guides/scripts#declaring-script-dependencies)。
- **Python 版本管理**：无需安装 pyenv 即可[自动下载、安装与切换](#python-版本管理-python-versions)多种 Python 版本。
- **CLI 工具管理**：支持直接[安装与隔离运行](#全局工具管理-tools)
  Python 生态中发布的各类命令行工具（替代 `pipx`）。
- **完全兼容 pip 接口**：包含无缝[兼容 pip 的命令行接口](#pip-兼容接口-the-pip-interface)，以熟悉的语法享受极致性能提升。
- **Workspace 工作空间支持**：支持类似 Cargo 风格的[多包工作区（Workspaces）](https://docs.astral.sh/uv/concepts/projects/workspaces)，轻松管理大型单体代码库（Monorepo）。
- **磁盘空间高效**：具备[全局内容寻址缓存](https://docs.astral.sh/uv/concepts/cache)，自动跨项目去重共享依赖包。
- **开箱即用**：无需预装 Rust 或 Python 环境，可通过 `curl` 或 `pip` 直接一键安装二进制可执行文件。
- **全平台支持**：支持 macOS、Linux 与 Windows。

uv 由 [Astral](https://astral.sh) 团队开发与支持，他们也是 [Ruff](https://github.com/astral-sh/ruff)
与 [ty](https://github.com/astral-sh/ty) 的缔造者。

## 安装指南

使用官方独立安装脚本安装 uv：

```bash
# macOS 与 Linux
curl -LsSf https://astral.sh/uv/install.sh | sh
```

```powershell
# Windows (PowerShell)
powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"
```

也可以通过 [PyPI](https://pypi.org/project/uv/) 安装：

```bash
# 使用 pip
pip install uv
```

```bash
# 或使用 pipx
pipx install uv
```

通过独立安装脚本安装后，uv 支持一键自升级：

```bash
uv self update
```

更多安装选项请参阅[官方安装文档](https://docs.astral.sh/uv/getting-started/installation/)。

## 官方文档

uv 的完整文档位于 [docs.astral.sh/uv](https://docs.astral.sh/uv)。

此外，可通过 `uv help` 直接在终端查看各命令行的详细参考帮助。

## 核心功能

### 项目管理 (Projects)

uv 统一管理项目依赖与虚拟环境，支持 lockfile 锁定、workspaces 工作区等现代项目管理范式，体验类似于
`rye` 或 `poetry`：

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
Checked 1 package in 0.02ms
```

更多细节请查阅[项目管理入门指南](https://docs.astral.sh/uv/guides/projects/)。

uv 同样支持项目的构建打包与发布到 PyPI，即使项目本身并未使用 uv 管理。详情请参阅[项目发布指南](https://docs.astral.sh/uv/guides/publish/)。

### 单文件脚本 (Scripts)

uv 原生支持管理单文件独立脚本的依赖与临时运行环境。

创建新脚本并添加声明其依赖的内联元数据：

```console
$ echo 'import requests; print(requests.get("https://astral.sh"))' > example.py

$ uv add --script example.py requests
Updated `example.py`
```

随后即可在完全隔离的临时虚拟环境中直接执行该脚本：

```console
$ uv run example.py
Reading inline script metadata from: example.py
Installed 5 packages in 12ms
<Response [200]>
```

更多细节请查阅[脚本运行指南](https://docs.astral.sh/uv/guides/scripts/)。

### 全局工具管理 (Tools)

uv 能够直接安装和即时运行 Python 包提供的命令行工具，功能类似于 `pipx`。

使用 `uvx`（`uv tool run` 的简写别名）在瞬态隔离环境中执行工具：

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
       (__)\       )\/           ||----w |
           ||     ||
```

使用 `uv tool install` 将工具持久安装到系统 PATH：

```console
$ uv tool install ruff
Resolved 1 package in 6ms
Installed 1 package in 2ms
 + ruff==0.5.0
Installed 1 executable: ruff

$ ruff --version
ruff 0.5.0
```

更多细节请查阅[工具管理指南](https://docs.astral.sh/uv/guides/tools/)。

### Python 版本管理 (Python versions)

uv 原生支持下载、安装与快速切换 Python 解释器版本。

一键安装多个 Python 版本：

```console
$ uv python install 3.12 3.13 3.14
Installed 3 versions in 972ms
 + cpython-3.12.12-macos-aarch64-none (python3.12)
 + cpython-3.13.9-macos-aarch64-none (python3.13)
 + cpython-3.14.0-macos-aarch64-none (python3.14)
```

按需指定版本创建虚拟环境或运行代码（若本地未安装会自动按需拉取）：

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

将当前目录的默认 Python 版本锁定到指定版本：

```console
$ uv python pin 3.11
Pinned `.python-version` to `3.11`
```

更多细节请查阅 [Python 版本安装指南](https://docs.astral.sh/uv/guides/install-python/)。

### pip 兼容接口 (The pip interface)

uv 为常见的 `pip`、`pip-tools` 和 `virtualenv` 命令提供了无缝替代（Drop-in replacement）。

uv 还对传统接口进行了深度扩展，支持依赖版本覆盖、跨平台独立解析、可复现构建策略等高级功能。

无需修改现有工作流即可快速迁移到 uv —— 通过 `uv pip` 接口体验 10-100 倍的速度飞跃。

将 `requirements.in` 编译为跨平台通用的 `requirements.txt`：

```console
$ uv pip compile requirements.in \
   --universal \
   --output-file requirements.txt
Resolved 43 packages in 12ms
```

创建虚拟环境：

```console
$ uv venv
Using Python 3.12.3
Creating virtual environment at: .venv
Activate with: source .venv/bin/activate
```

同步并安装锁定的依赖清单：

```console
$ uv pip sync requirements.txt
Resolved 43 packages in 11ms
Installed 43 packages in 208ms
 + babel==2.15.0
 + black==24.4.2
 + certifi==2024.7.4
 ...
```

更多细节请查阅 [pip 接口指南](https://docs.astral.sh/uv/pip/index/)。

## 参与贡献

我们热烈欢迎所有阶段的开发者参与贡献。请参阅[贡献指南](https://github.com/astral-sh/uv?tab=contributing-ov-file#contributing)开启贡献。

## 常见问题解答 (FAQ)

#### uv 应该如何发音？

发音为字母读音 "you - vee"（[`/juː viː/`](https://en.wikipedia.org/wiki/Help:IPA/English#Key)）。

#### uv 的大小写规范是什么？

统一全小写写作 "uv"。详情参阅[样式指南](./STYLE.md#styling-uv)。

#### uv 支持哪些操作系统和平台？

请查阅 uv 的[支持平台文档](https://docs.astral.sh/uv/reference/platforms/)。

#### uv 是否已生产就绪？

是的，uv 非常稳定，已被全球众多企业和顶尖开源项目广泛应用于生产环境。详情参阅 uv 的[版本发布策略](https://docs.astral.sh/uv/reference/versioning/)。

## 致谢 (Acknowledgements)

- uv 底层依赖解析算法采用了
  [PubGrub](https://github.com/pubgrub-rs/pubgrub)。特别感谢 PubGrub 维护者
  [Jacob Finkelman](https://github.com/Eh2406) 的支持。
- uv 的 Git 交互实现参考了 [Cargo](https://github.com/rust-lang/cargo)。
- 部分优化灵感源自 [pnpm](https://pnpm.io/)、[Orogene](https://github.com/orogene/orogene) 与
  [Bun](https://github.com/oven-sh/bun) 的杰出设计。同时我们也从 Nathaniel J. Smith 的
  [Posy](https://github.com/njsmith/posy) 中汲取了灵感并移植了 Windows 的
  [trampoline](https://github.com/njsmith/posy/tree/main/src/trampolines/windows-trampolines/posy-trampoline)
  支持。

## 开源许可证 (License)

uv 采用双重许可证模式发布：

- Apache 许可证 2.0 版本 ([LICENSE-APACHE](LICENSE-APACHE) 或
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT 许可证 ([LICENSE-MIT](LICENSE-MIT) 或 <https://opensource.org/licenses/MIT>)

您可以根据需要自由选择。

<div align="center">
  <a target="_blank" href="https://astral.sh" style="background:none">
    <img src="https://raw.githubusercontent.com/astral-sh/uv/main/assets/svg/Astral.svg" alt="Made by Astral">
  </a>
</div>

---

> 💡
> **文档维护说明**：本中文文档由社区志愿者（@JasonYeYuhe）翻译维护，最后同步更新于 2026年8月31日。如发现内容与官方英文原版存在差异或新特性滞后，欢迎提交 PR 共同完善！
