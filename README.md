# uv

[![uv](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/astral-sh/uv/main/assets/badge/v0.json)](https://github.com/astral-sh/uv)
[![image](https://img.shields.io/pypi/v/uv.svg)](https://pypi.python.org/pypi/uv)
[![image](https://img.shields.io/pypi/l/uv.svg)](https://pypi.python.org/pypi/uv)
[![image](https://img.shields.io/pypi/pyversions/uv.svg)](https://pypi.python.org/pypi/uv)
[![Actions status](https://github.com/astral-sh/uv/actions/workflows/ci.yml/badge.svg)](https://github.com/astral-sh/uv/actions)
[![Discord](https://img.shields.io/badge/Discord-%235865F2.svg?logo=discord&logoColor=white)](https://discord.gg/astral-sh)

An extremely fast Python package installer and resolver, written in Rust. Designed as a drop-in
replacement for common `pip` and `pip-tools` workflows.

<p align="center">
  <picture align="center">
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/astral-sh/uv/assets/1309177/03aa9163-1c79-4a87-a31d-7a9311ed9310">
    <source media="(prefers-color-scheme: light)" srcset="https://github.com/astral-sh/uv/assets/1309177/629e59c0-9c6e-4013-9ad4-adb2bcf5080d">
    <img alt="Shows a bar chart with benchmark results." src="https://github.com/astral-sh/uv/assets/1309177/629e59c0-9c6e-4013-9ad4-adb2bcf5080d">
  </picture>
</p>

<p align="center">
  <i>Installing the Trio dependencies with a warm cache.</i>
</p>

## Highlights

- ⚖️ Drop-in replacement for common `pip`, `pip-tools`, and `virtualenv` commands.
- ⚡️ [10-100x faster](https://github.com/astral-sh/uv/blob/main/BENCHMARKS.md) than `pip` and
  `pip-tools` (`pip-compile` and `pip-sync`).
- 💾 Disk-space efficient, with a global cache for dependency deduplication.
- 🐍 Installable via `curl`, `pip`, `pipx`, etc. uv is a static binary that can be installed without
  Rust or Python.
- 🧪 Tested at-scale against the top 10,000 PyPI packages.
- 🖥️ Support for macOS, Linux, and Windows.
- 🧰 Advanced features such as [dependency version overrides](#dependency-overrides) and
  [alternative resolution strategies](#resolution-strategy).
- ⁉️ Best-in-class error messages with a conflict-tracking resolver.
- 🤝 Support for a wide range of advanced `pip` features, including editable installs, Git
  dependencies, direct URL dependencies, local dependencies, constraints, source distributions, HTML
  and JSON indexes, and more.

uv is backed by [Astral](https://astral.sh), the creators of
[Ruff](https://github.com/astral-sh/ruff).

## Getting Started

Install uv with our standalone installers, or from [PyPI](https://pypi.org/project/uv/):

```shell
# On macOS and Linux.
curl -LsSf https://astral.sh/uv/install.sh | sh

# On Windows.
powershell -c "irm https://astral.sh/uv/install.ps1 | iex"

# For a specific version.
curl -LsSf https://astral.sh/uv/0.2.36/install.sh | sh
powershell -c "irm https://astral.sh/uv/0.2.36/install.ps1 | iex"

# With pip.
pip install uv

# With pipx.
pipx install uv

# With Homebrew.
brew install uv
```

To create a virtual environment:

```shell
uv venv  # Create a virtual environment at `.venv`.
```

To activate the virtual environment:

```shell
# On macOS and Linux.
source .venv/bin/activate

# On Windows.
.venv\Scripts\activate
```

To install a package into the virtual environment:

```shell
uv pip install flask                # Install Flask.
uv pip install -r requirements.txt  # Install from a requirements.txt file.
uv pip install -e .                 # Install the current project in editable mode.
uv pip install "package @ ."        # Install the current project from disk.
uv pip install "flask[dotenv]"      # Install Flask with "dotenv" extra.
```

To generate a set of locked dependencies:

```shell
uv pip compile requirements.in -o requirements.txt    # Read a requirements.in file.
uv pip compile pyproject.toml -o requirements.txt     # Read a pyproject.toml file.
uv pip compile setup.py -o requirements.txt           # Read a setup.py file.
echo flask | uv pip compile - -o requirements.txt     # Read from stdin.
uv pip freeze | uv pip compile - -o requirements.txt  # Lock the current environment.
```

To sync a set of locked dependencies with the virtual environment:

```shell
uv pip sync requirements.txt  # Install from a requirements.txt file.
```

uv's `pip-install` and `pip-compile` commands support many of the same command-line arguments as
existing tools, including `-r requirements.txt`, `-c constraints.txt`, `-e .` (for editable
installs), `--index-url`, and more.

## Limitations

While uv supports a large subset of the `pip` interface, it does not support the entire feature set.
In some cases, those differences are intentional; in others, they're a result of uv's early stage of
development.

For details, see our [`pip` compatibility guide](./PIP_COMPATIBILITY.md).

Like `pip-compile`, uv generates a platform-specific `requirements.txt` file (unlike, e.g., `poetry`
and `pdm`, which generate platform-agnostic `poetry.lock` and `pdm.lock` files). As such, uv's
`requirements.txt` files may not be portable across platforms and Python versions.

## Roadmap

uv is an extremely fast Python package resolver and installer, designed as a drop-in replacement for
`pip`, `pip-tools` (`pip-compile` and `pip-sync`), and `virtualenv`.

uv represents an intermediary goal in our pursuit of a
["Cargo for Python"](https://blog.rust-lang.org/2016/05/05/cargo-pillars.html#pillars-of-cargo): a
comprehensive project and package manager that is extremely fast, reliable, and easy to use.

Think: a single binary that bootstraps your Python installation and gives you everything you need to
be productive with Python, bundling not only `pip`, `pip-tools`, and `virtualenv`, but also `pipx`,
`tox`, `poetry`, `pyenv`, `ruff`, and more.

Our goal is to evolve uv into such a tool.

In the meantime, though, the narrower `pip-tools` scope allows us to solve the low-level problems
involved in building such a tool (like package installation) while shipping something immediately
useful with a minimal barrier to adoption.

## Advanced Usage

### Python discovery

uv itself does not depend on Python, but it does need to locate a Python environment to (1) install
dependencies into the environment and (2) build source distributions.

When running `pip sync` or `pip install`, uv will search for a virtual environment in the following
order:

- An activated virtual environment based on the `VIRTUAL_ENV` environment variable.
- An activated Conda environment based on the `CONDA_PREFIX` environment variable.
- A virtual environment at `.venv` in the current directory, or in the nearest parent directory.

If no virtual environment is found, uv will prompt the user to create one in the current directory
via `uv venv`.

When running `pip compile`, uv does not _require_ a virtual environment and will search for a Python
interpreter in the following order:

- An activated virtual environment based on the `VIRTUAL_ENV` environment variable.
- An activated Conda environment based on the `CONDA_PREFIX` environment variable.
- A virtual environment at `.venv` in the current directory, or in the nearest parent directory.
- The Python interpreter available as `python3` on macOS and Linux, or `python.exe` on Windows.

If a `--python-version` is provided to `pip compile` (e.g., `--python-version=3.7`), uv will search
for a Python interpreter matching that version in the following order:

- An activated virtual environment based on the `VIRTUAL_ENV` environment variable.
- An activated Conda environment based on the `CONDA_PREFIX` environment variable.
- A virtual environment at `.venv` in the current directory, or in the nearest parent directory.
- The Python interpreter available as, e.g., `python3.7` on macOS and Linux.
- The Python interpreter available as `python3` on macOS and Linux, or `python.exe` on Windows.
- On Windows, the Python interpreter returned by `py --list-paths` that matches the requested
  version.

### Installing into arbitrary Python environments

Since uv has no dependency on Python, it can install into virtual environments other than its own.
For example, setting `VIRTUAL_ENV=/path/to/venv` will cause uv to install into `/path/to/venv`,
regardless of where uv is installed. Note that if `VIRTUAL_ENV` is set to a directory that is
**not** a [PEP 405 compliant](https://peps.python.org/pep-0405/#specification) virtual environment,
it will be ignored.

uv can also install into arbitrary, even non-virtual environments, with the `--python` argument
provided to `uv pip sync` or `uv pip install`. For example,
`uv pip install --python=/path/to/python` will install into the environment linked to the
`/path/to/python` interpreter.

For convenience, `uv pip install --system` will install into the system Python environment. Using
`--system` is roughly equivalent to `uv pip install --python=$(which python)`, but note that
executables that are linked to virtual environments will be skipped. Although we generally recommend
using virtual environments for dependency management, `--system` is appropriate in continuous
integration and containerized environments.

The `--system` flag is also used to opt in to mutating system environments. For example, the
`--python` argument can be used to request a Python version (e.g., `--python 3.12`), and uv will
search for an interpreter that meets the request. If uv finds a system interpreter (e.g.,
`/usr/lib/python3.12`), then the `--system` flag is required to allow modification of this
non-virtual Python environment. Without the `--system` flag, uv will ignore any interpreters that
are not in virtual environments. Conversely, when the `--system` flag is provided, uv will ignore
any interpreters that _are_ in virtual environments.

Installing into system Python across platforms and distributions is notoriously difficult. uv
supports the common cases, but will not work in all cases. For example, installing into system
Python on Debian prior to Python 3.10 is unsupported due to the
[distribution's patching of `distutils` (but not `sysconfig`)](https://ffy00.github.io/blog/02-python-debian-and-the-install-locations/).
While we always recommend the use of virtual environments, uv considers them to be required in these
non-standard environments.

If uv is installed in a Python environment, e.g., with `pip`, it can still be used to modify other
environments. However, when invoked with `python -m uv`, uv will default to using the parent
interpreter's environment. Invoking uv via Python adds startup overhead and is not recommended for
general usage.

### Persistent configuration

uv supports persistent configuration at both the project- and user-level.

Specifically, uv will search for a `pyproject.toml` or `uv.toml` file in the current directory, or
in the nearest parent directory.

If a `pyproject.toml` file is found, uv will read configuration from the `[tool.uv.pip]` table. For
example, to set a persistent index URL, add the following to a `pyproject.toml`:

```toml
[tool.uv.pip]
index-url = "https://test.pypi.org/simple"
```

(If there is no such table, the `pyproject.toml` file will be ignored, and uv will continue
searching in the directory hierarchy.)

If a `uv.toml` file is found, uv will read from the `[pip]` table. For example:

```toml
[pip]
index-url = "https://test.pypi.org/simple"
```

uv will also discover user-level configuration at `~/.config/uv/uv.toml` (or
`$XDG_CONFIG_HOME/uv/uv.toml`) on macOS and Linux, or `%APPDATA%\uv\uv.toml` on Windows. User-level
configuration must use the `uv.toml` format, rather than the `pyproject.toml` format, as a
`pyproject.toml` is intended to define a Python _project_.

If both project- and user-level configuration are found, the settings will be merged, with the
project-level configuration taking precedence. Specifically, if a string, number, or boolean is
present in both tables, the project-level value will be used, and the user-level value will be
ignored. If an array is present in both tables, the arrays will be concatenated, with the
project-level settings appearing earlier in the merged array.

Settings provided via environment variables take precedence over persistent configuration, and
settings provided via the command line take precedence over both.

uv accepts a `--no-config` command-line argument which, when provided, disables the discovery of any
persistent configuration.

uv also accepts a `--config-file` command-line argument, which accepts a path to a `uv.toml` to use
as the configuration file. When provided, this file will be used in place of _any_ discovered
configuration files (e.g., user-level configuration will be ignored).

### Git authentication

uv allows packages to be installed from Git and supports the following schemes for authenticating
with private repositories.

Using SSH:

- `git+ssh://git@<hostname>/...` (e.g. `git+ssh://git@github.com/astral-sh/uv`)
- `git+ssh://git@<host>/...` (e.g. `git+ssh://git@github.com-key-2/astral-sh/uv`)

See the
[GitHub SSH documentation](https://docs.github.com/en/authentication/connecting-to-github-with-ssh/about-ssh)
for more details on how to configure SSH.

Using a password or token:

- `git+https://<user>:<token>@<hostname>/...` (e.g.
  `git+https://git:github_pat_asdf@github.com/astral-sh/uv`)
- `git+https://<token>@<hostname>/...` (e.g. `git+https://github_pat_asdf@github.com/astral-sh/uv`)
- `git+https://<user>@<hostname>/...` (e.g. `git+https://git@github.com/astral-sh/uv`)

When using a GitHub personal access token, the username is arbitrary. GitHub does not support
logging in with password directly, although other hosts may. If a username is provided without
credentials, you will be prompted to enter them.

If there are no credentials present in the URL and authentication is needed, uv will query the
[Git credential helper](https://git-scm.com/doc/credential-helpers).

### HTTP authentication

uv supports credentials over HTTP when querying package registries.

Authentication can come from the following sources, in order of precedence:

- The URL, e.g., `https://<user>:<password>@<hostname>/...`
- A [`netrc`](https://everything.curl.dev/usingcurl/netrc) configuration file
- A [keyring](https://github.com/jaraco/keyring) provider (requires opt-in)

If authentication is found for a single net location (scheme, host, and port), it will be cached for
the duration of the command and used for other queries to that net location. Authentication is not
cached across invocations of uv.

Note `--keyring-provider subprocess` or `UV_KEYRING_PROVIDER=subprocess` must be provided to enable
keyring-based authentication.

Authentication may be used for hosts specified in the following contexts:

- `index-url`
- `extra-index-url`
- `find-links`
- `package @ https://...`

See the [`pip` compatibility guide](PIP_COMPATIBILITY.md#registry-authentication) for details on
differences from `pip`.

### Dependency caching

uv uses aggressive caching to avoid re-downloading (and re-building dependencies) that have already
been accessed in prior runs.

The specifics of uv's caching semantics vary based on the nature of the dependency:

- **For registry dependencies** (like those downloaded from PyPI), uv respects HTTP caching headers.
- **For direct URL dependencies**, uv respects HTTP caching headers, and also caches based on the
  URL itself.
- **For Git dependencies**, uv caches based on the fully-resolved Git commit hash. As such,
  `uv pip compile` will pin Git dependencies to a specific commit hash when writing the resolved
  dependency set.
- **For local dependencies**, uv caches based on the last-modified time of the source archive (i.e.,
  the local `.whl` or `.tar.gz` file). For directories, uv caches based on the last-modified time of
  the `pyproject.toml`, `setup.py`, or `setup.cfg` file.

It's safe to run multiple `uv` commands concurrently, even against the same virtual environment.
uv's cache is designed to be thread-safe and append-only, and thus robust to multiple concurrent
readers and writers. uv applies a file-based lock to the target virtual environment when installing,
to avoid concurrent modifications across processes.

Note that it's _not_ safe to modify the uv cache directly (e.g., `uv cache clean`) while other `uv`
commands are running, and _never_ safe to modify the cache directly (e.g., by removing a file or
directory).

If you're running into caching issues, uv includes a few escape hatches:

- To force uv to revalidate cached data for all dependencies, run `uv pip install --refresh ...`.
- To force uv to revalidate cached data for a specific dependency, run, e.g.,
  `uv pip install --refresh-package flask ...`.
- To force uv to ignore existing installed versions, run `uv pip install --reinstall ...`.
- To clear the global cache entirely, run `uv cache clean`.

### Resolution strategy

By default, uv follows the standard Python dependency resolution strategy of preferring the latest
compatible version of each package. For example, `uv pip install flask>=2.0.0` will install the
latest version of Flask (at time of writing: `3.0.0`).

However, uv's resolution strategy can be configured to support alternative workflows. With
`--resolution=lowest`, uv will install the **lowest** compatible versions for all dependencies, both
**direct** and **transitive**. Alternatively, `--resolution=lowest-direct` will opt for the
**lowest** compatible versions for all **direct** dependencies, while using the **latest**
compatible versions for all **transitive** dependencies. This distinction can be particularly useful
for library authors who wish to test against the lowest supported versions of direct dependencies
without restricting the versions of transitive dependencies.

For example, given the following `requirements.in` file:

```text
flask>=2.0.0
```

Running `uv pip compile requirements.in` would produce the following `requirements.txt` file:

```text
# This file was autogenerated by uv via the following command:
#    uv pip compile requirements.in
blinker==1.7.0
    # via flask
click==8.1.7
    # via flask
flask==3.0.0
itsdangerous==2.1.2
    # via flask
jinja2==3.1.2
    # via flask
markupsafe==2.1.3
    # via
    #   jinja2
    #   werkzeug
werkzeug==3.0.1
    # via flask
```

However, `uv pip compile --resolution=lowest requirements.in` would instead produce:

```text
# This file was autogenerated by uv via the following command:
#    uv pip compile requirements.in --resolution=lowest
click==7.1.2
    # via flask
flask==2.0.0
itsdangerous==2.0.0
    # via flask
jinja2==3.0.0
    # via flask
markupsafe==2.0.0
    # via jinja2
werkzeug==2.0.0
    # via flask
```

### Pre-release handling

By default, uv will accept pre-release versions during dependency resolution in two cases:

1. If the package is a direct dependency, and its version markers include a pre-release specifier
   (e.g., `flask>=2.0.0rc1`).
1. If _all_ published versions of a package are pre-releases.

If dependency resolution fails due to a transitive pre-release, uv will prompt the user to re-run
with `--prerelease=allow`, to allow pre-releases for all dependencies.

Alternatively, you can add the transitive dependency to your `requirements.in` file with a
pre-release specifier (e.g., `flask>=2.0.0rc1`) to opt in to pre-release support for that specific
dependency.

Pre-releases are
[notoriously difficult](https://pubgrub-rs-guide.netlify.app/limitations/prerelease_versions) to
model, and are a frequent source of bugs in other packaging tools. uv's pre-release handling is
_intentionally_ limited and _intentionally_ requires user opt-in for pre-releases, to ensure
correctness.

For more, see ["Pre-release compatibility"](./PIP_COMPATIBILITY.md#pre-release-compatibility)

### Dependency overrides

Historically, `pip` has supported "constraints" (`-c constraints.txt`), which allows users to narrow
the set of acceptable versions for a given package.

uv supports constraints, but also takes this concept further by allowing users to _override_ the
acceptable versions of a package across the dependency tree via overrides
(`--override overrides.txt`).

In short, overrides allow the user to lie to the resolver by overriding the declared dependencies of
a package. Overrides are a useful last resort for cases in which the user knows that a dependency is
compatible with a newer version of a package than the package declares, but the package has not yet
been updated to declare that compatibility.

For example, if a transitive dependency declares `pydantic>=1.0,<2.0`, but the user knows that the
package is compatible with `pydantic>=2.0`, the user can override the declared dependency with
`pydantic>=2.0,<3` to allow the resolver to continue.

While constraints are purely _additive_, and thus cannot _expand_ the set of acceptable versions for
a package, overrides _can_ expand the set of acceptable versions for a package, providing an escape
hatch for erroneous upper version bounds.

### Platform-independent resolution

By default, uv's `pip-compile` command produces a resolution that's known to be compatible with the
current platform and Python version. Unlike Poetry and PDM, uv does not yet produce a
machine-agnostic lockfile ([#2679](https://github.com/astral-sh/uv/issues/2679)).

However, uv _does_ support resolving for alternate platforms and Python versions via the
`--python-platform` and `--python-version` command line arguments.

For example, if you're running uv on macOS, but want to resolve for Linux, you can run
`uv pip compile --python-platform=linux requirements.in` to produce a `manylinux2014`-compatible
resolution.

Similarly, if you're running uv on Python 3.9, but want to resolve for Python 3.8, you can run
`uv pip compile --python-version=3.8 requirements.in` to produce a Python 3.8-compatible resolution.

The `--python-platform` and `--python-version` arguments can be combined to produce a resolution for
a specific platform and Python version, enabling users to generate multiple lockfiles for different
environments from a single machine.

_N.B. Python's environment markers expose far more information about the current machine than can be
expressed by a simple `--python-platform` argument. For example, the `platform_version` marker on
macOS includes the time at which the kernel was built, which can (in theory) be encoded in package
requirements. uv's resolver makes a best-effort attempt to generate a resolution that is compatible
with any machine running on the target `--python-platform`, which should be sufficient for most use
cases, but may lose fidelity for complex package and platform combinations._

### Time-restricted reproducible resolutions

uv supports an `--exclude-newer` option to limit resolution to distributions published before a
specific date, allowing reproduction of installations regardless of new package releases. The date
may be specified as an [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339.html) timestamp (e.g.,
`2006-12-02T02:07:43Z`) or UTC date in the same format (e.g., `2006-12-02`).

Note the package index must support the `upload-time` field as specified in
[`PEP 700`](https://peps.python.org/pep-0700/). If the field is not present for a given
distribution, the distribution will be treated as unavailable.

To ensure reproducibility, messages for unsatisfiable resolutions will not mention that
distributions were excluded due to the `--exclude-newer` flag — newer distributions will be treated
as if they do not exist.

### Custom CA certificates

By default, uv loads certificates from the bundled `webpki-roots` crate. The `webpki-roots` are a
reliable set of trust roots from Mozilla, and including them in uv improves portability and
performance (especially on macOS, where reading the system trust store incurs a significant delay).

However, in some cases, you may want to use the platform's native certificate store, especially if
you're relying on a corporate trust root (e.g., for a mandatory proxy) that's included in your
system's certificate store. To instruct uv to use the system's trust store, run uv with the
`--native-tls` command-line flag, or set the `UV_NATIVE_TLS` environment variable to `true`.

If a direct path to the certificate is required (e.g., in CI), set the `SSL_CERT_FILE` environment
variable to the path of the certificate bundle, to instruct uv to use that file instead of the
system's trust store.

If client certificate authentication (mTLS) is desired, set the `SSL_CLIENT_CERT` environment
variable to the path of the PEM formatted file containing the certificate followed by the private
key.

## Platform support

uv has Tier 1 support for the following platforms:

- macOS (Apple Silicon)
- macOS (x86_64)
- Linux (x86_64)
- Windows (x86_64)

uv is continuously built, tested, and developed against its Tier 1 platforms. Inspired by the Rust
project, Tier 1 can be thought of as
["guaranteed to work"](https://doc.rust-lang.org/beta/rustc/platform-support.html).

uv has Tier 2 support
(["guaranteed to build"](https://doc.rust-lang.org/beta/rustc/platform-support.html)) for the
following platforms:

- Linux (PPC64)
- Linux (PPC64LE)
- Linux (aarch64)
- Linux (armv7)
- Linux (i686)
- Linux (s390x)

uv ships pre-built wheels to [PyPI](https://pypi.org/project/uv/) for its Tier 1 and Tier 2
platforms. However, while Tier 2 platforms are continuously built, they are not continuously tested
or developed against, and so stability may vary in practice.

Beyond the Tier 1 and Tier 2 platforms, uv is known to build on i686 Windows, and known _not_ to
build on aarch64 Windows, but does not consider either platform to be supported at this time. The
minimum supported Windows version is Windows 10, following
[Rust's own Tier 1 support](https://blog.rust-lang.org/2024/02/26/Windows-7.html).

uv supports and is tested against Python 3.8, 3.9, 3.10, 3.11, and 3.12.

## Environment variables

uv accepts the following command-line arguments as environment variables:

- `UV_INDEX_URL`: Equivalent to the `--index-url` command-line argument. If set, uv will use this
  URL as the base index for searching for packages.
- `UV_EXTRA_INDEX_URL`: Equivalent to the `--extra-index-url` command-line argument. If set, uv will
  use this space-separated list of URLs as additional indexes when searching for packages.
- `UV_CACHE_DIR`: Equivalent to the `--cache-dir` command-line argument. If set, uv will use this
  directory for caching instead of the default cache directory.
- `UV_NO_CACHE`: Equivalent to the `--no-cache` command-line argument. If set, uv will not use the
  cache for any operations.
- `UV_RESOLUTION`: Equivalent to the `--resolution` command-line argument. For example, if set to
  `lowest-direct`, uv will install the lowest compatible versions of all direct dependencies.
- `UV_PRERELEASE`: Equivalent to the `--prerelease` command-line argument. For example, if set to
  `allow`, uv will allow pre-release versions for all dependencies.
- `UV_SYSTEM_PYTHON`: Equivalent to the `--system` command-line argument. If set to `true`, uv will
  use the first Python interpreter found in the system `PATH`. WARNING: `UV_SYSTEM_PYTHON=true` is
  intended for use in continuous integration (CI) or containerized environments and should be used
  with caution, as modifying the system Python can lead to unexpected behavior.
- `UV_PYTHON`: Equivalent to the `--python` command-line argument. If set to a path, uv will use
  this Python interpreter for all operations.
- `UV_BREAK_SYSTEM_PACKAGES`: Equivalent to the `--break-system-packages` command-line argument. If
  set to `true`, uv will allow the installation of packages that conflict with system-installed
  packages. WARNING: `UV_BREAK_SYSTEM_PACKAGES=true` is intended for use in continuous integration
  (CI) or containerized environments and should be used with caution, as modifying the system Python
  can lead to unexpected behavior.
- `UV_NATIVE_TLS`: Equivalent to the `--native-tls` command-line argument. If set to `true`, uv will
  use the system's trust store instead of the bundled `webpki-roots` crate.
- `UV_INDEX_STRATEGY`: Equivalent to the `--index-strategy` command-line argument. For example, if
  set to `unsafe-any-match`, uv will consider versions of a given package available across all index
  URLs, rather than limiting its search to the first index URL that contains the package.
- `UV_REQUIRE_HASHES`: Equivalent to the `--require-hashes` command-line argument. If set to `true`,
  uv will require that all dependencies have a hash specified in the requirements file.
- `UV_CONSTRAINT`: Equivalent to the `--constraint` command-line argument. If set, uv will use this
  file as the constraints file. Uses space-separated list of files.
- `UV_BUILD_CONSTRAINT`: Equivalent to the `--build-constraint` command-line argument. If set, uv
  will use this file as constraints for any source distribution builds. Uses space-separated list of
  files.
- `UV_OVERRIDE`: Equivalent to the `--override` command-line argument. If set, uv will use this file
  as the overrides file. Uses space-separated list of files.
- `UV_LINK_MODE`: Equivalent to the `--link-mode` command-line argument. If set, uv will use this as
  a link mode.
- `UV_NO_BUILD_ISOLATION`: Equivalent to the `--no-build-isolation` command-line argument. If set,
  uv will skip isolation when building source distributions.
- `UV_CUSTOM_COMPILE_COMMAND`: Used to override `uv` in the output header of the `requirements.txt`
  files generated by `uv pip compile`. Intended for use-cases in which `uv pip compile` is called
  from within a wrapper script, to include the name of the wrapper script in the output file.
- `UV_KEYRING_PROVIDER`: Equivalent to the `--keyring-provider` command-line argument. If set, uv
  will use this value as the keyring provider.
- `UV_CONFIG_FILE`: Equivalent to the `--config-file` command-line argument. Expects a path to a
  local `uv.toml` file to use as the configuration file.
- `UV_NO_CONFIG`: Equivalent to the `--no-config` command-line argument. If set, uv will not read
  any configuration files from the current directory, parent directories, or user configuration
  directories.
- `UV_CONCURRENT_DOWNLOADS`: Sets the maximum number of in-flight concurrent downloads that `uv`
  will perform at any given time.
- `UV_CONCURRENT_BUILDS`: Sets the maximum number of source distributions that `uv` will build
  concurrently at any given time.
- `UV_CONCURRENT_INSTALLS`: Used to control the number of threads used when installing and unzipping
  packages.
- `UV_EXCLUDE_NEWER`: Equivalent to the `--exclude-newer` command-line argument. If set, uv will
  exclude distributions published after the specified date.
- `UV_PYTHON_INSTALL_MIRROR`: Managed Python installations are downloaded from
  [`python-build-standalone`](https://github.com/indygreg/python-build-standalone). This variable
  can be set to a mirror URL to use a different source for Python installations. The provided URL
  will replace `https://github.com/indygreg/python-build-standalone/releases/download` in, e.g.,
  `https://github.com/indygreg/python-build-standalone/releases/download/20240713/cpython-3.12.4%2B20240713-aarch64-apple-darwin-install_only.tar.gz`.
- `UV_PYPY_INSTALL_MIRROR`: Managed PyPy installations are downloaded from
  [python.org](https://downloads.python.org/). This variable can be set to a mirror URL to use a
  different source for PyPy installations. The provided URL will replace
  `https://downloads.python.org/pypy` in, e.g.,
  `https://downloads.python.org/pypy/pypy3.8-v7.3.7-osx64.tar.bz2`.

In each case, the corresponding command-line argument takes precedence over an environment variable.

In addition, uv respects the following environment variables:

- `SSL_CERT_FILE`: If set, uv will use this file as the certificate bundle instead of the system's
  trust store.
- `SSL_CLIENT_CERT`: If set, uv will use this file for mTLS authentication. This should be a single
  file containing both the certificate and the private key in PEM format.
- `RUST_LOG`: If set, uv will use this value as the log level for its `--verbose` output. Accepts
  any filter compatible with the `tracing_subscriber` crate. For example, `RUST_LOG=trace` will
  enable trace-level logging. See the
  [tracing documentation](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html#example-syntax)
  for more.
- `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY`: The proxy to use for all HTTP/HTTPS requests.
- `HTTP_TIMEOUT` (or `UV_HTTP_TIMEOUT`): If set, uv will use this value (in seconds) as the timeout
  for HTTP reads (default: 30 s).
- `PYC_INVALIDATION_MODE`: The validation modes to use when run with `--compile`. See:
  [`PycInvalidationMode`](https://docs.python.org/3/library/py_compile.html#py_compile.PycInvalidationMode).
- `VIRTUAL_ENV`: Used to detect an activated virtual environment.
- `CONDA_PREFIX`: Used to detect an activated Conda environment.
- `PROMPT`: Used to detect the use of the Windows Command Prompt (as opposed to PowerShell).
- `NU_VERSION`: Used to detect the use of NuShell.
- `FISH_VERSION`: Used to detect the use of the Fish shell.
- `BASH_VERSION`: Used to detect the use of the Bash shell.
- `ZSH_VERSION`: Used to detect the use of the Zsh shell.
- `MACOSX_DEPLOYMENT_TARGET`: Used with `--python-platform macos` and related variants to set the
  deployment target (i.e., the minimum supported macOS version). Defaults to `12.0`, the
  least-recent non-EOL macOS version at time of writing.
- `NO_COLOR`: Disable colors. Takes precedence over `FORCE_COLOR`. See
  [no-color.org](https://no-color.org).
- `FORCE_COLOR`: Enforce colors regardless of TTY support. See
  [force-color.org](https://force-color.org).

## Versioning

uv uses a custom versioning scheme in which the minor version number is bumped for breaking changes,
and the patch version number is bumped for bug fixes, enhancements, and other non-breaking changes.

uv does not yet have a stable API; once uv's API is stable (v1.0.0), the versioning scheme will
adhere to [Semantic Versioning](https://semver.org/).

## Acknowledgements

uv's dependency resolver uses [PubGrub](https://github.com/pubgrub-rs/pubgrub) under the hood. We're
grateful to the PubGrub maintainers, especially [Jacob Finkelman](https://github.com/Eh2406), for
their support.

uv's Git implementation is based on [Cargo](https://github.com/rust-lang/cargo).

Some of uv's optimizations are inspired by the great work we've seen in [pnpm](https://pnpm.io/),
[Orogene](https://github.com/orogene/orogene), and [Bun](https://github.com/oven-sh/bun). We've also
learned a lot from Nathaniel J. Smith's [Posy](https://github.com/njsmith/posy) and adapted its
[trampoline](https://github.com/njsmith/posy/tree/main/src/trampolines/windows-trampolines/posy-trampoline)
for Windows support.

## License

uv is licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
  https://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in uv
by you, as defined in the Apache-2.0 license, shall be dually licensed as above, without any
additional terms or conditions.

<div align="center">
  <a target="_blank" href="https://astral.sh" style="background:none">
    <img src="https://raw.githubusercontent.com/astral-sh/uv/main/assets/svg/Astral.svg" alt="Made by Astral">
  </a>
</div>
