# Installing uv

## Installation methods

Install uv with our standalone installers or your package manager of choice.

### Standalone installer

uv provides a standalone installer to download and install uv:

=== "macOS and Linux"

    ```console
    $ curl -LsSf https://astral.sh/uv/install.sh | sh
    ```

=== "Windows"

    ```console
    $ powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"
    ```

Request a specific version by including it in the URL:

=== "macOS and Linux"

    ```console
    $ curl -LsSf https://astral.sh/uv/0.4.6/install.sh | sh
    ```

=== "Windows"

    ```console
    $ powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/0.4.6/install.ps1 | iex"
    ```

!!! tip

    The installation script may be inspected before use:

    === "macOS and Linux"

        ```console
        $ curl -LsSf https://astral.sh/uv/install.sh | less
        ```

    === "Windows"

        ```console
        $ powershell -c "irm https://astral.sh/uv/install.ps1 | more"
        ```

    Alternatively, the installer or binaries can be downloaded directly from [GitHub](#github-releases).

#### Configuring installation

By default, uv is installed to `~/.local/bin`. uv's installer also respects the `XDG_BIN_HOME`
environment variable. To use a custom installation path, use `UV_INSTALL_DIR`:

=== "macOS and Linux"

    ```console
    $ curl -LsSf https://astral.sh/uv/install.sh | env UV_INSTALL_DIR="/custom/path" sh
    ```

=== "Windows"

    ```powershell
    $env:UV_INSTALL_DIR = "C:\Custom\Path" powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"
    ```

The installer will also update your shell profiles to ensure the uv binary is on your `PATH`. To
disable this behavior, use `INSTALLER_NO_MODIFY_PATH`. For example:

```console
$ curl -LsSf https://astral.sh/uv/install.sh | env INSTALLER_NO_MODIFY_PATH=1 sh
```

Using environment variables is recommended because they are consistent across platforms. However,
options can be passed directly to the install script. For example, to see the available options:

```console
$ curl -LsSf https://astral.sh/uv/install.sh | sh -s -- --help
```

In ephemeral environments like CI, use `UV_UNMANAGED_INSTALL` to install uv to a specific path while
preventing the installer from modifying shell profiles or environment variables:

```console
$ curl -LsSf https://astral.sh/uv/install.sh | env UV_UNMANAGED_INSTALL="/custom/path" sh
```

The use of `UV_UNMANAGED_INSTALL` will also disable self-updates (via `uv self update`).

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

### Cargo

uv is available via Cargo, but must be built from Git rather than [crates.io](https://crates.io) due
to its dependency on unpublished crates.

```console
$ cargo install --git https://github.com/astral-sh/uv uv
```

### Homebrew

uv is available in the core Homebrew packages.

```console
$ brew install uv
```

### Winget

uv is available via [winget](https://winstall.app/apps/astral-sh.uv).

```console
$ winget install --id=astral-sh.uv  -e
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

!!! tip

    Updating uv will re-run the installer and can modify your shell profiles. To disable this
    behavior, set `INSTALLER_NO_MODIFY_PATH=1`.

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

To enable shell autocompletion for uvx, run one of the following:

=== "Linux and macOS"

    ```bash
    # Determine your shell (e.g., with `echo $SHELL`), then run one of:
    echo 'eval "$(uvx --generate-shell-completion bash)"' >> ~/.bashrc
    echo 'eval "$(uvx --generate-shell-completion zsh)"' >> ~/.zshrc
    echo 'uvx --generate-shell-completion fish | source' >> ~/.config/fish/config.fish
    echo 'eval (uvx --generate-shell-completion elvish | slurp)' >> ~/.elvish/rc.elv
    ```

=== "Windows"

    ```powershell
    Add-Content -Path $PROFILE -Value '(& uvx --generate-shell-completion powershell) | Out-String | Invoke-Expression'
    ```

Then restart the shell or source the shell config file.

## Uninstallation

If you need to remove uv from your system, just remove the `uv` and `uvx` binaries:

=== "macOS and Linux"

    ```console
    $ rm ~/.local/bin/uv ~/.local/bin/uvx
    ```

=== "Windows"

    ```powershell
    $ rm $HOME\.local\bin\uv.exe
    $ rm $HOME\.local\bin\uvx.exe
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
