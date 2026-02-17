# Installing uv

## Installation methods

Install uv with our standalone installers or your package manager of choice.

### Standalone installer

uv provides a standalone installer to download and install uv:

=== "macOS and Linux"

    Use `curl` to download the script and execute it with `sh`:

    ```console
    $ curl -LsSf https://astral.sh/uv/install.sh | sh
    ```

    If your system doesn't have `curl`, you can use `wget`:

    ```console
    $ wget -qO- https://astral.sh/uv/install.sh | sh
    ```

    Request a specific version by including it in the URL:

    ```console
    $ curl -LsSf https://astral.sh/uv/0.10.4/install.sh | sh
    ```

=== "Windows"

    Use `irm` to download the script and execute it with `iex`:

    ```pwsh-session
    PS> powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"
    ```

    Changing the [execution policy](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_execution_policies?view=powershell-7.4#powershell-execution-policies) allows running a script from the internet.

    Request a specific version by including it in the URL:

    ```pwsh-session
    PS> powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/0.10.4/install.ps1 | iex"
    ```

!!! tip

    The installation script may be inspected before use:

    === "macOS and Linux"

        ```console
        $ curl -LsSf https://astral.sh/uv/install.sh | less
        ```

    === "Windows"

        ```pwsh-session
        PS> powershell -c "irm https://astral.sh/uv/install.ps1 | more"
        ```

    Alternatively, the installer or binaries can be downloaded directly from [GitHub](#github-releases).

See the reference documentation on the [installer](../reference/installer.md) for details on
customizing your uv installation.

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

### MacPorts

uv is available via [MacPorts](https://ports.macports.org/port/uv/).

```console
$ sudo port install uv
```

### WinGet

uv is available via [WinGet](https://winstall.app/apps/astral-sh.uv).

```console
$ winget install --id=astral-sh.uv  -e
```

### Scoop

uv is available via [Scoop](https://scoop.sh/#/apps?q=uv).

```console
$ scoop install main/uv
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

### Cargo

uv is available via [crates.io](https://crates.io).

```console
$ cargo install --locked uv
```

!!! note

    This method builds uv from source, which requires a compatible Rust toolchain.

## Upgrading uv

When uv is installed via the standalone installer, it can update itself on-demand:

```console
$ uv self update
```

!!! tip

    Updating uv will re-run the installer and can modify your shell profiles. To disable this
    behavior, set `UV_NO_MODIFY_PATH=1`.

When another installation method is used, self-updates are disabled. Use the package manager's
upgrade method instead. For example, with `pip`:

```console
$ pip install --upgrade uv
```

## Shell autocompletion

!!! tip

    You can run `echo $SHELL` to help you determine your shell.

To enable shell autocompletion for uv commands, run one of the following:

=== "Bash"

    ```bash
    echo 'eval "$(uv generate-shell-completion bash)"' >> ~/.bashrc
    ```

=== "Zsh"

    ```bash
    echo 'eval "$(uv generate-shell-completion zsh)"' >> ~/.zshrc
    ```

=== "fish"

    ```bash
    echo 'uv generate-shell-completion fish | source' > ~/.config/fish/completions/uv.fish
    ```

=== "Elvish"

    ```bash
    echo 'eval (uv generate-shell-completion elvish | slurp)' >> ~/.elvish/rc.elv
    ```

=== "PowerShell / pwsh"

    ```powershell
    if (!(Test-Path -Path $PROFILE)) {
      New-Item -ItemType File -Path $PROFILE -Force
    }
    Add-Content -Path $PROFILE -Value '(& uv generate-shell-completion powershell) | Out-String | Invoke-Expression'
    ```

To enable shell autocompletion for uvx, run one of the following:

=== "Bash"

    ```bash
    echo 'eval "$(uvx --generate-shell-completion bash)"' >> ~/.bashrc
    ```

=== "Zsh"

    ```bash
    echo 'eval "$(uvx --generate-shell-completion zsh)"' >> ~/.zshrc
    ```

=== "fish"

    ```bash
    echo 'uvx --generate-shell-completion fish | source' > ~/.config/fish/completions/uvx.fish
    ```

=== "Elvish"

    ```bash
    echo 'eval (uvx --generate-shell-completion elvish | slurp)' >> ~/.elvish/rc.elv
    ```

=== "PowerShell / pwsh"

    ```powershell
    if (!(Test-Path -Path $PROFILE)) {
      New-Item -ItemType File -Path $PROFILE -Force
    }
    Add-Content -Path $PROFILE -Value '(& uvx --generate-shell-completion powershell) | Out-String | Invoke-Expression'
    ```

Then restart the shell or source the shell config file.

## Uninstallation

If you need to remove uv from your system, follow these steps:

1.  Clean up stored data (optional):

    ```console
    $ uv cache clean
    $ rm -r "$(uv python dir)"
    $ rm -r "$(uv tool dir)"
    ```

    !!! tip

        Before removing the binaries, you may want to remove any data that uv has stored. See the
        [storage reference](../reference/storage.md) for details on where uv stores data.

2.  Remove the uv, uvx, and uvw binaries:

    === "macOS and Linux"

        ```console
        $ rm ~/.local/bin/uv ~/.local/bin/uvx
        ```

    === "Windows"

        ```pwsh-session
        PS> rm $HOME\.local\bin\uv.exe
        PS> rm $HOME\.local\bin\uvx.exe
        PS> rm $HOME\.local\bin\uvw.exe
        ```

    !!! note

        Prior to 0.5.0, uv was installed into `~/.cargo/bin`. The binaries can be removed from there to
        uninstall. Upgrading from an older version will not automatically remove the binaries from
        `~/.cargo/bin`.

## Next steps

See the [first steps](./first-steps.md) or jump straight to the [guides](../guides/index.md) to
start using uv.
