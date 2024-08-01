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

!!! tip

    The installation script may be inspected with:

    ```bash
    # On macOS and Linux.
    curl -LsSf https://astral.sh/uv/install.sh | less

    # On Windows.
    powershell -c "irm https://astral.sh/uv/install.ps1 | more"
    ```

    Alternatively, the installer or binaries can be downloaded directly from [GitHub](#github-releases).

A specific release can be requested by including the version in the URL:

```bash
# On macOS and Linux.
curl -LsSf https://astral.sh/uv/0.2.11/install.sh | sh

# On Windows.
powershell -c "irm https://astral.sh/uv/0.2.11/install.ps1 | iex"
```

When the standalone installer is used, uv can upgrade itself:

```bash
uv self update
```

When another installation method is used, self updates are disabled. Use the package manager's
upgrade method instead.

## PyPI

For convenience, uv is published to [PyPI](https://pypi.org/project/uv/).

If installing from PyPI, we recommend installing uv into an isolated environment, e.g., with `pipx`:

```bash
pipx install uv
```

However, `pip` can also be used:

```bash
pip install uv
```

!!! note

    There are prebuilt distributions (wheels) for many platforms; if not available for a given 
    platform, uv will be built from source which requires a Rust toolchain to be installed. See the
    [contributing setup guide](https://github.com/astral-sh/uv/blob/main/CONTRIBUTING.md#setup)
    for details on building uv from source.

## Homebrew

uv is available in the core Homebrew packages.

```bash
brew install uv
```

## Docker

uv provides a Docker image at
[`ghcr.io/astral-sh/uv`](https://github.com/astral-sh/uv/pkgs/container/uv).

See our guide on [using uv in Docker](./guides/integration/docker.md) for more details.

## GitHub Releases

uv release artifacts can be downloaded directly from [GitHub
Releases](https://github.com/astral-sh/uv/releases).

Each release page includes binaries for all supported platforms as well as instructions for using
the standalone installer via `github.com` instead of `astral.sh`.
