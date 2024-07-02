# First steps with uv

## Check the version

After [installing uv](./installation.md), check that it works from the CLI:

```bash
uv version
```

The installed version should be displayed.

## uv's interfaces

uv's commands can be grouped into a few sections.

### Project management

These commands are intended for managing development of a Python project. In these workflows, management of the virtual environment is done automatically by uv.

- `uv add`
- `uv remove`
- `uv sync`
- `uv lock`

See the documentation on [projects](./preview/projects.md) for more details on getting started.

### Toolchain management

These commands are used to manage Python itself. uv is capable of installing and managing multiple Python versions.

- `uv toolchain install`
- `uv toolchain list`
- `uv toolchain find`

See the documentation on [toolchains](./preview/toolchains.md) for more details on getting started.

### Command-line tool management

These commands are used to manage command-line tools written in Python.

- `uv tool run`

See the documentation on [tools](./preview/tools.md) for more details on getting started.

### Low-level plumbing commands

The commands in this group allow manual management of environments and packages. They are intended to be used in legacy workflows or cases where the high-level commands do not provide enough control.

This command is designed as replacement for the Python `venv` and `virtualenv` modules:

- `uv venv`

These commands are designed as replacements for `pip`:

- `uv pip install`
- `uv pip show`
- `uv pip freeze`
- `uv pip check`
- `uv pip list`
- `uv pip uninstall`

These commands are designed as replacements for `pip-tools`:

- `uv pip compile`
- `uv pip sync`

This command is designed as a replacement for `pipdeptree`:

- `uv pip tree`

Please note these commands do not exactly implement the interfaces and behavior of the tools that informed their design. Consult the [pip-compatibility guide](./pip/compatibility.md) for details on differences.
