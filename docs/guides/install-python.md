---
title: Installing and managing Python
description:
  A guide to using uv to install Python, including requesting specific versions, automatic
  installation, viewing installed versions, and more.
---

# Installing Python

If Python is already installed on your system, uv will
[detect and use](#using-existing-python-versions) it without configuration. However, uv can also
install and manage Python versions. uv [automatically installs](#automatic-python-downloads) missing
Python versions as needed — you don't need to install Python to get started.

## Getting started

To install the latest Python version:

```console
$ uv python install
```

!!! note

    Python does not publish official distributable binaries. As such, uv uses distributions from the Astral [`python-build-standalone`](https://github.com/astral-sh/python-build-standalone) project. See the [Python distributions](../concepts/python-versions.md#managed-python-distributions) documentation for more details.

Once Python is installed, it will be used by `uv` commands automatically.

!!! important

    When Python is installed by uv, it will not be available globally (i.e. via the `python` command).
    Support for this feature is in _preview_. See [Installing Python executables](../concepts/python-versions.md#installing-python-executables)
    for details.

    You can still use
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

To install an alternative Python implementation, e.g., PyPy:

```console
$ uv python install pypy@3.10
```

See the [`python install`](../concepts/python-versions.md#installing-a-python-version) documentation
for more details.

## Reinstalling Python

To reinstall uv-managed Python versions, use `--reinstall`, e.g.:

```console
$ uv python install --reinstall
```

This will reinstall all previously installed Python versions. Improvements are constantly being
added to the Python distributions, so reinstalling may resolve bugs even if the Python version does
not change.

## Viewing Python installations

To view available and installed Python versions:

```console
$ uv python list
```

See the [`python list`](../concepts/python-versions.md#viewing-available-python-versions)
documentation for more details.

## Automatic Python downloads

Python does not need to be explicitly installed to use uv. By default, uv will automatically
download Python versions when they are required. For example, the following would download Python
3.12 if it was not installed:

```console
$ uvx python@3.12 -c "print('hello world')"
```

Even if a specific Python version is not requested, uv will download the latest version on demand.
For example, if there are no Python versions on your system, the following will install Python
before creating a new virtual environment:

```console
$ uv venv
```

!!! tip

    Automatic Python downloads can be [easily disabled](../concepts/python-versions.md#disabling-automatic-python-downloads) if you want more control over when Python is downloaded.

<!-- TODO(zanieb): Restore when Python shim management is added
Note that when an automatic Python installation occurs, the `python` command will not be added to the shell. Use `uv python install-shim` to ensure the `python` shim is installed.
-->

## Using existing Python versions

uv will use existing Python installations if present on your system. There is no configuration
necessary for this behavior: uv will use the system Python if it satisfies the requirements of the
command invocation. See the
[Python discovery](../concepts/python-versions.md#discovery-of-python-versions) documentation for
details.

To force uv to use the system Python, provide the `--no-managed-python` flag. See the
[Python version preference](../concepts/python-versions.md#requiring-or-disabling-managed-python-versions)
documentation for more details.

## Next steps

To learn more about `uv python`, see the [Python version concept](../concepts/python-versions.md)
page and the [command reference](../reference/cli.md#uv-python).

Or, read on to learn how to [run scripts](./scripts.md) and invoke Python with uv.
