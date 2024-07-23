
# Features

uv supports the full Python development experience — from installing Python and hacking on simple scripts to working on large projects that support multiple Python versions and platforms.

uv's commands can be broken down into sections of discrete features which can be used independently.

## Python version management

Installing and managing Python itself.

- `uv python install`
- `uv python list`
- `uv python find`
- `uv python pin`
- `uv python uninstall`

See the [guide on installing Python](./guides/install-python.md) to get started.

## Running scripts

Executing standalone Python scripts, e.g., `example.py`.

- `uv run`

See the [guide on running scripts](./guides/scripts.md) to get started.

## Project management

Creating and working on Python projects, i.e., with a `pyproject.toml`.

- `uv init`
- `uv add`
- `uv remove`
- `uv sync`
- `uv lock`
- `uv run`
- `uv tree`

See the [guide on projects](./guides/projects.md) to get started.

## Tool installation

Running and installing tools published to Python package indexes, e.g., `ruff` or `black`.

- `uvx` / `uv tool run`
- `uv tool install`
- `uv tool uninstall`
- `uv tool list`
- `uv tool update-shell`

See the [guide on tools](./guides/tools.md) to get started.

## Low-level commands

Manually managing environments and packages — intended to be used in legacy workflows or cases where the high-level commands do not provide enough control.

Creating virtual environments (replacing `venv` and `virtualenv`):

- `uv venv`

See the documentation on [using environments](./pip/environments.md) for details.

Managing packages in an environment (replacing [`pip`](https://github.com/pypa/pip)):

- `uv pip install`
- `uv pip show`
- `uv pip freeze`
- `uv pip check`
- `uv pip list`
- `uv pip uninstall`

See the documentation on [managing packages](./pip/packages.md) for details.

Locking packages in an environment (replacing [`pip-tools`](https://github.com/jazzband/pip-tools)):

- `uv pip compile`
- `uv pip sync`

See the documentation on [locking environments](./pip/compile.md) for details.

Viewing package dependencies in an environment (replacing [`pipdeptree`](https://github.com/tox-dev/pipdeptree)):

- `uv pip tree`

!!! important

    These commands do not exactly implement the interfaces and behavior of the tools they are based on. The further you stray from common workflows, the more likely you are to encounter differences. Consult the [pip-compatibility guide](./pip/compatibility.md) for details.

## Internal commands

Managing and inspecting uv's state, such as the cache, storage directories, or performing a self-update:

- `uv cache clean`
- `uv cache prune`
- `uv cache dir`
- `uv tool dir`
- `uv python dir`
- `uv self update`

## Next steps

Check out the [documentation overview](./overview.md) for a list of guides and concepts.
