# Installing uv

## Installation methods

Install uv with our standalone installers or your package manager of choice.

### Standalone installer

uv provides a standalone installer to download and install uv:

```console title="macOS and Linux"
$ curl -LsSf https://astral.sh/uv/install.sh | sh
```

```console title="Windows"
$ powershell -c "irm https://astral.sh/uv/install.ps1 | iex"
```

By default, uv is installed to `~/.cargo/bin`.

!!! tip

    The installation script may be inspected before use:

    ```console title="macOS and Linux"
    $ curl -LsSf https://astral.sh/uv/install.sh | less
    ```

    ```console title="Windows"
    $ powershell -c "irm https://astral.sh/uv/install.ps1 | more"
    ```

    Alternatively, the installer or binaries can be downloaded directly from [GitHub](#github-releases).

Request a specific version by including it in the URL:

```console title="macOS and Linux"
$ curl -LsSf https://astral.sh/uv/0.3.4/install.sh | sh
```

```console title="Windows"
$ powershell -c "irm https://astral.sh/uv/0.3.4/install.ps1 | iex"
```

### PyPI

For convenience, uv is published to [PyPI](https://pypi.org/project/uv/).

If installing from PyPI, we recommend installing uv into an isolated environment, e.g., with `pipx`:

```console
$ pipx install uv
```

However, `pip` can also be used:

```console
$ pip install uv
```

!!! note

    uv ships with prebuilt distributions (wheels) for many platforms; if a wheel is not available for a given
    platform, uv will be built from source, which requires a Rust toolchain. See the
    [contributing setup guide](https://github.com/astral-sh/uv/blob/main/CONTRIBUTING.md#setup)
    for details on building uv from source.

### Homebrew

uv is available in the core Homebrew packages.

```console
$ brew install uv
```

### Docker

uv provides a Docker image at
[`ghcr.io/astral-sh/uv`](https://github.com/astral-sh/uv/pkgs/container/uv).

See our guide on [using uv in Docker](../guides/integration/docker.md) for more details.

### GitHub Releases

uv release artifacts can be downloaded directly from
[GitHub Releases](https://github.com/astral-sh/uv/releases).

Each release page includes binaries for all supported platforms as well as instructions for using
the standalone installer via `github.com` instead of `astral.sh`.

## Upgrading uv

When uv is installed via the standalone installer, it can update itself on-demand:

```console
$ uv self update
```

When another installation method is used, self-updates are disabled. Use the package manager's
upgrade method instead. For example, with `pip`:

```console
$ pip install --upgrade uv
```

## Shell autocompletion

To enable shell autocompletion for uv commands, run one of the following:

=== "Linux and macOS"

    ```bash
    # Determine your shell (e.g., with `echo $SHELL`), then run one of:
    echo 'eval "$(uv generate-shell-completion bash)"' >> ~/.bashrc
    echo 'eval "$(uv generate-shell-completion zsh)"' >> ~/.zshrc
    echo 'uv generate-shell-completion fish | source' >> ~/.config/fish/config.fish
    echo 'eval (uv generate-shell-completion elvish | slurp)' >> ~/.elvish/rc.elv
    ```

=== "Windows"

    ```powershell
    Add-Content -Path $PROFILE -Value '(& uv generate-shell-completion powershell) | Out-String | Invoke-Expression'
    ```

Then restart the shell or source the shell config file.

## Uninstallation

If you need to remove uv from your system, just remove the `uv` and `uvx` binaries:

```console
$ rm ~/.cargo/bin/uv ~/.cargo/bin/uvx
```

!!! tip

    You may want to remove data that uv has stored before removing the binaries:

    ```console
    $ uv cache clean
    $ rm -r "$(uv python dir)"
    $ rm -r "$(uv tool dir)"
    ```

## Next steps

See the [first steps](./first-steps.md) or jump straight to the [guides](../guides/index.md) to
start using uv.
