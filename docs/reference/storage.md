# Storage

uv persists data in several locations on your system.

## Directory Strategies

uv follows platform conventions for determining where to store different types of data.

Generally, it's best to configure these rather than each uv-specific storage location.

### Cache

For temporary files and caches:

- `$XDG_CACHE_HOME/uv` or `$HOME/.cache/uv` on Unix systems
- `%LOCALAPPDATA%\uv\cache` on Windows

### Data

For persistent application data:

- `$XDG_DATA_HOME/uv` or `~/.local/share/uv` on Unix systems
- `%APPDATA%\uv\data` on Windows
- `.uv` in the working directory as a fallback

### Config

For user configuration files:

- `$XDG_CONFIG_HOME/uv` or `~/.config/uv` on Unix systems
- `%APPDATA%\uv` on Windows

For system configuration files:

- `$XDG_CONFIG_DIRS/uv` or `/etc/uv` on Unix systems
- `%PROGRAMDATA%\uv` on Windows

### Binaries

For executable files:

- `$XDG_BIN_HOME`, `$XDG_DATA_HOME/../bin`, or `~/.local/bin` on all platforms

## Cache

uv uses a local cache to avoid re-downloading and re-building dependencies. The cache contains
wheels, source distributions, responses from package indexes, Git repositories, and Python
interpreter metadata.

By default, the cache is stored in the [cache home](#cache).

Use `uv cache dir` to show the current cache directory path.

Use the `--cache-dir` option or set the `UV_CACHE_DIR` environment variable to configure the cache
location.

For more details, see the [cache documentation](../concepts/cache.md).

## Python versions

uv can download and manage Python versions.

By default, Python versions are stored in the [data home](#data) in a `python/` subdirectory, e.g.,
`~/.local/share/uv/python`.

Use `uv python dir` to show the Python installation directory.

Use the `UV_PYTHON_INSTALL_DIR` environment variable to configure the installation directory.

For more details, see the [Python versions documentation](../concepts/python-versions.md).

### Python executables

!!! note

    This feature is in preview, and is not enabled without `--preview` or `UV_PREVIEW`.

uv also supports adding Python executables to your `PATH`.

By default, Python executables are stored in the [bin home](#bin).

Use `uv python dir --bin` to show the Python executable directory.

Use the `UV_PYTHON_BIN_DIR` environment variable to configure the executable directory.

## Tools

uv can install Python applications as tools using `uv tool install`. Each tool gets its own isolated
environment.

By default, tools are installed in the [data home](#data) under a `tools/` subdirectory, e.g.,
`~/.local/share/uv/tools`

Use `uv tool dir` to show the tool installation directory.

Use the `UV_TOOL_DIR` environment variable to configure the installation directory.

For more details, see the [tools documentation](../concepts/tools.md).

### Tool executables

When installing tools, uv will add tools to your `PATH`.

By default, tool executbales are stored in the [bin home](#bin).

Use `uv tool dir --bin` to show the tool executable directory.

Use the `UV_TOOL_BIN_DIR` environment variable to configure the executable directory.

## Configuration

uv's behavior can be configured through configuration files stored in standard locations.

Configuration files are located in the [config directories](#config).

For more details, see the [configuration files documentation](../concepts/configuration-files.md).

## Project environments

uv creates virtual environments for projects to isolate their dependencies.

By default, project virtual environments are created in `.venv` within the project directory.

Use the `UV_PROJECT_ENVIRONMENT` environment variable to override this location.

For more details, see the
[projects environment documentation](../concepts/projects/config.md#project-environment-path).
