# Migrating from pip to a uv project

This guide will discuss converting from a `pip` and `pip-tools` workflow centered on `requirements`
files to uv's project workflow using a `pyproject.toml` and `uv.lock` file.

!!! tip

    If you're looking to migrate from `pip` and `pip-tools` to uv's drop-in interface instead, see
    the [`uv pip` migration guide](./pip-to-uv-pip.md) instead.

We'll start with an overview of the file formats used when developing with `pip`, then discuss
transitioning to uv.

## Requirements files

When using pip, requirements files specify both the dependencies for your project and lock
dependencies to a specific version. For example, if you require `fastapi` and `pydantic`, you'd
specify these in a `requirements.in` file:

```text title="requirements.in"
fastapi
pydantic>2
```

Notice there's a version constraint on `pydantic` — this means only `pydantic` versions later than
`2.0.0` can be used. In contrast, `fastapi` does not have a version constraint — any version can be
used.

These dependencies can be compiled into a `requirements.txt` file:

```text title="requirements.txt"
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
`pip-compile` from `pip-tools`. The `requirements.txt` can also be generated using `pip freeze`, by
first installing the input dependencies into the environment then exporting the installed versions:

```console
$ pip install -r requirements.in
$ pip freeze > requirements.txt
```

```text tite="requirements.txt"
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

The requirements file format can only a single set of dependencies at once. This means if you have
additional _groups_ of dependencies, such as development dependencies, they need separate files. For
example, we'll create a `-dev` dependency file:

```text title="requirements-dev.in"
-r requirements.in
-c requirements.txt

pytest
```

Notice the base requirements are included with `-r requirements.in`. This is common, as it ensures
your development environment considers _all_ of the dependencies together.

The compiled development dependencies look like:

```text title="requirements-dev.txt"
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

```text title="requirements.in"
tqdm
```

On Linux, this compiles to:

```text title="requirements-linux.txt"
tqdm==4.67.1
    # via -r requirements.in
```

While on Windows, this compiles to:

```text title="requirements-win.txt"
colorama==0.4.6
    # via tqdm
tqdm==4.67.1
    # via -r requirements.in
```

`colorama` is a Windows-only dependency of `tqdm`.

uv's resolver can compile dependencies for multiple platforms at once (see "universal resolution"),
allowing you to use a single `requirements.txt` for all platforms:

```console
$ uv pip compile --universal requirements.in
```

```text title="requirements.txt"
colorama==0.4.6 ; sys_platform == 'win32'
    # via tqdm
tqdm==4.67.1
    # via -r requirements.in
```

This resolution mode is also used when using a `pyproject.toml` and `uv.lock`.

## The `pyproject.toml`

## Importing requirements files

The simpest way to import requirements is with `uv add`:

```
$ uv add -r requirements.in
```

However, there is some nuance to this transition. Notice we used the `requirements.in` file, which
does not pin to exact versions of packages so uv will solve for new versions of these packages. You
may want to continue using your previously locked versions from your `requirements.txt` so when
switching over to uv none of your dependency versions change.

The solution is to add your locked versions as _constraints_. uv supports using these on `add` to
preserved locked versions:

```
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

```text title="requirements-win.txt"
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

```text title="requirements-win.txt"
colorama==0.4.6 ; sys_platform == 'win32'
    # via tqdm
tqdm==4.67.1
    # via -r requirements.in
```

When using `-o`, uv will constrain the versions to match the existing output file if it can.

Markers can be added for other platforms by changing the `--python-platform` and `-o` values for
each requirements file you need to import, e.g., to `linux` and `macos`.

Once each `requirements.txt` file has been transformed, the dependencies can be imported to the
`pyproject.toml` and `uv.lock` with `uv add`:

```console
$ uv add -r requirements.in -c requirements-win.txt -c requirements-linux.txt
```
