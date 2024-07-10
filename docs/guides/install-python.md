# Installing Python

uv can manage Python installations — Python does not need to be installed to use uv.

To install Python:

```console
$ uv python install
```

This will install a uv-managed Python version even if there is already a Python installation on the system.

Once Python is installed, it can be invoked via `python`:

```console
$ python --version
```

To prevent uv from managing Python system-wide, provide the `--no-shim` option during installation.

## Installing a specific version

To install a specific Python version:

```console
$ uv python install 3.12
```

See the [Python toolchain](../preview/toolchains.md) documentation for more details.

## Viewing Python installations

To view available and installed Python versions:

```console
$ uv python list
```

## Automatic Python downloads

Note that Python does not need to be explicitly installed to use uv. By default, uv will automatically download Python versions when they are required. For example, the following would download Python 3.12 if it was not installed:

```console
$ uv run --python 3.12 python -c 'print("hello world")'
```

Even if a specific Python version is not requested, uv will download the latest version on demand. For example, the following will create a new virtual environment and download a managed Python version if one hasn't been installed yet:

```console
$ uv venv --python-preference only-managed
```

Note that when an automatic Python installation occurs, the `python` command will not be added to the shell. Use `uv python install-shim` to ensure the `python` shim is installed.

## Using an existing Python installation

uv will also use an existing Python installation if already present on the system. There's no configuration necessary for this behavior, uv will use the system Python if it satisfies the requirements of the command invocation.

To force uv to use the system Python, provide the `--python-preference only-system` option.
