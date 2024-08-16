# Working on projects

uv is capable of managing Python projects using a `pyproject.toml` with a `[project]` metadata
table.

## Creating a new project

You can create a new Python project using the `uv init` command:

```console
$ uv init hello-world
$ cd hello-world
```

Alternatively, you can initialize a project in the working directory:

```console
$ mkdir hello-world
$ cd hello-world
$ uv init
```

This will create the following directory structure:

```text
.
├── pyproject.toml
├── README.md
└── src
    └── hello_world
        └── __init__.py
```

### Working on an existing project

If your project already contains a standard `pyproject.toml`, you can start using uv immediately.
Commands like `uv add` and `uv run` will create a [lockfile](#uvlock) and [environment](#venv) the
first time they are used.

If you are migrating from an alternative Python package manager, you may need to edit your
`pyproject.toml` manually before using uv. Most Python package managers extend the `pyproject.toml`
standard to support common features, such as development dependencies. These extensions are specific
to each package manager and will need to be converted to uv's format. See the documentation on
[project dependencies](../concepts/dependencies.md) for more details.

## Project structure

A project consists of a few important parts that work together and allow uv to manage your project.
Along with the files created by `uv init`, uv will create a virtual environment and `uv.lock` file
in the root of your project the first time you run a project command.

### `pyproject.toml`

The `pyproject.toml` contains metadata about your project:

```toml title="pyproject.toml"
[project]
name = "hello-world"
version = "0.1.0"
description = "Add your description here"
readme = "README.md"
dependencies = []

[tool.uv]
dev-dependencies = []
```

This is where you specify dependencies, as well as details about the project such as its description
or license. You can edit this file manually, or use commands like `uv add` and `uv remove` to manage
your project through the CLI.

!!! tip

    See the official [`pyproject.toml` guide](https://packaging.python.org/en/latest/guides/writing-pyproject-toml/)
    for more details on getting started with the `pyproject.toml` format.

### `.venv`

The `.venv` folder contains your project's virtual environment, a Python environment that is
isolated from the rest of your system. This is where uv will install your project's dependencies.

See the [project environment](../concepts/projects.md#project-environments) documentation for more
details.

### `uv.lock`

`uv.lock` is a cross-platform lockfile that contains exact information about your project's
dependencies. Unlike the `pyproject.toml` which is used to specify the broad requirements of your
project, the lockfile contains the exact resolved versions that are installed in the project
environment. This file should be checked into version control, allowing for consistent and
reproducible installations across machines.

`uv.lock` is a human-readable TOML file but is managed by uv and should not be edited manually.

See the [lockfile](../concepts/projects.md#lockfile) documentation for more details.

## Managing dependencies

You can add dependencies to your `pyproject.toml` with the `uv add` command. This will also update
the lockfile and project environment:

```console
$ uv add requests
```

You can also specify version constraints or alternative sources:

```console
$ # Specify a version constraint
$ uv add 'requests==2.31.0'

$ # Add a git dependency
$ uv add requests --git https://github.com/psf/requests
```

To remove a package, you can use `uv remove`:

```console
$ uv remove requests
```

See the documentation on [managing dependencies](../concepts/projects.md#managing-dependencies) for
more details.

## Running commands

`uv run` can be used to run arbitrary scripts or commands in your project environment.

Prior to every `uv run` invocation, uv will verify that the lockfile is up-to-date with the
`pyproject.toml`, and that the environment is up-to-date with the lockfile, keeping your project
in-sync without the need for manual intervention. `uv run` guarantees that your command is run in a
consistent, locked environment.

For example, to use `flask`:

```console
$ uv add flask
$ uv run -- flask run -p 3000
```

Or, to run a script:

```python title="example.py"
# Require a project dependency
import flask

print("hello world")
```

```console
$ uv run example.py
```

Alternatively, you can use `uv sync` to manually update the environment then activate it before
executing a command:

```console
$ uv sync
$ source .venv/bin/activate
$ flask run -p 3000
$ python example.py
```

!!! note

    The virtual environment must be active to run scripts and commands in the project without `uv run`. Virtual environment activation differs per shell and platform.

See the documentation on [running commands](../concepts/projects.md#running-commands) and
[running scripts](../concepts/projects.md#running-scripts) in projects for more details.

## Next steps

To learn more about working on projects with uv, see the [Projects concept](../concepts/projects.md)
page and the [command reference](../reference/cli.md#uv).

Or, read on to learn how to [publish your project as a package](./publish.md).
