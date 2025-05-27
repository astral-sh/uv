# Python versions

A Python version is composed of a Python interpreter (i.e. the `python` executable), the standard
library, and other supporting files.

## Managed and system Python installations

Since it is common for a system to have an existing Python installation, uv supports
[discovering](#discovery-of-python-versions) Python versions. However, uv also supports
[installing Python versions](#installing-a-python-version) itself. To distinguish between these two
types of Python installations, uv refers to Python versions it installs as _managed_ Python
installations and all other Python installations as _system_ Python installations.

!!! note

    uv does not distinguish between Python versions installed by the operating system vs those
    installed and managed by other tools. For example, if a Python installation is managed with
    `pyenv`, it would still be considered a _system_ Python version in uv.

## Requesting a version

A specific Python version can be requested with the `--python` flag in most uv commands. For
example, when creating a virtual environment:

```console
$ uv venv --python 3.11.6
```

uv will ensure that Python 3.11.6 is available — downloading and installing it if necessary — then
create the virtual environment with it.

The following Python version request formats are supported:

- `<version>` (e.g., `3`, `3.12`, `3.12.3`)
- `<version-specifier>` (e.g., `>=3.12,<3.13`)
- `<implementation>` (e.g., `cpython` or `cp`)
- `<implementation>@<version>` (e.g., `cpython@3.12`)
- `<implementation><version>` (e.g., `cpython3.12` or `cp312`)
- `<implementation><version-specifier>` (e.g., `cpython>=3.12,<3.13`)
- `<implementation>-<version>-<os>-<arch>-<libc>` (e.g., `cpython-3.12.3-macos-aarch64-none`)

Additionally, a specific system Python interpreter can be requested with:

- `<executable-path>` (e.g., `/opt/homebrew/bin/python3`)
- `<executable-name>` (e.g., `mypython3`)
- `<install-dir>` (e.g., `/some/environment/`)

By default, uv will automatically download Python versions if they cannot be found on the system.
This behavior can be
[disabled with the `python-downloads` option](#disabling-automatic-python-downloads).

### Python version files

The `.python-version` file can be used to create a default Python version request. uv searches for a
`.python-version` file in the working directory and each of its parents. If none is found, uv will
check the user-level configuration directory. Any of the request formats described above can be
used, though use of a version number is recommended for interoperability with other tools.

A `.python-version` file can be created in the current directory with the
[`uv python pin`](../reference/cli.md/#uv-python-pin) command.

A global `.python-version` file can be created in the user configuration directory with the
[`uv python pin --global`](../reference/cli.md/#uv-python-pin) command.

Discovery of `.python-version` files can be disabled with `--no-config`.

uv will not search for `.python-version` files beyond project or workspace boundaries (with the
exception of the user configuration directory).

## Installing a Python version

uv bundles a list of downloadable CPython and PyPy distributions for macOS, Linux, and Windows.

!!! tip

    By default, Python versions are automatically downloaded as needed without using
    `uv python install`.

To install a Python version at a specific version:

```console
$ uv python install 3.12.3
```

To install the latest patch version:

```console
$ uv python install 3.12
```

To install a version that satisfies constraints:

```console
$ uv python install '>=3.8,<3.10'
```

To install multiple versions:

```console
$ uv python install 3.9 3.10 3.11
```

To install a specific implementation:

```console
$ uv python install pypy
```

All of the [Python version request](#requesting-a-version) formats are supported except those that
are used for requesting local interpreters such as a file path.

By default `uv python install` will verify that a managed Python version is installed or install the
latest version. If a `.python-version` file is present, uv will install the Python version listed in
the file. A project that requires multiple Python versions may define a `.python-versions` file. If
present, uv will install all of the Python versions listed in the file.

!!! important

    The available Python versions are frozen for each uv release. To install new Python versions,
    you may need upgrade uv.

### Installing Python executables

!!! important

    Support for installing Python executables is in _preview_, this means the behavior is experimental
    and subject to change.

To install Python executables into your `PATH`, provide the `--preview` option:

```console
$ uv python install 3.12 --preview
```

This will install a Python executable for the requested version into `~/.local/bin`, e.g., as
`python3.12`.

!!! tip

    If `~/.local/bin` is not in your `PATH`, you can add it with `uv tool update-shell`.

To install `python` and `python3` executables, include the `--default` option:

```console
$ uv python install 3.12 --default --preview
```

When installing Python executables, uv will only overwrite an existing executable if it is managed
by uv — e.g., if `~/.local/bin/python3.12` exists already uv will not overwrite it without the
`--force` flag.

uv will update executables that it manages. However, it will prefer the latest patch version of each
Python minor version by default. For example:

```console
$ uv python install 3.12.7 --preview  # Adds `python3.12` to `~/.local/bin`
$ uv python install 3.12.6 --preview  # Does not update `python3.12`
$ uv python install 3.12.8 --preview  # Updates `python3.12` to point to 3.12.8
```

## Project Python versions

uv will respect Python requirements defined in `requires-python` in the `pyproject.toml` file during
project command invocations. The first Python version that is compatible with the requirement will
be used, unless a version is otherwise requested, e.g., via a `.python-version` file or the
`--python` flag.

## Viewing available Python versions

To list installed and available Python versions:

```console
$ uv python list
```

To filter the Python versions, provide a request, e.g., to show all Python 3.13 interpreters:

```console
$ uv python list 3.13
```

Or, to show all PyPy interpreters:

```console
$ uv python list pypy
```

By default, downloads for other platforms and old patch versions are hidden.

To view all versions:

```console
$ uv python list --all-versions
```

To view Python versions for other platforms:

```console
$ uv python list --all-platforms
```

To exclude downloads and only show installed Python versions:

```console
$ uv python list --only-installed
```

See the [`uv python list`](../reference/cli.md#uv-python-list) reference for more details.

## Finding a Python executable

To find a Python executable, use the `uv python find` command:

```console
$ uv python find
```

By default, this will display the path to the first available Python executable. See the
[discovery rules](#discovery-of-python-versions) for details about how executables are discovered.

This interface also supports many [request formats](#requesting-a-version), e.g., to find a Python
executable that has a version of 3.11 or newer:

```console
$ uv python find '>=3.11'
```

By default, `uv python find` will include Python versions from virtual environments. If a `.venv`
directory is found in the working directory or any of the parent directories or the `VIRTUAL_ENV`
environment variable is set, it will take precedence over any Python executables on the `PATH`.

To ignore virtual environments, use the `--system` flag:

```console
$ uv python find --system
```

## Discovery of Python versions

When searching for a Python version, the following locations are checked:

- Managed Python installations in the `UV_PYTHON_INSTALL_DIR`.
- A Python interpreter on the `PATH` as `python`, `python3`, or `python3.x` on macOS and Linux, or
  `python.exe` on Windows.
- On Windows, the Python interpreters in the Windows registry and Microsoft Store Python
  interpreters (see `py --list-paths`) that match the requested version.

In some cases, uv allows using a Python version from a virtual environment. In this case, the
virtual environment's interpreter will be checked for compatibility with the request before
searching for an installation as described above. See the
[pip-compatible virtual environment discovery](../pip/environments.md#discovery-of-python-environments)
documentation for details.

When performing discovery, non-executable files will be ignored. Each discovered executable is
queried for metadata to ensure it meets the [requested Python version](#requesting-a-version). If
the query fails, the executable will be skipped. If the executable satisfies the request, it is used
without inspecting additional executables.

When searching for a managed Python version, uv will prefer newer versions first. When searching for
a system Python version, uv will use the first compatible version — not the newest version.

If a Python version cannot be found on the system, uv will check for a compatible managed Python
version download.

### Python pre-releases

Python pre-releases will not be selected by default. Python pre-releases will be used if there is no
other available installation matching the request. For example, if only a pre-release version is
available it will be used but otherwise a stable release version will be used. Similarly, if the
path to a pre-release Python executable is provided then no other Python version matches the request
and the pre-release version will be used.

If a pre-release Python version is available and matches the request, uv will not download a stable
Python version instead.

## Disabling automatic Python downloads

By default, uv will automatically download Python versions when needed.

The [`python-downloads`](../reference/settings.md#python-downloads) option can be used to disable
this behavior. By default, it is set to `automatic`; set to `manual` to only allow Python downloads
during `uv python install`.

!!! tip

    The `python-downloads` setting can be set in a
    [persistent configuration file](../configuration/files.md) to change the default behavior, or
    the `--no-python-downloads` flag can be passed to any uv command.

## Requiring or disabling managed Python versions

By default, uv will attempt to use Python versions found on the system and only download managed
Python versions when necessary. To ignore system Python versions, and only use managed Python
versions, use the `--managed-python` flag:

```console
$ uv python list --managed-python
```

Similarly, to ignore managed Python versions and only use system Python versions, use the
`--no-managed-python` flag:

```console
$ uv python list --no-managed-python
```

To change uv's default behavior in a configuration file, use the
[`python-preference` setting](#adjusting-python-version-preferences).

## Adjusting Python version preferences

The [`python-preference`](../reference/settings.md#python-preference) setting determines whether to
prefer using Python installations that are already present on the system, or those that are
downloaded and installed by uv.

By default, the `python-preference` is set to `managed` which prefers managed Python installations
over system Python installations. However, system Python installations are still preferred over
downloading a managed Python version.

The following alternative options are available:

- `only-managed`: Only use managed Python installations; never use system Python installations.
  Equivalent to `--managed-python`.
- `system`: Prefer system Python installations over managed Python installations.
- `only-system`: Only use system Python installations; never use managed Python installations.
  Equivalent to `--no-managed-python`.

!!! note

    Automatic Python version downloads can be [disabled](#disabling-automatic-python-downloads)
    without changing the preference.

## Python implementation support

uv supports the CPython, PyPy, and GraalPy Python implementations. If a Python implementation is not
supported, uv will fail to discover its interpreter.

The implementations may be requested with either the long or short name:

- CPython: `cpython`, `cp`
- PyPy: `pypy`, `pp`
- GraalPy: `graalpy`, `gp`

Implementation name requests are not case sensitive.

See the [Python version request](#requesting-a-version) documentation for more details on the
supported formats.

## Managed Python distributions

uv supports downloading and installing CPython and PyPy distributions.

### CPython distributions

As Python does not publish official distributable CPython binaries, uv instead uses pre-built
distributions from the Astral
[`python-build-standalone`](https://github.com/astral-sh/python-build-standalone) project.
`python-build-standalone` is also is used in many other Python projects, like
[Rye](https://github.com/astral-sh/rye), [Mise](https://mise.jdx.dev/lang/python.html), and
[bazelbuild/rules_python](https://github.com/bazelbuild/rules_python).

The uv Python distributions are self-contained, highly-portable, and performant. While Python can be
built from source, as in tools like `pyenv`, doing so requires preinstalled system dependencies, and
creating optimized, performant builds (e.g., with PGO and LTO enabled) is very slow.

These distributions have some behavior quirks, generally as a consequence of portability; see the
[`python-build-standalone` quirks](https://gregoryszorc.com/docs/python-build-standalone/main/quirks.html)
documentation for details. Additionally, some platforms may not be supported (e.g., distributions
are not yet available for musl Linux on ARM).

### PyPy distributions

PyPy distributions are provided by the PyPy project.
