# Project structure and files

## The `pyproject.toml`

Python project metadata is defined in a `pyproject.toml` file. uv requires this file to identify the
root directory of a project.

!!! tip

    `uv init` can be used to create a new project. See [Creating projects](./init.md) for
    details.

A minimal project definition includes a name, version, and description:

```toml title="pyproject.toml"
[project]
name = "example"
version = "0.1.0"
description = "Add your description here"
```

It's recommended, but not required, to include a Python version requirement in the `[project]`
section:

```toml title="pyproject.toml"
requires-python = ">=3.12"
```

Including a Python version requirement defines the Python syntax that is allowed in the project and
affects selection of dependency versions (they must support the same Python version range).

The `pyproject.toml` also lists dependencies of the project in the `project.dependencies` and
`project.optional-dependencies` fields. uv supports modifying the project's dependencies from the
command line with `uv add` and `uv remove`. uv also supports extending the standard dependency
definitions with [package sources](./dependencies.md) in `tool.uv.sources`.

!!! tip

    See the official [`pyproject.toml` guide](https://packaging.python.org/en/latest/guides/writing-pyproject-toml/) for more details on getting started with a `pyproject.toml`.

## The project environment

When working on a project with uv, uv will create a virtual environment as needed. While some uv
commands will create a temporary environment (e.g., `uv run --isolated`), uv also manages a
persistent environment with the project and its dependencies in a `.venv` directory next to the
`pyproject.toml`. It is stored inside the project to make it easy for editors to find — they need
the environment to give code completions and type hints. It is not recommended to include the
`.venv` directory in version control; it is automatically excluded from `git` with an internal
`.gitignore` file.

To run a command in the project environment, use `uv run`. Alternatively the project environment can
be activated as normal for a virtual environment.

When `uv run` is invoked, it will create the project environment if it does not exist yet or ensure
it is up-to-date if it exists. The project environment can also be explicitly created with
`uv sync`.

It is _not_ recommended to modify the project environment manually, e.g., with `uv pip install`. For
project dependencies, use `uv add` to add a package to the environment. For one-off requirements,
use [`uvx`](../../guides/tools.md) or
[`uv run --with`](./run.md#requesting-additional-dependencies).

!!! tip

    If you don't want uv to manage the project environment, set [`managed = false`](../../reference/settings.md#managed)
    to disable automatic locking and syncing of the project. For example:

    ```toml title="pyproject.toml"
    [tool.uv]
    managed = false
    ```

## The lockfile

uv creates a `uv.lock` file next to the `pyproject.toml`.

`uv.lock` is a _universal_ or _cross-platform_ lockfile that captures the packages that would be
installed across all possible Python markers such as operating system, architecture, and Python
version.

Unlike the `pyproject.toml`, which is used to specify the broad requirements of your project, the
lockfile contains the exact resolved versions that are installed in the project environment. This
file should be checked into version control, allowing for consistent and reproducible installations
across machines.

A lockfile ensures that developers working on the project are using a consistent set of package
versions. Additionally, it ensures when deploying the project as an application that the exact set
of used package versions is known.

The lockfile is created and updated during uv invocations that use the project environment, i.e.,
`uv sync` and `uv run`. The lockfile may also be explicitly updated using `uv lock`.

`uv.lock` is a human-readable TOML file but is managed by uv and should not be edited manually.
There is no Python standard for lockfiles at this time, so the format of this file is specific to uv
and not usable by other tools.
