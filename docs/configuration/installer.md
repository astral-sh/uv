# Configuring the uv installer

## Changing the install path

By default, uv is installed to `~/.local/bin`. If `XDG_BIN_HOME` is set, it will be used instead.
Similarly, if `XDG_DATA_HOME` is set, the target directory will be inferred as
`XDG_DATA_HOME/../bin`.

To change the installation path, use `UV_INSTALL_DIR`:

=== "macOS and Linux"

    ```console
    $ curl -LsSf https://astral.sh/uv/install.sh | env UV_INSTALL_DIR="/custom/path" sh
    ```

=== "Windows"

    ```powershell
    $env:UV_INSTALL_DIR = "C:\Custom\Path" powershell -ExecutionPolicy ByPass -c "irm https://astral.sh/uv/install.ps1 | iex"
    ```

## Disabling shell modifications

The installer may also update your shell profiles to ensure the uv binary is on your `PATH`. To
disable this behavior, use `INSTALLER_NO_MODIFY_PATH`. For example:

```console
$ curl -LsSf https://astral.sh/uv/install.sh | env INSTALLER_NO_MODIFY_PATH=1 sh
```

If installed with `INSTALLER_NO_MODIFY_PATH`, subsequent operations, like `uv self update`, will not
modify your shell profiles.

## Unmanaged installations

In ephemeral environments like CI, use `UV_UNMANAGED_INSTALL` to install uv to a specific path while
preventing the installer from modifying shell profiles or environment variables:

```console
$ curl -LsSf https://astral.sh/uv/install.sh | env UV_UNMANAGED_INSTALL="/custom/path" sh
```

The use of `UV_UNMANAGED_INSTALL` will also disable self-updates (via `uv self update`).

## Passing options to the install script

Using environment variables is recommended because they are consistent across platforms. However,
options can be passed directly to the install script. For example, to see the available options:

```console
$ curl -LsSf https://astral.sh/uv/install.sh | sh -s -- --help
```
