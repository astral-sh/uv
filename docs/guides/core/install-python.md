# Installing Python

If Python is already installed on your system, uv will
[detect and use](#using-an-existing-python-installation) it without configuration. However, uv can
also install and manage Python versions for you.

!!! tip

    uv will [automatically fetch Python versions](#automatic-python-downloads) as needed — you don't need to install Python to get started.

<!-- TODO(zanieb): I don't love this heading. -->

## Getting started

To install the latest Python version:

```console
$ uv python install
```

This will install a uv-managed Python version even if there is already a Python installation on your
system. If you've previously installed Python with uv, a new version will not be installed.

!!! note

    Python does not publish official distributable binaries. As such, uv uses third-party distributions from the [`python-build-standalone`](https://github.com/indygreg/python-build-standalone) project. The project is partially maintained by the uv maintainers and is used by other prominent Python projects (e.g., [Rye](https://github.com/astral-sh/rye), [Bazel](https://github.com/bazelbuild/rules_python)). See the [Python distributions](../concepts/python-versions.md#managed-python-distributions) documentation for more details.

<!-- TODO(zanieb): Restore when Python shim management is added
Note that when an automatic Python installation occurs, the `python` command will not be added to the shell. Use `uv python install-shim` to ensure the `python` shim is installed.

Once Python is installed, it can be invoked via `python`:

```console
$ python --version
```

To prevent uv from managing Python system-wide, provide the `--no-shim` option during installation.
-->

Once Python is installed, it will be used by `uv` commands automatically.

!!! important

    When Python is installed by uv, it will not be available globally (i.e. via the `python` command).
    Support for this feature is planned for a future release. In the meantime, use
    [`uv run`](../guides/scripts.md#using-different-python-versions) or
    [create and activate a virtual environment](../pip/environments.md) to use `python` directly.

## Installing a specific version

To install a specific Python version:

```console
$ uv python install 3.12
```

To install multiple Python versions:

```console
$ uv python install 3.11 3.12
```

To install an alternative Python implementation, e.g. PyPy:

```console
$ uv python install pypy@3.12
```

See the [`python install`](../concepts/python-versions.md#installing-a-python-version) documentation
for more details.

## Viewing Python installations

To view available and installed Python versions:

```console
$ uv python list
```

See the [`python list`](../concepts/python-versions.md#viewing-available-python-versions)
documentation for more details.

<!--TODO(zanieb): The above should probably link to a CLI reference and that content should be moved out of that file -->

## Automatic Python downloads

Note that Python does not need to be explicitly installed to use uv. By default, uv will
automatically download Python versions when they are required. For example, the following would
download Python 3.12 if it was not installed:

```console
$ uv run --python 3.12 python -c 'print("hello world")'
```

Even if a specific Python version is not requested, uv will download the latest version on demand.
For example, the following will create a new virtual environment and download a managed Python
version if Python is not found:

```console
$ uv venv
```

!!! tip

    Automatic Python downloads can be [easily disabled](../concepts/python-versions.md#disabling-automatic-python-downloads) if you want more control over when Python is downloaded.

<!-- TODO(zanieb): Restore when Python shim management is added
Note that when an automatic Python installation occurs, the `python` command will not be added to the shell. Use `uv python install-shim` to ensure the `python` shim is installed.
-->

## Using an existing Python installation

uv will use existing Python installations if present on your system. There is no configuration
necessary for this behavior: uv will use the system Python if it satisfies the requirements of the
command invocation. See the
[Python discovery](../concepts/python-versions.md#discovery-of-python-versions) documentation for
details.

To force uv to use the system Python, provide the `--python-preference only-system` option. See the
[Python version preference](../concepts/python-versions.md#adjusting-python-version-preferences)
documentation for more details.

## Next steps

To learn more about `uv python`, see the [Python version concept](../concepts/python-versions.md)
page and the [command reference](../reference/cli.md#uv-python).

Or, read on to learn how to [run scripts](./scripts.md) and invoke Python with uv.
