# Configuration files

uv supports persistent configuration files at both the project- and user-level.

Specifically, uv will search for a `pyproject.toml` or `uv.toml` file in the current directory, or
in the nearest parent directory.

!!! note

    For `tool` commands, which operate the user level, local configuration
    files will be ignored. Instead, uv will exclusively read from user-level configuration
    (e.g., `~/.config/uv/uv.toml`).

In workspaces, uv will begin its search at the workspace root, ignoring any configuration defined in
workspace members. Since the workspace is locked as a single unit, configuration is shared across
all members.

If a `pyproject.toml` file is found, uv will read configuration from the `[tool.uv.pip]` table. For
example, to set a persistent index URL, add the following to a `pyproject.toml`:

```toml title="project.toml"
[tool.uv.pip]
index-url = "https://test.pypi.org/simple"
```

(If there is no such table, the `pyproject.toml` file will be ignored, and uv will continue
searching in the directory hierarchy.)

If a `uv.toml` file is found, uv will read from the `[pip]` table. For example:

```toml title="uv.toml"
[pip]
index-url = "https://test.pypi.org/simple"
```

uv will also discover user-level configuration at `~/.config/uv/uv.toml` (or
`$XDG_CONFIG_HOME/uv/uv.toml`) on macOS and Linux, or `%APPDATA%\uv\uv.toml` on Windows. User-level
configuration must use the `uv.toml` format, rather than the `pyproject.toml` format, as a
`pyproject.toml` is intended to define a Python _project_.

If both project- and user-level configuration are found, the settings will be merged, with the
project-level configuration taking precedence. Specifically, if a string, number, or boolean is
present in both tables, the project-level value will be used, and the user-level value will be
ignored. If an array is present in both tables, the arrays will be concatenated, with the
project-level settings appearing earlier in the merged array.

Settings provided via environment variables take precedence over persistent configuration, and
settings provided via the command line take precedence over both.

uv accepts a `--no-config` command-line argument which, when provided, disables the discovery of any
persistent configuration.

uv also accepts a `--config-file` command-line argument, which accepts a path to a `uv.toml` to use
as the configuration file. When provided, this file will be used in place of _any_ discovered
configuration files (e.g., user-level configuration will be ignored).

## Settings

See the [settings reference](../reference/settings.md) for an enumeration of the available settings.
