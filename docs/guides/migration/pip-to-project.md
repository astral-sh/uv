# Migrating from pip to a uv project

This guide will discuss converting from a `pip` and `pip-tools` workflow centered on `requirements`
files to uv's project workflow using a `pyproject.toml` and `uv.lock` file.

!!! note

    If you're looking to migrate from `pip` and `pip-tools` to uv's drop-in interface or from an
    existing workflow where you're already using a `pyproject.toml`, those guides are not yet
    written. See [#5200](https://github.com/astral-sh/uv/issues/5200) to track progress.

We'll start with an overview of developing with `pip`, then discuss migrating to uv.

!!! tip

    If you're familiar with the ecosystem, you can jump ahead to the
    [requirements file import](#importing-requirements-files) instructions.

## Project dependencies

When you want to use a package in your project, you need to install it first. `pip` supports
imperative installation of packages, e.g.:

```console
$ pip install fastapi
```

This installs the package into the environment that `pip` is installed in. This may be a virtual
environment, or, the global environment of your system's Python installation.

Then, you can run a Python script that requires the package:

```python title="example.py"
import fastapi
```

It's best practice to create a virtual environment for each project, to avoid mixing packages
between them. For example:

```console
$ python -m venv
$ source .venv/bin/activate
$ pip ...
```

We will revisit this topic in the [project environments section](#project-environments) below.

## Requirements files

When sharing projects with others, it's useful to declare all the packages you require upfront.
`pip` supports installing requirements from a file, e.g.:

```python title="requirements.txt"
fastapi
```

```console
$ pip install -r requirements.txt
```

Notice above that `fastapi` is not "locked" to a specific version — each person working on the
project may have a different version of `fastapi` installed. `pip-tools` was created to improve this
experience.

When using `pip-tools`, requirements files specify both the dependencies for your project and lock
dependencies to a specific version — the file extension is used to differentiate between the two.
For example, if you require `fastapi` and `pydantic`, you'd specify these in a `requirements.in`
file:

```python title="requirements.in"
fastapi
pydantic>2
```

Notice there's a version constraint on `pydantic` — this means only `pydantic` versions later than
`2.0.0` can be used. In contrast, `fastapi` does not have a version constraint — any version can be
used.

These dependencies can be compiled into a `requirements.txt` file:

```console
$ pip compile requirements.in -o requirements.txt
```

```python title="requirements.txt"
annotated-types==0.7.0
    # via pydantic
anyio==4.8.0
    # via starlette
fastapi==0.115.11
    # via -r requirements.in
idna==3.10
    # via anyio
pydantic==2.10.6
    # via
    #   -r requirements.in
    #   fastapi
pydantic-core==2.27.2
    # via pydantic
sniffio==1.3.1
    # via anyio
starlette==0.46.1
    # via fastapi
typing-extensions==4.12.2
    # via
    #   fastapi
    #   pydantic
    #   pydantic-core
```

Here, all the versions constraints are _exact_. Only a single version of each package can be used.
The above example was generated with `uv pip compile`, but could also be generated with
`pip-compile` from `pip-tools`.

Though less common, the `requirements.txt` can also be generated using `pip freeze`, by first
installing the input dependencies into the environment then exporting the installed versions:

```console
$ pip install -r requirements.in
$ pip freeze > requirements.txt
```

```python title="requirements.txt"
annotated-types==0.7.0
anyio==4.8.0
fastapi==0.115.11
idna==3.10
pydantic==2.10.6
pydantic-core==2.27.2
sniffio==1.3.1
starlette==0.46.1
typing-extensions==4.12.2
```

After compiling dependencies into a locked set of versions, these files are committed to version
control and distributed with the project.

Then, when someone wants to use the project, they install from the requirements file:

```console
$ pip install -r requirements.txt
```

<!--- TODO: Discuss equivalent commands for `uv pip compile` and `pip compile` -->

### Development dependencies

The requirements file format can only describe a single set of dependencies at once. This means if
you have additional _groups_ of dependencies, such as development dependencies, they need separate
files. For example, we'll create a `-dev` dependency file:

```python title="requirements-dev.in"
-r requirements.in
-c requirements.txt

pytest
```

Notice the base requirements are included with `-r requirements.in`. This ensures your development
environment considers _all_ of the dependencies together. The `-c requirements.txt` _constrains_ the
package version to ensure that the `requirements-dev.txt` uses the same versions as
`requirements.txt`.

!!! note

    It's common to use `-r requirements.txt` directly instead of using both
    `-r requirements.in`, and `-c requirements.txt`. There's no difference in the resulting package
    versions, but using both files produces annotations which allow you to determine which
    dependencies are _direct_ (annotated with `-r requirements.in`) and which are _indirect_ (only
    annotated with `-c requirements.txt`).

The compiled development dependencies look like:

```python title="requirements-dev.txt"
annotated-types==0.7.0
    # via
    #   -c requirements.txt
    #   pydantic
anyio==4.8.0
    # via
    #   -c requirements.txt
    #   starlette
fastapi==0.115.11
    # via
    #   -c requirements.txt
    #   -r requirements.in
idna==3.10
    # via
    #   -c requirements.txt
    #   anyio
iniconfig==2.0.0
    # via pytest
packaging==24.2
    # via pytest
pluggy==1.5.0
    # via pytest
pydantic==2.10.6
    # via
    #   -c requirements.txt
    #   -r requirements.in
    #   fastapi
pydantic-core==2.27.2
    # via
    #   -c requirements.txt
    #   pydantic
pytest==8.3.5
    # via -r requirements-dev.in
sniffio==1.3.1
    # via
    #   -c requirements.txt
    #   anyio
starlette==0.46.1
    # via
    #   -c requirements.txt
    #   fastapi
typing-extensions==4.12.2
    # via
    #   -c requirements.txt
    #   fastapi
    #   pydantic
    #   pydantic-core
```

As with the base dependency files, these are committed to version control and distributed with the
project. When someone wants to work on the project, they'll install from the requirements file:

```console
$ pip install -r requirements-dev.txt
```

### Platform-specific dependencies

When compiling dependencies with `pip` or `pip-tools`, the result is only usable on the same
platform as it is generated on. This poses a problem for projects which need to be usable on
multiple platforms, such as Windows and macOS.

For example, take a simple dependency:

```python title="requirements.in"
tqdm
```

On Linux, this compiles to:

```python title="requirements-linux.txt"
tqdm==4.67.1
    # via -r requirements.in
```

While on Windows, this compiles to:

```python title="requirements-win.txt"
colorama==0.4.6
    # via tqdm
tqdm==4.67.1
    # via -r requirements.in
```

`colorama` is a Windows-only dependency of `tqdm`.

When using `pip` and `pip-tools`, a project needs to declare a requirements lock file for each
supported platform.

!!! note

        uv's resolver can compile dependencies for multiple platforms at once (see ["universal resolution"](../../concepts/resolution.md#universal-resolution)),
        allowing you to use a single `requirements.txt` for all platforms:

        ```console
        $ uv pip compile --universal requirements.in
        ```

        ```python title="requirements.txt"
        colorama==0.4.6 ; sys_platform == 'win32'
            # via tqdm
        tqdm==4.67.1
            # via -r requirements.in
        ```

        This resolution mode is also used when using a `pyproject.toml` and `uv.lock`.

## The `pyproject.toml`

The `pyproject.toml` is a standardized file for Python project metadata. It replaces
`requirements.in` files, allowing you to represent arbitrary groups of project dependencies. It also
provides a centralized location for metadata about your project, such as the build system or tool
settings.

<!-- TODO: Link to the official docs on this or write more -->

We can translate the example `requirements.in` and `requirements-dev.in` files to a `pyproject.toml`
as follows:

```toml title="pyproject.toml"
[project]
name = "example"
version = "0.0.1"
dependencies = [
    "fastapi",
    "pydantic>2"
]

[dependency-groups]
dev = ["pytest"]
```

## `uv.lock`

uv uses the `uv.lock` file to lock package versions. The format of this file is specific to uv,
allowing uv to support advanced features. It replaces `requirements.txt` files. Unlike
`requirements.txt` files, the `uv.lock` file can represent arbitrary groups of dependencies, so
multiple files are not needed to lock development dependencies.

To learn more, see the [lockfile](../../concepts/projects/layout.md#the-lockfile) documentation.

## Importing requirements files

The simplest way to import requirements is with `uv add`:

```console
$ uv add -r requirements.in
```

However, there is some nuance to this transition. Notice we used the `requirements.in` file, which
does not pin to exact versions of packages so uv will solve for new versions of these packages. You
may want to continue using your previously locked versions from your `requirements.txt` so, when
switching over to uv, none of your dependency versions change.

The solution is to add your locked versions as _constraints_. uv supports using these on `add` to
preserve locked versions:

```console
$ uv add -r requirements.in -c requirements.txt
```

Your existing versions will be retained when producing a `uv.lock` file.

### Importing platform-specific constraints

If your platform-specific dependencies have been compiled into separate files, you can still
transition to a universal lockfile. However, you cannot just use `-c` to specify constraints from
your existing platform-specific `requirements.txt` files because they do not include markers
describing the environment and will consequently conflict.

To add the necessary markers, use `uv pip compile` to convert your existing files. For example,
given the following:

```python title="requirements-win.txt"
colorama==0.4.6
    # via tqdm
tqdm==4.67.1
    # via -r requirements.in
```

The markers can be added with:

```console
$ uv pip compile requirements.in -o requirements-win.txt --python-platform windows --no-strip-markers
```

Notice the resulting output includes a Windows marker on `colorama`:

```python title="requirements-win.txt"
colorama==0.4.6 ; sys_platform == 'win32'
    # via tqdm
tqdm==4.67.1
    # via -r requirements.in
```

When using `-o`, uv will constrain the versions to match the existing output file, if it can.

Markers can be added for other platforms by changing the `--python-platform` and `-o` values for
each requirements file you need to import, e.g., to `linux` and `macos`.

Once each `requirements.txt` file has been transformed, the dependencies can be imported to the
`pyproject.toml` and `uv.lock` with `uv add`:

```console
$ uv add -r requirements.in -c requirements-win.txt -c requirements-linux.txt
```

### Importing development dependency files

As discussed in the [development dependencies](#development-dependencies) section, it's common to
have groups of dependencies for development purposes.

To import development dependencies, use the `--dev` flag during `uv add`:

```console
$ uv add --dev -r requirements-dev.in -c requirements-dev.txt
```

If the `requirements-dev.in` includes the parent `requirements.in` via `-r`, it will need to be
stripped to avoid adding the base requirements to the `dev` dependency group. The following example
uses `sed` to strip lines that start with `-r`, then pipes the result to `uv add`:

```console
$ sed '/^-r /d' requirements-dev.in | uv add --dev -r - -c requirements-dev.txt
```

In addition to the `dev` dependency group, uv supports arbitrary group names. For example, if you
also have a dedicated set of dependencies for building your documentation, those can be imported to
a `docs` group:

```console
$ uv add -r requirements-docs.in -c requirements-docs.txt --group docs
```

## Project environments

Unlike `pip`, uv is not centered around the concept of an "active" virtual environment. Instead, uv
uses a dedicated virtual environment for each project in a `.venv` directory. This environment is
automatically managed, so when you run a command, like `uv add`, the environment is synced with the
project dependencies.

The preferred way to execute commands in the environment is with `uv run`. Prior to every `uv run`
invocation, uv will verify that the lockfile is up-to-date with the `pyproject.toml`, and that the
environment is up-to-date with the lockfile, keeping your project in-sync without the need for
manual intervention. `uv run` guarantees that your command is run in a consistent, locked
environment.

The project environment can also be explicitly created with `uv sync`, e.g., for use with editors.

When in projects, uv will not respect the `VIRTUAL_ENV` variable by default, though you can opt-in
to it with the `--active` flag.

To learn more, see the
[project environment](../../concepts/projects/layout.md#the-project-environment) documentation.

## Next steps

Now that you've migrated to uv, take a look at the
[project concept](../../concepts/projects/index.md) page for more details about uv projects.
