**Warning: This documentation refers to experimental features that may change.**

# Toolchains

A Python toolchain is composed of a Python interpreter (i.e. the `python` executable), the standard library, and other supporting files. It is common for an operating system to come with a Python toolchain installed and there are many tools to help manage Python toolchains.

## Requesting a toolchain

uv will automatically download a toolchain if it cannot be found.

In stable commands, this behavior requires enabling preview mode. For example, when creating a virtual environment:

```bash
uv venv --preview --python 3.11.6
```

uv will ensure that Python 3.11.6 is available — downloading and installing it if necessary — then create the virtual environment with it.

For commands that are in preview, like `uv sync`, preview behavior is always on.

```bash
uv sync --python 3.12.3
```

Many toolchain request formats are supported:

- `<version>` e.g. `3`, `3.12`, `3.12.3`
- `<version-specifier>` e.g. `>=3.12,<3.13`
- `<implementation>` e.g. `cpython` or `cp`
- `<implementation>@<version>` e.g. `cpython@3.12`
- `<implementation><version>` e.g. `cpython3.12` or `cp312`
- `<implementation><version-specifier>` e.g. `cpython>=3.12,<3.13`
- `<implementation>-<version>-<os>-<arch>-<libc>` e.g. `cpython-3.12.3-macos-aarch64-none`

At this time, only CPython downloads are supported. However, PyPy support is planned.

## Installing a toolchain

Sometimes it is preferable to install the toolchains before they are needed.

To install a toolchain at a specific version:

```bash
uv toolchain install 3.12.3
```

To install the latest patch version:

```bash
uv toolchain install 3.12
```

To install a version that satisfies constraints:

```bash
uv toolchain install '>=3.8,<3.10'
```

To install multiple versions:

```bash
uv toolchain install 3.9 3.10 3.11
```

## Installing project toolchains

By default `uv toolchain install` will verify that a managed toolchain is installed or install the latest version.

However, a project may define a `.python-version` file specifying the default Python toolchain to be used. If present,
uv will install the toolchain listed in the file.

Alternatively, a project that requires multiple Python versions may also define a `.python-versions` file. If present,
uv will install all of the toolchains listed in the file. This file takes precedence over the `.python-version` file.

uv will also respect Python requirements defined in a `pyproject.toml` file during project command invocations.

## Viewing available toolchains

To list installed and available toolchains:

```bash
uv toolchain list
```

By default, downloads for other platforms and old patch versions are hidden.

To view all versions:

```bash
uv toolchain list --all-versions
```

To view toolchains for other platforms:

```bash
uv toolchain list --all-platforms
```

To exclude downloads and only show installed toolchains:

```bash
uv toolchain list --only-installed
```

## Adjusting toolchain preferences

By default, uv will attempt to use Python toolchains found on the system and only download managed interpreters when necessary.
However, It's possible to adjust uv's toolchain selection preference with the `toolchain-preference` option.

- `only-managed`: Only use managed toolchains, never use system toolchains.
- `prefer-installed-managed`: Prefer installed managed toolchains, but use system toolchains if not found. If neither can be
  found, download a managed interpreter.
- `prefer-managed`: Prefer managed toolchains, even if one needs to be downloaded, but use system toolchains if found.
- `prefer-system`: Prefer system toolchains, only use managed toolchains if no system interpreter is found.
- `only-system`: Only use system toolchains, never use managed toolchains.

These options allow disabling uv's managed toolchains entirely or always using them and ignoring any existing system installations.
