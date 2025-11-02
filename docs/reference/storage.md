# Storage

uv persists data in several locations on your system.

## Directory Strategies

uv follows platform conventions (like
[XDG](https://specifications.freedesktop.org/basedir-spec/latest/) on Unix) for determining where to
store different types of data. Generally, it's best to configure these rather than each uv-specific
storage location.

Here's a summary of the locations uv uses on each platform:

| Purpose                    | Unix Default                                                               | Windows Default                                         |
| -------------------------- | -------------------------------------------------------------------------- | ------------------------------------------------------- |
| Temporary files and caches | `$XDG_CACHE_HOME/uv` or `~/.cache/uv` as a fallback                        | `%LOCALAPPDATA%\uv\cache`                               |
| Persistent data            | `$XDG_DATA_HOME/uv` or `~/.local/share/uv` as a fallback                   | `%APPDATA%\uv\data` if exists, otherwise `%APPDATA%\uv` |
| User configuration files   | `$XDG_CONFIG_HOME/uv` or `~/.config/uv` as a fallback                      | `%APPDATA%\uv`                                          |
| System configuration files | `$XDG_CONFIG_DIRS/uv` or `/etc/uv` as a fallback                           | `%PROGRAMDATA%\uv`                                      |
| Executables                | `$XDG_BIN_HOME` or `$XDG_DATA_HOME/../bin` or `~/.local/bin` as a fallback | same as on Unix                                         |
| Environment                | `.venv` in the project or workspace directory                              | same as on Unix                                         |

## Caching

uv uses a local cache to avoid re-downloading and re-building dependencies.

By default, the cache is stored according to [the table above](#directory-strategies), and can be
overridden via command line arguments, environment variables, or settings as detailed in
[the cache documentation](../concepts/cache.md#cache-directory).

Use `uv cache dir` to show the current cache directory path.

It is important for performance for the cache directory to be on the same filesystem as the
[virtualenvs](#project-environments) uv operates on.

## Python versions

uv can download and manage Python versions.

By default, Python versions are stored as persistent data according to
[the table above](#directory-strategies), in a `python/` subdirectory, e.g.,
`~/.local/share/uv/python`.

Use `uv python dir` to show the Python installation directory.

Use the `UV_PYTHON_INSTALL_DIR` environment variable to configure the installation directory.

!!! note

    Changing where Python is installed will not be automatically reflected in existing virtual environments; they will keep referring to the old location, and will need to be updated manually (e.g. by re-creating them).

For more details on how uv manages Python versions, see the
[dedicated documentation page](../concepts/python-versions.md).

### Python executables

uv also supports adding Python executables to your `PATH`.

By default, Python executables are stored according to [the table above](#directory-strategies).

Use `uv python dir --bin` to show the Python executable directory.

Use the `UV_PYTHON_BIN_DIR` environment variable to configure the executable directory.

## Tools

uv can install Python applications as tools using `uv tool install`.

By default, tools are installed as persistent data according to
[the table above](#directory-strategies), under a `tools/` subdirectory, e.g.,
`~/.local/share/uv/tools`

Use `uv tool dir` to show the tool installation directory.

Use the `UV_TOOL_DIR` environment variable to configure the installation directory.

For more details, see the [tools documentation](../concepts/tools.md).

### Tool executables

When installing tools, uv will add tools to your `PATH`.

By default, tool executables are stored according to [the table above](#directory-strategies).

Use `uv tool dir --bin` to show the tool executable directory.

Use the `UV_TOOL_BIN_DIR` environment variable to configure the executable directory.

## uv

uv itself is also installed by [the installer](./installer.md) into the executables folder from
[the table above](#directory-strategies), and this can be overridden via the `UV_INSTALL_DIR`
environment variable.

## Configuration

uv's behavior (including most of the storage locations on this page) can be configured through
configuration files stored in standard locations.

Configuration files are located in the corresponding system- or user-specific locations from
[the table above](#directory-strategies).

For more details, see the [configuration files documentation](../concepts/configuration-files.md).

## Project environments

uv creates virtual environments for projects to isolate their dependencies.

By default, project virtual environments are created in `.venv` within the project directory, and a
workspace's environment is created with the same name in the workspace root.

Use the `UV_PROJECT_ENVIRONMENT` environment variable to override this location, which is should be
either an absolute path, or relative to the workspace root.

For more details, see the
[projects environment documentation](../concepts/projects/config.md#project-environment-path).
