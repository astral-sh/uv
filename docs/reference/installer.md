# The uv installer

## Changing the installation path

By default, uv is installed in the user [executable directory](./storage.md#executable-directory).

To change the installation path, use `UV_INSTALL_DIR`:

=== "macOS and Linux"

    ```console
    $ curl -LsSf https://astral.sh/uv/install.sh | env UV_INSTALL_DIR="/custom/path" sh
    ```

=== "Windows"

    ```pwsh-session
    PS> powershell -ExecutionPolicy ByPass -c {$env:UV_INSTALL_DIR = "C:\Custom\Path";irm https://astral.sh/uv/install.ps1 | iex}
    ```

!!! note

    Changing the installation path only affects where the uv binary is installed. uv will still store
    its data (cache, Python installations, tools, etc.) in the default locations. See the
    [storage reference](./storage.md) for details on these locations and how to customize them.

## Disabling shell modifications

The installer may also update your shell profiles to ensure the uv binary is on your `PATH`. To
disable this behavior, use `UV_NO_MODIFY_PATH`. For example:

```console
$ curl -LsSf https://astral.sh/uv/install.sh | env UV_NO_MODIFY_PATH=1 sh
```

If installed with `UV_NO_MODIFY_PATH`, subsequent operations, like `uv self update`, will not modify
your shell profiles.

## Unmanaged installations

In ephemeral environments like CI, use `UV_UNMANAGED_INSTALL` to install uv to a specific path while
preventing the installer from modifying shell profiles or environment variables:

```console
$ curl -LsSf https://astral.sh/uv/install.sh | env UV_UNMANAGED_INSTALL="/custom/path" sh
```

The use of `UV_UNMANAGED_INSTALL` will also disable self-updates (via `uv self update`).

## Passing options to the installation script

Using environment variables is recommended because they are consistent across platforms. However,
options can be passed directly to the installation script. For example, to see the available
options:

```console
$ curl -LsSf https://astral.sh/uv/install.sh | sh -s -- --help
```
