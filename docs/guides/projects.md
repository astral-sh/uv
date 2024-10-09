# Working on projects

uv supports managing Python projects, which define their dependencies in a `pyproject.toml` file.

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

uv will create the following files:

```text
.
├── .python-version
├── README.md
├── hello.py
└── pyproject.toml
```

The `hello.py` file contains a simple "Hello world" program. Try it out with `uv run`:

```console
$ uv run hello.py
Hello from hello-world!
```

## Project structure

A project consists of a few important parts that work together and allow uv to manage your project.
In addition to the files created by `uv init`, uv will create a virtual environment and `uv.lock`
file in the root of your project the first time you run a project command, i.e., `uv run`,
`uv sync`, or `uv lock`.

A complete listing would look like:

```text
.
├── .venv
│   ├── bin
│   ├── lib
│   └── pyvenv.cfg
├── .python-version
├── README.md
├── hello.py
├── pyproject.toml
└── uv.lock
```

### `pyproject.toml`

The `pyproject.toml` contains metadata about your project:

```toml title="pyproject.toml"
[project]
name = "hello-world"
version = "0.1.0"
description = "Add your description here"
readme = "README.md"
dependencies = []
```

You'll use this file to specify dependencies, as well as details about the project such as its
description or license. You can edit this file manually, or use commands like `uv add` and
`uv remove` to manage your project from the terminal.

!!! tip

    See the official [`pyproject.toml` guide](https://packaging.python.org/en/latest/guides/writing-pyproject-toml/)
    for more details on getting started with the `pyproject.toml` format.

You'll also use this file to specify uv [configuration options](../configuration/files.md) in a
[`[tool.uv]`](../reference/settings.md) section.

### `.python-version`

The `.python-version` file contains the project's default Python version. This file tells uv which
Python version to use when creating the project's virtual environment.

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

See the [lockfile](../concepts/projects.md#project-lockfile) documentation for more details.

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
$ uv add git+https://github.com/psf/requests
```

To remove a package, you can use `uv remove`:

```console
$ uv remove requests
```

To upgrade a package, run `uv lock` with the `--upgrade-package` flag:

```console
$ uv lock --upgrade-package requests
```

The `--upgrade-package` flag will attempt to update the specified package to the latest compatible
version, while keeping the rest of the lockfile intact.

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

## Building distributions

`uv build` can be used to build source distributions and binary distributions (wheel) for your
project.

By default, `uv build` will build the project in the current directory, and place the built
artifacts in a `dist/` subdirectory:

```console
$ uv build
$ ls dist/
hello-world-0.1.0-py3-none-any.whl
hello-world-0.1.0.tar.gz
```

See the documentation on [building projects](../concepts/projects.md#building-projects) for more
details.

## Next steps

To learn more about working on projects with uv, see the [Projects concept](../concepts/projects.md)
page and the [command reference](../reference/cli.md#uv).

Or, read on to learn how to [publish your project as a package](./publish.md).
