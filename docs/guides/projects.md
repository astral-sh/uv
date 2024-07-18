# Working on projects

uv can manage the development of a Python project from the ground up.

## Creating a new project

You can create a new Python project using the `uv init` command:

```console
$ uv init hello-world
$ cd hello-world
```

Alternatively, you can initialize a project in the working directory:

```console
$ cd hello-world
$ uv init
```

This will create the following directory structure:

```
.
├── .venv
├── pyproject.toml
├── README.md
├── uv.lock
└── src
    └── hello-world
        └── __init__.py
```

### Working on an existing project

If your project already contains a standard `pyproject.toml`, you can start
using uv without any extra work. Commands like `uv add` and `uv run` will
create the lockfile and virtual environment the first time they are run.

If you are migrating from an alternative Python package manager, you may need to
edit your `pyproject.toml` manually to use uv. uv uses the `[tool.uv]` section
of the `pyproject.toml` to support non-standard features, such as development
dependencies. Alternative Python package managers may use different sections,
or a custom format altogether.

## Project structure

A project consists of a few important parts that work together and allow uv to
manage your project.

### `pyproject.toml`

The `pyproject.toml` contains metadata about your project:

```toml
[project]
name = "hello-world"
version = "0.1.0"
description = "Add your description here"
readme = "README.md"
dependencies = []

[tool.uv]
dev-dependencies = []
```

This is where you specify dependencies, as well as details about the project
such as it's description or license. You can edit this file manually, or use
commands like `uv add` and `uv remove` to manage your project through the
CLI.

### `.venv`

The `.venv` folder contains your project's virtual environment, a Python
environment that is isolated from the rest of your system. This is where uv will
install your project's dependencies.

### `uv.lock`

`uv.lock` is a lockfile that contains exact information about your project's
dependencies. Unlike the `pyproject.toml` which is used to specify the broad
requirements of your project, the lockfile contains the exact versions that are
installed in the virtual environment. This allows for consistent and
reproducible installations across machines.

`uv.lock` is a human-readable TOML file but is managed by uv and should not be
edited manually.

## Running commands

`uv run` can be used to run arbitrary scripts or commands in your project
environment. This ensures that the lockfile and virtual environment are
up-to-date before executing a given command:

```console
$ uv run python my_script.py
$ uv run flask run -p 3000
```

Alternatively, you can use `uv sync` to manually synchronize the lockfile and
virtual environment before executing a command:

```console
$ uv sync
$ python my_script.py
```

## Managing dependencies

You can add dependencies to your `pyproject.toml` with the `uv add` command.
This will also update the lockfile and virtual environment:

```console
$ uv add requests
```

You can also specify version constraints or alternative sources:

```console
# Specify a version constraint
$ uv add 'requests==2.31.0'

# Add a git dependency
$ uv add requests --git https://github.com/psf/requests
```

To remove a package, you can use `uv remove`:

```console
$ uv remove requests
```

## Next steps

See the [projects concept](../projects.md) documentation for more details about
projects.
