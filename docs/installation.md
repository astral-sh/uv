# Installing uv

Install uv with our standalone installers, from PyPI, or from your package manager of choice.

## Standalone installer

uv provides a standalone installer that downloads and installs uv:

```bash
# On macOS and Linux.
curl -LsSf https://astral.sh/uv/install.sh | sh

# On Windows.
powershell -c "irm https://astral.sh/uv/install.ps1 | iex"
```

uv is installed to `~/.cargo/bin`.

A specific release can be requested by including the version in the URL:

```bash
# On macOS and Linux.
curl -LsSf https://astral.sh/uv/0.2.11/install.sh | sh

# On Windows.
powershell -c "irm https://astral.sh/uv/0.2.11/install.ps1 | iex"
```

When the standalone installer is used, uv can upgrade itself.

```bash
uv self update
```

Note when all other installers are used, self updates are disabled.

## PyPI

For convenience, uv is published to [PyPI](https://pypi.org/project/uv/). When installed from PyPI, uv can be built from source but there are prebuilt distributions (wheels) for many platforms.

If installing from PyPI, we recommend using `pipx` to install uv into an isolated environment:

```bash
pipx install uv
```

However, `pip` can also be used:

```bash
pip install uv
```

## Homebrew

uv is available in the core Homebrew packages.

```bash
brew install uv
```

## Docker

uv provides a Docker image at [`ghcr.io/astral-sh/uv`](https://github.com/astral-sh/uv/pkgs/container/uv).

See our guide on [using uv in Docker](./guides/docker.md) for more details.
