# Creating projects

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
$ uv run --directory example-lib python -c "import example_lib; print(example_lib.hello())"
Hello from example-lib!
```

You can select a different build backend template by using `--build-backend` with `hatchling`,
`flit-core`, `pdm-backend`, `setuptools`, `maturin`, or `scikit-build-core`.

```console
$ uv init --lib --build-backend maturin example-lib
$ tree example-lib
example-lib
├── .python-version
├── Cargo.toml
├── README.md
├── pyproject.toml
└── src
    ├── lib.rs
    └── example_lib
        ├── py.typed
        ├── __init__.py
        └── _core.pyi
```

And you can import and execute it using `uv run`:

```console
$ uv run --directory example-lib python -c "import example_lib; print(example_lib.hello())"
Hello from example-lib!
```

!!! tip

    Changes to `lib.rs` or `main.cpp` will require running `--reinstall` when using binary build
    backends such as `maturin` and `scikit-build-core`.

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
$ uv run --directory example-packaged-app example-packaged-app
Hello from example-packaged-app!
```

!!! tip

    An existing application can be redefined as a distributable package by adding a build system.
    However, this may require changes to the project directory structure, depending on the build
    backend.

In addition, you can further customize the build backend of a packaged application by specifying
`--build-backend` including binary build backends such as `maturin`.

```console
$ uv init --app --package --build-backend maturin example-packaged-app
$ tree example-packaged-app
example-packaged-app
├── .python-version
├── Cargo.toml
├── README.md
├── pyproject.toml
└── src
    ├── lib.rs
    └── example_packaged_app
        ├── __init__.py
        └── _core.pyi
```

Which can also be executed with `uv run`:

```console
$ uv run --directory example-packaged-app example-packaged-app
Hello from example-packaged-app!
```
