# Projects

Python projects help manage Python applications spanning multiple files.

!!! tip

    Looking for an introduction to creating a project with uv? See the [projects guide](../guides/projects.md) first.

## Project metadata

Python project metadata is defined in a `pyproject.toml` file.

!!! tip

    `uv init` can be used to create a new project. See [Creating projects](#creating-projects) for
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

## Defining entry points

uv uses the standard `[project.scripts]` table to define entry points for the project.

For example, to declare a command called `hello` that invokes the `hello` function in the
`example_package_app` module:

```toml title="pyproject.toml"
[project.scripts]
hello = "example_package_app:hello"
```

!!! important

    Using `[project.scripts]` requires a [build system](#build-systems) to be defined.

## Build systems

Projects _may_ define a `[build-system]` in the `pyproject.toml`. The build system defines how the
project should be packaged and installed.

uv uses the presence of a build system to determine if a project contains a package that should be
installed in the project virtual environment. If a build system is not defined, uv will not attempt
to build or install the project itself, just its dependencies. If a build system is defined, uv will
build and install the project into the project environment. By default, projects are installed in
[editable mode](https://setuptools.pypa.io/en/latest/userguide/development_mode.html) so changes to
the source code are reflected immediately, without re-installation.

### Configuring project packaging

uv also allows manually declaring if a project should be packaged using the
[`tool.uv.package`](../reference/settings.md#package) setting.

Setting `tool.uv.package = true` will force a project to be built and installed into the project
environment. If no build system is defined, uv will use the setuptools legacy backend.

Setting `tool.uv.package = false` will force a project package _not_ to be built and installed into
the project environment. uv will ignore a declared build system when interacting with the project.

## Creating projects

uv supports creating a project with `uv init`.

uv will create a project in the working directory, or, in a target directory by providing a name,
e.g., `uv init foo`. If there's already a project in the target directory, i.e., there's a
`pyproject.toml`, uv will exit with an error.

When creating projects, uv distinguishes between two types: [**applications**](#applications) and
[**libraries**](#libraries).

By default, uv will create a project for an application. The `--lib` flag can be used to create a
project for a library instead.

### Applications

Application projects are suitable for web servers, scripts, and command-line interfaces.

Applications are the default target for `uv init`, but can also be specified with the `--app` flag:

```console
$ uv init --app example-app
$ tree example-app
example-app
├── .python-version
├── README.md
├── hello.py
└── pyproject.toml
```

When creating an application, uv will generate a minimal `pyproject.toml`. A build system is not
defined and the source code is in the top-level directory, e.g., `hello.py`. The project does not
contain a package that will be built and installed into the project environment.

```toml title="pyproject.toml"
[project]
name = "example-app"
version = "0.1.0"
description = "Add your description here"
readme = "README.md"
requires-python = ">=3.11"
dependencies = []
```

The created script defines a `main` function with some standard boilerplate:

```python title="hello.py"
def main():
    print("Hello from example-app!")


if __name__ == "__main__":
    main()
```

And can be executed with `uv run`:

```console
$ uv run hello.py
Hello from example-project!
```

### Libraries

A library is a project that is intended to be built and distributed as a Python package, for
example, by uploading it to PyPI. A library provides functions and objects for other projects to
consume.

Libraries can be created by using the `--lib` flag:

```console
$ uv init --lib example-lib
$ tree example-lib
example-lib
├── .python-version
├── README.md
├── pyproject.toml
└── src
    └── example_lib
        ├── py.typed
        └── __init__.py
```

When creating a library, uv defines a build system and places the source code in a `src` directory.
These changes ensure that the library is isolated from any `python` invocations in the project root
and that distributed library code is well separated from the rest of the project source code. The
project includes a package at `src/example_lib` that will be built and installed into the project
environment.

```toml title="pyproject.toml"
[project]
name = "example-lib"
version = "0.1.0"
description = "Add your description here"
readme = "README.md"
requires-python = ">=3.11"
dependencies = []

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"
```

!!! note

    uv does not provide a build backend yet. `hatchling` is used by default, but there are other
    options. You may need to use the [hatch build](https://hatch.pypa.io/1.9/config/build/) options
    to configure `hatchling` for your project structure.

    Progress towards a uv build backend can be tracked in [astral-sh/uv#3957](https://github.com/astral-sh/uv/issues/3957).

The created module defines a simple API function:

```python title="__init__.py"
def hello() -> str:
    return "Hello from example-lib!"
```

And you can import and execute it using `uv run`:

```console
$ uv run python -c "import example_lib; print(example_lib.hello())"
Hello from example-lib!
```

### Packaged applications

The `--package` flag can be passed to `uv init` to create a distributable application, e.g., if you
want to publish a command-line interface via PyPI. uv will define a build backend for the project,
include a `[project.scripts]` entrypoint, and install the project package into the project
environment.

The project structure looks the same as a library:

```console
$ uv init --app --package example-packaged-app
$ tree example-packaged-app
example-packaged-app
├── .python-version
├── README.md
├── pyproject.toml
└── src
    └── example_packaged_app
        └── __init__.py
```

But the module defines a CLI function:

```python title="__init__.py"
def main() -> None:
    print("Hello from example-packaged-app!")
```

And the `pyproject.toml` includes a script entrypoint:

```toml title="pyproject.toml" hl_lines="9 10"
[project]
name = "example-packaged-app"
version = "0.1.0"
description = "Add your description here"
readme = "README.md"
requires-python = ">=3.11"
dependencies = []

[project.scripts]
example-packaged-app = "example_packaged_app:main"

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"
```

Which can be executed with `uv run`:

```console
$ uv run example-packaged-app
Hello from example-packaged-app!
```

!!! tip

    An existing application can be redefined as a distributable package by adding a build system.
    However, this may require changes to the project directory structure, depending on the build
    backend.

## Project environments

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
use [`uvx`](../guides/tools.md) or
[`uv run --with`](#running-commands-with-additional-dependencies).

!!! tip

    If you don't want uv to manage the project environment, set [`managed = false`](../reference/settings.md#managed)
    to disable automatic locking and syncing of the project. For example:

    ```toml title="pyproject.toml"
    [tool.uv]
    managed = false
    ```

By default, the project will be installed in editable mode, such that changes to the source code are
immediately reflected in the environment. `uv sync` and `uv run` both accept a `--no-editable` flag,
which instructs uv to install the project in non-editable mode. `--no-editable` is intended for
deployment use-cases, such as building a Docker container, in which the project should be included
in the deployed environment without a dependency on the originating source code.

### Configuring the project environment path

The `UV_PROJECT_ENVIRONMENT` environment variable can be used to configure the project virtual
environment path (`.venv` by default).

If a relative path is provided, it will be resolved relative to the workspace root. If an absolute
path is provided, it will be used as-is, i.e. a child directory will not be created for the
environment. If an environment is not present at the provided path, uv will create it.

This option can be used to write to the system Python environment, though it is not recommended.
`uv sync` will remove extraneous packages from the environment by default and, as such, may leave
the system in a broken state.

!!! important

    If an absolute path is provided and the setting is used across multiple projects, the
    environment will be overwritten by invocations in each project. This setting is only recommended
    for use for a single project in CI or Docker images.

!!! note

    uv does not read the `VIRTUAL_ENV` environment variable during project operations. A warning
    will be displayed if `VIRTUAL_ENV` is set to a different path than the project's environment.

## Project lockfile

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

!!! tip

    If you need to integrate uv with other tools or workflows, you can export `uv.lock` to `requirements.txt` format
    with `uv export --format requirements-txt`. The generated `requirements.txt` file can then be installed via
    `uv pip install`, or with other tools like `pip`.

    In general, we recommend against using both a `uv.lock` and a `requirements.txt` file. If you find yourself
    exporting a `uv.lock` file, consider opening an issue to discuss your use case.

### Checking if the lockfile is up-to-date

To avoid updating the lockfile during `uv sync` and `uv run` invocations, use the `--frozen` flag.

To avoid updating the environment during `uv run` invocations, use the `--no-sync` flag.

To assert the lockfile matches the project metadata, use the `--locked` flag. If the lockfile is not
up-to-date, an error will be raised instead of updating the lockfile.

### Upgrading locked package versions

By default, uv will prefer the locked versions of packages when running `uv sync` and `uv lock`.
Package versions will only change if the project's dependency constraints exclude the previous,
locked version.

To upgrade all packages:

```console
$ uv lock --upgrade
```

To upgrade a single package to the latest version, while retaining the locked versions of all other
packages:

```console
$ uv lock --upgrade-package <package>
```

To upgrade a single package to a specific version:

```console
$ uv lock --upgrade-package <package>==<version>
```

!!! note

    In all cases, upgrades are limited to the project's dependency constraints. For example, if the
    project defines an upper bound for a package then an upgrade will not go beyond that version.

### Limited resolution environments

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

### Optional dependencies

uv requires that all optional dependencies ("extras") declared by the project are compatible with
each other and resolves all optional dependencies together when creating the lockfile.

If optional dependencies declared in one group are not compatible with those in another group, uv
will fail to resolve the requirements of the project with an error.

!!! note

    There is currently no way to declare conflicting optional dependencies. See
    [astral.sh/uv#6981](https://github.com/astral-sh/uv/issues/6981) to track support.

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

!!! note

    "Updating" a dependency refers to changing the constraints for the dependency in the
    `pyproject.toml`. The locked version of the dependency will only change if necessary to
    satisfy the new constraints. To force the package version to update to the latest within
    the constraints, use `--upgrade-package <name>`, e.g.:

    ```console
    $ uv add 'httpx>0.1.0' --upgrade-package httpx
    ```

    See the [lockfile](#upgrading-locked-package-versions) section for more details on upgrading
    package versions.

Or, to change the bounds for `httpx`:

```console
$ uv add 'httpx<0.2.0'
```

To add a dependency source, e.g., to use `httpx` from GitHub during development:

```console
$ uv add git+https://github.com/encode/httpx
```

### Platform-specific dependencies

To ensure that a dependency is only installed on a specific platform or on specific Python versions,
use Python's standardized
[environment markers](https://peps.python.org/pep-0508/#environment-markers) syntax.

For example, to install `jax` on Linux, but not on Windows or macOS:

```console
$ uv add 'jax; sys_platform == "linux"'
```

The resulting `pyproject.toml` will then include the environment marker in the dependency
definition:

```toml title="pyproject.toml" hl_lines="6"
[project]
name = "project"
version = "0.1.0"
requires-python = ">=3.11"
dependencies = ["jax; sys_platform == 'linux'"]
```

Similarly, to include `numpy` on Python 3.11 and later:

```console
$ uv add 'numpy; python_version >= "3.11"'
```

See Python's [environment marker](https://peps.python.org/pep-0508/#environment-markers)
documentation for a complete enumeration of the available markers and operators.

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

## Building projects

To distribute your project to others (e.g., to upload it to an index like PyPI), you'll need to
build it into a distributable format.

Python projects are typically distributed as both source distributions (sdists) and binary
distributions (wheels). The former is typically a `.tar.gz` or `.zip` file containing the project's
source code along with some additional metadata, while the latter is a `.whl` file containing
pre-built artifacts that can be installed directly.

`uv build` can be used to build both source distributions and binary distributions for your project.
By default, `uv build` will build the project in the current directory, and place the built
artifacts in a `dist/` subdirectory:

```console
$ uv build
$ ls dist/
example-0.1.0-py3-none-any.whl
example-0.1.0.tar.gz
```

You can build the project in a different directory by providing a path to `uv build`, e.g.,
`uv build path/to/project`.

`uv build` will first build a source distribution, and then build a binary distribution (wheel) from
that source distribution.

You can limit `uv build` to building a source distribution with `uv build --sdist`, a binary
distribution with `uv build --wheel`, or build both distributions from source with
`uv build --sdist --wheel`.

`uv build` accepts `--build-constraint`, which can be used to constrain the versions of any build
requirements during the build process. When coupled with `--require-hashes`, uv will enforce that
the requirement used to build the project match specific, known hashes, for reproducibility.

For example, given the following `constraints.txt`:

```text
setuptools==68.2.2 --hash=sha256:b454a35605876da60632df1a60f736524eb73cc47bbc9f3f1ef1b644de74fd2a
```

Running the following would build the project with the specified version of `setuptools`, and verify
that the downloaded `setuptools` distribution matches the specified hash:

```console
$ uv build --build-constraint constraints.txt --require-hashes
```

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

[tool.uv]
no-build-isolation-package = ["cchardet"]
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

Some packages, like `cchardet` above, only require build dependencies for the _installation_ phase
of `uv sync`. Others, like `flash-attn`, require their build dependencies to be present even just to
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

You could run the following sequence of commands to sync `flash-attn`:

```console
$ uv venv
$ uv pip install torch
$ uv sync
```

Alternatively, you can provide the `flash-attn` metadata upfront via the
[`dependency-metadata`](../reference/settings.md#dependency-metadata) setting, thereby forgoing the
need to build the package during the dependency resolution phase. For example, to provide the
`flash-attn` metadata upfront, include the following in your `pyproject.toml`:

```toml title="pyproject.toml"
[[tool.uv.dependency-metadata]]
name = "flash-attn"
version = "2.6.3"
requires-dist = ["torch", "einops"]
```

!!! tip

    To determine the package metadata for a package like `flash-attn`, navigate to the appropriate Git repository,
    or look it up on [PyPI](https://pypi.org/project/flash-attn) and download the package's source distribution.
    The package requirements can typically be found in the `setup.py` or `setup.cfg` file.

    (If the package includes a built distribution, you can unzip it to find the `METADATA` file; however, the presence
    of a built distribution would negate the need to provide the metadata upfront, since it would already be available
    to uv.)

Once included, you can again use the two-step `uv sync` process to install the build dependencies.
Given the following `pyproject.toml`:

```toml title="pyproject.toml"
[project]
name = "project"
version = "0.1.0"
description = "..."
readme = "README.md"
requires-python = ">=3.12"
dependencies = []

[project.optional-dependencies]
build = ["torch", "setuptools", "packaging"]
compile = ["flash-attn"]

[tool.uv]
no-build-isolation-package = ["flash-attn"]

[[tool.uv.dependency-metadata]]
name = "flash-attn"
version = "2.6.3"
requires-dist = ["torch", "einops"]
```

You could run the following sequence of commands to sync `flash-attn`:

```console
$ uv sync --extra build
$ uv sync --extra build --extra compile
```
