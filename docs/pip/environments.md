# Python environments

Each Python installation has an environment that is active when Python is used. Packages can be installed into an environment to make their modules available from your Python scripts. Generally, it is considered best practice not to modify a Python installation's environment. This is especially important for Python installations that come with the operating system which often manage the packages themselves. A virtual environment is a lightweight way to isolate packages from a Python installation's environment. Unlike `pip`, uv requires using a virtual environment by default.

## Creating a virtual environment

uv supports creating virtual environments:

```bash
# Create a virtual environment at `.venv`
uv venv
```

A specific name or path can be specified:

```bash
# Create a virtual environment at `my-name`
uv venv my-name
```

A Python version can be requested:

```bash
# Create a virtual environment with Python 3.11
uv venv --python 3.11
```

Note this requires the requested Python version to be available on the system.
However, in preview mode, [uv will download Python for you](../preview/toolchains.md).
See the [python request](../python/requests.md) documentation for more details on requesting Python versions.

## Using a virtual environment

When using the default virtual environment name, uv will automatically find and use the virtual environment during subsequent invocations.

```bash
uv venv

# Install a package in the new virtual environment
uv pip install ruff
```

The virtual environment can be "activated" to make its packages available:

```bash
# On macOS and Linux.
source .venv/bin/activate

# On Windows.
.venv\Scripts\activate
```

## Using arbitrary Python environments

Since uv has no dependency on Python, it can install into virtual environments other than
its own. For example, setting `VIRTUAL_ENV=/path/to/venv` will cause uv to install into
`/path/to/venv`, regardless of where uv is installed. Note that if `VIRTUAL_ENV` is set to
a directory that is **not** a [PEP 405 compliant](https://peps.python.org/pep-0405/#specification)
virtual environment, it will be ignored.

uv can also install into arbitrary, even non-virtual environments, with the `--python` argument
provided to `uv pip sync` or `uv pip install`. For example, `uv pip install --python=/path/to/python`
will install into the environment linked to the `/path/to/python` interpreter.

For convenience, `uv pip install --system` will install into the system Python environment.
Using `--system` is roughly equivalent to `uv pip install --python=$(which python)`,
but note that executables that are linked to virtual environments will be skipped.
Although we generally recommend using virtual environments for dependency management,
`--system` is appropriate in continuous integration and containerized environments.

The `--system` flag is also used to opt in to mutating system environments. For example, the
the `--python` argument can be used to request a Python version (e.g., `--python 3.12`), and uv will
search for an interpreter that meets the request. If uv finds a system interpreter (e.g., `/usr/lib/python3.12`),
then the `--system` flag is required to allow modification of this non-virtual Python environment.
Without the `--system` flag, uv will ignore any interpreters that are not in virtual environments.
Conversely, when the `--system` flag is provided, uv will ignore any interpreters that _are_
in virtual environments.

Installing into system Python across platforms and distributions is notoriously difficult. uv
supports the common cases, but will not work in all cases. For example, installing into system
Python on Debian prior to Python 3.10 is unsupported due to the [distribution's patching
of `distutils` (but not `sysconfig`)](https://ffy00.github.io/blog/02-python-debian-and-the-install-locations/).
While we always recommend the use of virtual environments, uv considers them to be required in
these non-standard environments.

If uv is installed in a Python environment, e.g., with `pip`, it can still be used to modify
other environments. However, when invoked with `python -m uv`, uv will default to using the parent
interpreter's environment. Invoking uv via Python adds startup overhead and is not recommended for
general usage.
