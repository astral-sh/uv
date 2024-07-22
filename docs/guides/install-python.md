# Installing Python

If Python is already installed on your system, uv will [detect and use](#using-an-existing-python-installation) it without configuration. However, uv can also install and manage Python versions for you.

To install the latest Python version:

```console
$ uv python install
```

This will install a uv managed Python version even if there is already a Python installation on your system.

!!! note

    Python does not publish official distributable binaries so uv uses third-party distributions from the [`python-build-standalone`](https://github.com/indygreg/python-build-standalone) project. The project is partially maintained by the uv maintainers and is used by many other Python projects. See the [Python distributions](../python-versions.md#python-distributions) documentation for more details.


<!-- TODO(zanieb): Restore when Python shim management is added
Note that when an automatic Python installation occurs, the `python` command will not be added to the shell. Use `uv python install-shim` to ensure the `python` shim is installed.

Once Python is installed, it can be invoked via `python`:

```console
$ python --version
```

To prevent uv from managing Python system-wide, provide the `--no-shim` option during installation.
-->

Once Python is installed, it will be used by `uv` commands automatically.

## Installing a specific version

To install a specific Python version:

```console
$ uv python install 3.12
```

See the [`python install`](../python-versions.md#installing-a-python-version) documentation for more details.

## Viewing Python installations

To view available and installed Python versions:

```console
$ uv python list
```

See the [`python list`](../python-versions.md#viewing-available-python-versions) documentation for more details.

<!--TODO(zanieb): The above should probably link to a CLI reference and that content should be moved out of that file -->

## Automatic Python downloads

Note that Python does not need to be explicitly installed to use uv. By default, uv will automatically download Python versions when they are required. For example, the following would download Python 3.12 if it was not installed:

```console
$ uv run --python 3.12 python -c 'print("hello world")'
```

Even if a specific Python version is not requested, uv will download the latest version on demand. For example, the following will create a new virtual environment and download a managed Python version if one hasn't been installed yet:

```console
$ uv venv --python-preference only-managed
```

<!-- TODO(zanieb): Restore when Python shim management is added
Note that when an automatic Python installation occurs, the `python` command will not be added to the shell. Use `uv python install-shim` to ensure the `python` shim is installed.
-->

## Using an existing Python installation

uv will also use an existing Python installation if already present on your system. There is no configuration necessary for this behavior: uv will use the system Python if it satisfies the requirements of the command invocation. See the [Python discovery](../python-versions.md#discovery-order) documentation for details.

To force uv to use the system Python, provide the `--python-preference only-system` option. See the [Python version preference](../python-versions.md#adjusting-python-version-preferences) documentation for more details.
