**Warning: This documentation refers to experimental features that may change.**

# Python versions

A Python installation is composed of a Python interpreter (i.e. the `python` executable), the standard library, and other supporting files. It is common for an operating system to come with a Python version installed and there are many tools to help manage Python versions.

## Requesting a version

uv will automatically download a Python version if it cannot be found.

In stable commands, this behavior requires enabling preview mode. For example, when creating a virtual environment:

```bash
uv venv --preview --python 3.11.6
```

uv will ensure that Python 3.11.6 is available — downloading and installing it if necessary — then create the virtual environment with it.

For commands that are in preview, like `uv sync`, preview behavior is always on.

```bash
uv sync --python 3.12.3
```

Many Python version request formats are supported:

- `<version>` e.g. `3`, `3.12`, `3.12.3`
- `<version-specifier>` e.g. `>=3.12,<3.13`
- `<implementation>` e.g. `cpython` or `cp`
- `<implementation>@<version>` e.g. `cpython@3.12`
- `<implementation><version>` e.g. `cpython3.12` or `cp312`
- `<implementation><version-specifier>` e.g. `cpython>=3.12,<3.13`
- `<implementation>-<version>-<os>-<arch>-<libc>` e.g. `cpython-3.12.3-macos-aarch64-none`

At this time, only CPython downloads are supported. However, PyPy support is planned.

## Installing a Python version

Sometimes it is preferable to install the Python versions before they are needed.

To install a Python version at a specific version:

```bash
uv python install 3.12.3
```

To install the latest patch version:

```bash
uv python install 3.12
```

To install a version that satisfies constraints:

```bash
uv python install '>=3.8,<3.10'
```

To install multiple versions:

```bash
uv python install 3.9 3.10 3.11
```

## Project Python versions

By default `uv python install` will verify that a managed Python version is installed or install the latest version.

However, a project may define a `.python-version` file specifying the default Python version to be used. If present,
uv will install the Python version listed in the file.

Alternatively, a project that requires multiple Python versions may also define a `.python-versions` file. If present,
uv will install all of the Python versions listed in the file. This file takes precedence over the `.python-version` file.

uv will also respect Python requirements defined in a `pyproject.toml` file during project command invocations.

## Viewing available Python versions

To list installed and available Python versions:

```bash
uv python list
```

By default, downloads for other platforms and old patch versions are hidden.

To view all versions:

```bash
uv python list --all-versions
```

To view Python versions for other platforms:

```bash
uv python list --all-platforms
```

To exclude downloads and only show installed Python versions:

```bash
uv python list --only-installed
```

## Adjusting Python version preferences

By default, uv will attempt to use Python versions found on the system and only download managed interpreters when necessary.
However, It's possible to adjust uv's Python version selection preference with the `python-preference` option.

- `only-managed`: Only use managed Python installations; never use system Python installations
- `installed`:    Prefer installed Python installations, only download managed Python installations if no system Python installation is found
- `managed`:      Prefer managed Python installations over system Python installations, even if fetching is required
- `system`:       Prefer system Python installations over managed Python installations
- `only-system`:  Only use system Python installations; never use managed Python installations

These options allow disabling uv's managed Python versions entirely or always using them and ignoring any existing system installations.

## Discovery order

When searching for a Python version, the following locations are checked:

- Managed Python versions in the `UV_PYTHON_INSTALL_DIR`.
- A Python interpreter on the `PATH` as `python3` on macOS and Linux, or `python.exe` on Windows.
- On Windows, the Python interpreter returned by `py --list-paths` that matches the requested
  version.

If a specific Python version is requested, e.g. `--python 3.7`, additional executable names are included in the search:

- A Python interpreter on the `PATH` as, e.g., `python3.7` on macOS and Linux.
