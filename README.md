# uv

[![uv](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/astral-sh/uv/main/assets/badge/v0.json)](https://github.com/astral-sh/uv)
[![image](https://img.shields.io/pypi/v/uv.svg)](https://pypi.python.org/pypi/uv)
[![image](https://img.shields.io/pypi/l/uv.svg)](https://pypi.python.org/pypi/uv)
[![image](https://img.shields.io/pypi/pyversions/uv.svg)](https://pypi.python.org/pypi/uv)
[![Actions status](https://github.com/astral-sh/uv/workflows/CI/badge.svg)](https://github.com/astral-sh/uv/actions)
[![Discord](https://img.shields.io/badge/Discord-%235865F2.svg?logo=discord&logoColor=white)](https://discord.gg/astral-sh)

An extremely fast Python package installer and resolver, written in Rust. Designed as a drop-in
replacement for `pip` and `pip-compile`.

uv is backed by [Astral](https://astral.sh), the creators of [Ruff](https://github.com/astral-sh/ruff).

## Highlights

- ⚖️ Drop-in replacement for common `pip`, `pip-tools`, and `virtualenv` commands.
- ⚡️ [10-100x faster](https://github.com/astral-sh/uv/blob/main/BENCHMARKS.md) than `pip`
  and `pip-tools` (`pip-compile` and `pip-sync`).
- 💾 Disk-space efficient, with a global cache for dependency deduplication.
- 🐍 Installable via `curl`, `pip`, `pipx`, etc. uv is a static binary that can be installed
  without Rust or Python.
- 🧪 Tested at-scale against the top 10,000 PyPI packages.
- 🖥️ Support for macOS, Linux, and Windows.
- 🧰 Advanced features such as [dependency version overrides](#dependency-overrides) and
   [alternative resolution strategies](#resolution-strategy).
- ⁉️ Best-in-class error messages with a conflict-tracking resolver.
- 🤝 Support for a wide range of advanced `pip` features, including editable installs, Git
  dependencies, direct URL dependencies, local dependencies, constraints, source distributions,
  HTML and JSON indexes, and more.

## Getting Started

Install uv with our standalone installers, or from [PyPI](https://pypi.org/project/uv/):

```shell
# On macOS and Linux.
curl -LsSf https://astral.sh/uv/install.sh | sh

# On Windows.
irm https://astral.sh/uv/install.ps1 | iex

# With pip.
pip install uv

# With pipx.
pipx install uv
```

To create a virtual environment:

```shell
uv venv  # Create a virtual environment at .venv.
```

To activate the virtual environment:

```shell
# On macOS and Linux.
source .venv/bin/activate

# On Windows.
.\.venv\Scripts\activate.ps1
```

To install a package into the virtual environment:

```shell
uv pip install flask                # Install Flask.
uv pip install -r requirements.txt  # Install from a requirements.txt file.
uv pip install -e .                 # Install the current project in editable mode.
uv pip install "package @ ."        # Install the current project from disk
```

To generate a set of locked dependencies from an input file:

```shell
uv pip compile pyproject.toml -o requirements.txt   # Read a pyproject.toml file.
uv pip compile requirements.in -o requirements.txt  # Read a requirements.in file.
```

To sync a set of locked dependencies with the virtual environment:

```shell
uv pip sync requirements.txt  # Install from a requirements.txt file.
```

uv's `pip-install` and `pip-compile` commands support many of the same command-line arguments
as existing tools, including `-r requirements.txt`, `-c constraints.txt`, `-e .` (for editable
installs), `--index-url`, and more.

## Limitations

uv does not support the entire `pip` feature set. Namely, uv does not (and does not plan to)
support the following `pip` features:

- `.egg` dependencies
- Editable installs for Git and direct URL dependencies (editable installs _are_ supported for local
  dependencies)

On the other hand, uv plans to (but does not currently) support:

- [Hash-checking mode](https://github.com/astral-sh/uv/issues/474)
- [URL requirements without package names](https://github.com/astral-sh/uv/issues/313)
  (e.g., `https://...` instead of `package @ https://...`)

Like `pip-compile`, uv generates a platform-specific `requirements.txt` file (unlike, e.g.,
`poetry` and `pdm`, which generate platform-agnostic `poetry.lock` and `pdm.lock` files). As such,
uv's `requirements.txt` files may not be portable across platforms and Python versions.

## Roadmap

uv is an extremely fast Python package resolver and installer, designed as a drop-in
replacement for `pip`, `pip-tools` (`pip-compile` and `pip-sync`), and `virtualenv`.

uv represents an intermediary goal in our pursuit of a ["Cargo for Python"](https://blog.rust-lang.org/2016/05/05/cargo-pillars.html#pillars-of-cargo):
a comprehensive project and package manager that is extremely fast, reliable, and easy to use.

Think: a single binary that bootstraps your Python installation and gives you everything you need to
be productive with Python, bundling not only `pip`, `pip-tools`, and `virtualenv`, but also `pipx`,
`tox`, `poetry`, `pyenv`, `ruff`, and more.

Our goal is to evolve uv into such a tool.

In the meantime, though, the narrower `pip-tools` scope allows us to solve the low-level problems
involved in building such a tool (like package installation) while shipping something immediately
useful with a minimal barrier to adoption.

## Advanced Usage

### Python discovery

uv itself does not depend on Python, but it does need to locate a Python environment to (1)
install dependencies into the environment and (2) build source distributions.

When running `pip sync` or `pip install`, uv will search for a virtual environment in the
following order:

- An activated virtual environment based on the `VIRTUAL_ENV` environment variable.
- An activated Conda environment based on the `CONDA_PREFIX` environment variable.
- A virtual environment at `.venv` in the current directory, or in the nearest parent directory.

If no virtual environment is found, uv will prompt the user to create one in the current
directory via `uv venv`.

When running `pip compile`, uv does not _require_ a virtual environment and will search for a
Python interpreter in the following order:

- An activated virtual environment based on the `VIRTUAL_ENV` environment variable.
- An activated Conda environment based on the `CONDA_PREFIX` environment variable.
- A virtual environment at `.venv` in the current directory, or in the nearest parent directory.
- The Python interpreter available as `python3` on macOS and Linux, or `python.exe` on Windows.

If a `--python-version` is provided to `pip compile` (e.g., `--python-version=3.7`), uv will
search for a Python interpreter matching that version in the following order:

- An activated virtual environment based on the `VIRTUAL_ENV` environment variable.
- An activated Conda environment based on the `CONDA_PREFIX` environment variable.
- A virtual environment at `.venv` in the current directory, or in the nearest parent directory.
- The Python interpreter available as, e.g., `python3.7` on macOS and Linux. On Windows, uv
  will use the same mechanism as `py --list-paths` to discover all available Python interpreters,
  and will select the first interpreter matching the requested version.
- The Python interpreter available as `python3` on macOS and Linux, or `python.exe` on Windows.

Since uv has no dependency on Python, it can even install into virtual environments other than
its own. For example, setting `VIRTUAL_ENV=/path/to/venv` will cause uv to install into
`/path/to/venv`, no matter where uv is installed.

### Dependency caching

uv uses aggressive caching to avoid re-downloading (and re-building dependencies) that have
already been accessed in prior runs.

The specifics of uv's caching semantics vary based on the nature of the dependency:

- **For registry dependencies** (like those downloaded from PyPI), uv respects HTTP caching headers.
- **For direct URL dependencies**, uv respects HTTP caching headers, and also caches based on
  the URL itself.
- **For Git dependencies**, uv caches based on the fully-resolved Git commit hash. As such,
  `uv pip compile` will pin Git dependencies to a specific commit hash when writing the resolved
  dependency set.
- **For local dependencies**, uv caches based on the last-modified time of the `setup.py` or
  `pyproject.toml` file.

If you're running into caching issues, uv includes a few escape hatches:

- To force uv to revalidate cached data for all dependencies, run `uv pip install --refresh ...`.
- To force uv to revalidate cached data for a specific dependency, run, e.g., `uv pip install --refresh-package flask ...`.
- To force uv to ignore existing installed versions, run `uv pip install --reinstall ...`.
- To clear the global cache entirely, run `uv clean`.

### Resolution strategy

By default, uv follows the standard Python dependency resolution strategy of preferring the
latest compatible version of each package. For example, `uv pip install flask>=2.0.0` will
install the latest version of Flask (at time of writing: `3.0.0`).

However, uv's resolution strategy can be configured to prefer the _lowest_ compatible version of
each package (`--resolution=lowest`), or even the lowest compatible version of any _direct_
dependencies (`--resolution=lowest-direct`), both of which can be useful for library authors looking
to test their packages against the oldest supported versions of their dependencies.

For example, given the following `requirements.in` file:

```text
flask>=2.0.0
```

Running `uv pip compile requirements.in` would produce the following `requirements.txt` file:

```text
# This file was autogenerated by uv v0.0.1 via the following command:
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
# This file was autogenerated by uv v0.0.1 via the following command:
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

If dependency resolution fails due to a transitive pre-release, uv will prompt the user to
re-run with `--prerelease=allow`, to allow pre-releases for all dependencies.

Alternatively, you can add the transitive dependency to your `requirements.in` file with
pre-release specifier (e.g., `flask>=2.0.0rc1`) to opt in to pre-release support for that specific
dependency.

Pre-releases are [notoriously difficult](https://pubgrub-rs-guide.netlify.app/limitations/prerelease_versions)
to model, and are a frequent source of bugs in other packaging tools. uv's pre-release handling
is _intentionally_ limited and _intentionally_ requires user intervention to opt in to pre-releases
to ensure correctness, though pre-release handling will be revisited in future releases.

### Dependency overrides

Historically, `pip` has supported "constraints" (`-c constraints.txt`), which allows users to
narrow the set of acceptable versions for a given package.

uv supports constraints, but also takes this concept further by allowing users to _override_ the
acceptable versions of a package across the dependency tree via overrides (`-o overrides.txt`).

In short, overrides allow the user to lie to the resolver by overriding the declared dependencies
of a package. Overrides are a useful last resort for cases in which the user knows that a
dependency is compatible with a newer version of a package than the package declares, but the
package has not yet been updated to declare that compatibility.

For example, if a transitive dependency declares `pydantic>=1.0,<2.0`, but the user knows that
the package is compatible with `pydantic>=2.0`, the user can override the declared dependency
with `pydantic>=2.0,<3` to allow the resolver to continue.

While constraints are purely _additive_, and thus cannot _expand_ the set of acceptable versions for
a package, overrides _can_ expand the set of acceptable versions for a package, providing an escape
hatch for erroneous upper version bounds.

### Multi-version resolution

uv's `pip-compile` command produces a resolution that's known to be compatible with the
current platform and Python version. Unlike Poetry, PDM, and other package managers, uv does
not yet produce a machine-agnostic lockfile.

However, uv _does_ support resolving for alternate Python versions via the `--python-version`
command line argument. For example, if you're running uv on Python 3.9, but want to resolve for
Python 3.8, you can run `uv pip compile --python-version=3.8 requirements.in` to produce a
Python 3.8-compatible resolution.

## Platform support

uv has Tier 1 support for the following platforms:

- macOS (Apple Silicon)
- macOS (x86_64)
- Linux (x86_64)
- Windows (x86_64)

uv is continuously built, tested, and developed against its Tier 1 platforms. Inspired by the
Rust project, Tier 1 can be thought of as ["guaranteed to work"](https://doc.rust-lang.org/beta/rustc/platform-support.html).

uv has Tier 2 support (["guaranteed to build"](https://doc.rust-lang.org/beta/rustc/platform-support.html)) for the following platforms:

- Linux (PPC64)
- Linux (PPC64LE)
- Linux (aarch64)
- Linux (armv7)
- Linux (i686)
- Linux (s390x)

uv ships pre-built wheels to [PyPI](https://pypi.org/project/uv/) for its Tier 1 and
Tier 2 platforms. However, while Tier 2 platforms are continuously built, they are not continuously
tested or developed against, and so stability may vary in practice.

Beyond the Tier 1 and Tier 2 platforms, uv is known to build on i686 Windows, and known _not_
to build on aarch64 Windows, but does not consider either platform to be supported at this time.

uv supports and is tested against Python 3.8, 3.9, 3.10, 3.11, and 3.12.

## Acknowledgements

uv's dependency resolver uses [PubGrub](https://github.com/pubgrub-rs/pubgrub) under the hood.
We're grateful to the PubGrub maintainers, especially [Jacob Finkelman](https://github.com/Eh2406),
for their support.

uv's Git implementation is based on [Cargo](https://github.com/rust-lang/cargo).

Some of uv's optimizations are inspired by the great work we've seen in
[pnpm](https://pnpm.io/), [Orogene](https://github.com/orogene/orogene), and
[Bun](https://github.com/oven-sh/bun). We've also learned a lot from Nathaniel
J. Smith's [Posy](https://github.com/njsmith/posy) and adapted its [trampoline](https://github.com/njsmith/posy/tree/main/src/trampolines/windows-trampolines/posy-trampoline)
for Windows support.

## License

uv is licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or https://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or https://opensource.org/licenses/MIT)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in uv by you, as defined in the Apache-2.0 license, shall be
dually licensed as above, without any additional terms or conditions.

<div align="center">
  <a target="_blank" href="https://astral.sh" style="background:none">
    <img src="https://raw.githubusercontent.com/astral-sh/uv/main/assets/svg/Astral.svg" alt="Made by Astral">
  </a>
</div>
