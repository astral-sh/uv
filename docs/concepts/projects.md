# Projects

Python projects help manage Python applications spanning multiple files.

!!! tip

    Looking for an introduction to creating a project with uv? See the [projects guide](../guides/projects.md) first.

## Project metadata

Python project metadata is defined in a `pyproject.toml` file.

`uv init` can be used to create a new project, with a basic `pyproject.toml` and package definition.

A minimal project definition includes a name, version, and description:

```toml title="pyproject.toml"
[project]
name = "example"
version = "0.1.0"
description = "Add your description here"
```

Additionally, it's recommended to include a Python version requirement:

```toml title="pyproject.toml"
[project]
requires-python = ">=3.12"
```

This Python version requirement determines what syntax is valid in the project and affects the
versions of dependencies which can be used (they must support the same Python range).

The `pyproject.toml` also lists dependencies of the project. uv supports modifying the standard
dependency list from the command line with `uv add` and `uv remove`. uv also supports
[extended package sources](./dependencies.md) for advanced users.

!!! tip

    See the official [`pyproject.toml` guide](https://packaging.python.org/en/latest/guides/writing-pyproject-toml/) for more details on getting started with a `pyproject.toml`.

## Project environments

uv creates a virtual environment in a `.venv` directory next to the `pyproject.toml`. This virtual
environment contains the project and its dependencies. It is stored inside the project to make it
easy for editors to find — they need the environment to give code completions and type hints. It is
not recommended to include the `.venv` directory in version control; it is automatically excluded
from `git` with an internal `.gitignore` file.

To run a command in the project environment, use `uv run`. Alternatively the project environment can
be activated as normal for a virtual environment.

When `uv run` is invoked, it will create the project environment if it does not exist yet or ensure
it is up-to-date if it exists. The project environment can also be explicitly created with
`uv sync`.

It is _not_ recommended to modify the project environment manually, e.g., with `uv pip install`. For
project dependencies, use `uv add` to add a package to the environment. For one-off requirements,
use [`uvx`](../guides/tools.md) or
[`uv run --with`](#running-commands-with-additional-dependencies).

!!! tip

    If you don't want uv to manage the project environment, set [`managed = false`](../reference/settings.md#managed)
    to disable automatic locking and syncing of the project. For example:

    ```toml title="pyproject.toml"
    [tool.uv]
    managed = false
    ```

## Lockfile

uv creates a `uv.lock` file next to the `pyproject.toml`.

`uv.lock` is a _universal_ or _cross-platform_ lockfile that captures the packages that would be
installed across all possible Python markers such as operating system, architecture, and Python
version.

If your project supports a more limited set of platforms or Python versions, you can constrain the
set of solved platforms via the `environments` setting, which accepts a list of PEP 508 environment
markers. For example, to constrain the lockfile to macOS and Linux, and exclude Windows:

```toml title="pyproject.toml"
[tool.uv]
environments = [
    "sys_platform == 'darwin'",
    "sys_platform == 'linux'",
]
```

Entries in the `environments` setting must be disjoint (i.e., they must not overlap). For example,
`sys_platform == 'darwin'` and `sys_platform == 'linux'` are disjoint, but
`sys_platform == 'darwin'` and `python_version >= '3.9'` are not, since both could be true at the
same time.

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

To avoid updating the lockfile during `uv sync` and `uv run` invocations, use the `--frozen` flag.

To assert the lockfile is up-to-date, use the `--locked` flag. If the lockfile is not up-to-date, an
error will be raised instead of updating the lockfile.

## Managing dependencies

uv is capable of adding, updating, and removing dependencies using the CLI.

To add a dependency:

```console
$ uv add httpx
```

uv supports adding [editable dependencies](./dependencies.md#editable-dependencies),
[development dependencies](./dependencies.md#development-dependencies),
[optional dependencies](./dependencies.md#optional-dependencies), and alternative
[dependency sources](./dependencies.md#dependency-sources). See the
[dependency specification](./dependencies.md) documentation for more details.

uv will raise an error if the dependency cannot be resolved, e.g.:

```console
$ uv add 'httpx>9999'
error: Because only httpx<=9999 is available and example==0.1.0 depends on httpx>9999, we can conclude that example==0.1.0 cannot be used.
And because only example==0.1.0 is available and you require example, we can conclude that the requirements are unsatisfiable.
```

To remove a dependency:

```console
$ uv remove httpx
```

To update an existing dependency, e.g., to add a lower bound to the `httpx` version:

```console
$ uv add 'httpx>0.1.0'
```

Or, to change the bounds for `httpx`:

```console
$ uv add 'httpx<0.2.0'
```

To add a dependency source, e.g., to use `httpx` from GitHub during development:

```console
$ uv add git+https://github.com/encode/httpx
```

## Running commands

When working on a project, it is installed into virtual environment at `.venv`. This environment is
isolated from the current shell by default, so invocations that require the project, e.g.,
`python -c "import example"`, will fail. Instead, use `uv run` to run commands in the project
environment:

```console
$ uv run python -c "import example"
```

When using `run`, uv will ensure that the project environment is up-to-date before running the given
command.

The given command can be provided by the project environment or exist outside of it, e.g.:

```console
$ # Presuming the project provides `example-cli`
$ uv run example-cli foo

$ # Running a `bash` script that requires the project to be available
$ uv run bash scripts/foo.sh
```

### Running commands with additional dependencies

Additional dependencies or different versions of dependencies can be requested per invocation.

The `--with` option is used to include a dependency for the invocation, e.g., to request a different
version of `httpx`:

```console
$ uv run --with httpx==0.26.0 python -c "import httpx; print(httpx.__version__)"
0.26.0
$ uv run --with httpx==0.25.0 python -c "import httpx; print(httpx.__version__)"
0.25.0
```

The requested version will be respected regardless of the project's requirements. For example, even
if the project requires `httpx==0.24.0`, the output above would be the same.

### Running scripts

Scripts that declare inline metadata are automatically executed in environments isolated from the
project. See the [scripts guide](../guides/scripts.md#declaring-script-dependencies) for more
details.

For example, given a script:

```python title="example.py"
# /// script
# dependencies = [
#   "httpx",
# ]
# ///

import httpx

resp = httpx.get("https://peps.python.org/api/peps.json")
data = resp.json()
print([(k, v["title"]) for k, v in data.items()][:10])
```

The invocation `uv run example.py` would run _isolated_ from the project with only the given
dependencies listed.

## Projects with many packages

If working in a project composed of many packages, see the [workspaces](./workspaces.md)
documentation.

## Build isolation

By default, uv builds all packages in isolated virtual environments, as per
[PEP 517](https://peps.python.org/pep-0517/). Some packages are incompatible with build isolation,
be it intentionally (e.g., due to the use of heavy build dependencies, mostly commonly PyTorch) or
unintentionally (e.g., due to the use of legacy packaging setups).

To disable build isolation for a specific dependency, add it to the `no-build-isolation-package`
list in your `pyproject.toml`:

```toml title="pyproject.toml"
[project]
name = "project"
version = "0.1.0"
description = "..."
readme = "README.md"
requires-python = ">=3.12"
dependencies = ["cchardet"]

[tool.uv]
no-build-isolation-package = ["cchardet"]
```

Installing packages without build isolation requires that the package's build dependencies are
installed in the project environment _prior_ to installing the package itself. This can be achieved
by separating out the build dependencies and the packages that require them into distinct optional
groups. For example:

```toml title="pyproject.toml"
[project]
name = "project"
version = "0.1.0"
description = "..."
readme = "README.md"
requires-python = ">=3.12"
dependencies = []

[project.optional-dependencies]
build = ["setuptools", "cython"]
compile = ["cchardet"]
```

Given the above, a user would first sync the `build` dependencies:

```console
$ uv sync --extra build
 + cython==3.0.11
 + foo==0.1.0 (from file:///Users/crmarsh/workspace/uv/foo)
 + setuptools==73.0.1
```

Followed by the `compile` dependencies:

```console
$ uv sync --extra compile
 + cchardet==2.1.7
 - cython==3.0.11
 - setuptools==73.0.1
```

Note that `uv sync --extra compile` would, by default, uninstall the `cython` and `setuptools`
packages. To instead retain the build dependencies, include both extras in the second `uv sync`
invocation:

```console
$ uv sync --extra build
$ uv sync --extra build --extra compile
```

Some packages, like `cchardet`, only require build dependencies for the _installation_ phase of
`uv sync`. Others, like `flash-atten`, require their build dependencies to be present even just to
resolve the project's lockfile during the _resolution_ phase.

In such cases, the build dependencies must be installed prior to running any `uv lock` or `uv sync`
commands, using the lower lower-level `uv pip` API. For example, given:

```toml title="pyproject.toml"
[project]
name = "project"
version = "0.1.0"
description = "..."
readme = "README.md"
requires-python = ">=3.12"
dependencies = ["flash-attn"]

[tool.uv]
no-build-isolation-package = ["flash-attn"]
```

You could run the following sequence of commands:

```console
$ uv venv
$ uv pip install torch
$ uv sync
```
