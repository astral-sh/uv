# Specifying dependencies

In uv, project dependencies are declared across two `pyproject.toml` tables: `project.dependencies`
and `tool.uv.sources`.

`project.dependencies` defines the standards-compliant dependency metadata, propagated when
uploading to PyPI or building a wheel.

`tool.uv.sources` enriches the dependency metadata with additional sources, incorporated during
development. A dependency source can be a Git repository, a URL, a local path, or an alternative
registry.

`tool.uv.sources` enables uv to support common patterns like editable installations and relative
paths that are not supported by the `project.dependencies` standard. For example:

```toml title="pyproject.toml"
[project]
name = "albatross"
version = "0.1.0"
dependencies = [
  "bird-feeder",
]

[tool.uv.sources]
bird-feeder = { path = "./packages/bird-feeder" }
```

## Project dependencies

The `project.dependencies` table represents the dependencies that are used when uploading to PyPI or
building a wheel. Individual dependencies are specified using [PEP 508](#pep-508) syntax, and the
table follows the [PEP 621](https://packaging.python.org/en/latest/specifications/pyproject-toml/)
standard.

`project.dependencies` defines the list of packages that are required for the project, along with
the version constraints that should be used when installing them. Each entry includes a dependency
name and version. An entry may include extras or environment markers for platform-specific packages.
For example:

```toml title="pyproject.toml"
[project]
name = "albatross"
version = "0.1.0"
dependencies = [
  # Any version in this range
  "tqdm >=4.66.2,<5",
  # Exactly this version of torch
  "torch ==2.2.2",
  # Install transformers with the torch extra
  "transformers[torch] >=4.39.3,<5",
  # Only install this package on older python versions
  # See "Environment Markers" for more information
  "importlib_metadata >=7.1.0,<8; python_version < '3.10'",
  "mollymawk ==0.1.0"
]
```

If the project only requires packages from standard package indexes, then `project.dependencies` is
sufficient. If the project depends on packages from Git, remote URLs, or local sources,
`tool.uv.sources` can be used to enrich the dependency metadata without ejecting from the
stands-compliant `project.dependencies` table.

!!! tip

    See the [projects](./projects.md#managing-dependencies) documentation to add, remove, or update
    dependencies from the `pyproject.toml` from the CLI.

## Dependency sources

During development, a project may rely on a package that isn't available on PyPI. The following
additional sources are supported by uv:

- Git: A Git repository.
- URL: A remote wheel or source distribution.
- Path: A local wheel, source distribution, or project directory.
- Workspace: A member of the current workspace.

Only a single source may be defined for each dependency.

Note that if a non-uv project uses a project with sources as a Git- or path-dependency, only
`project.dependencies` and `project.optional-dependencies` are respected. Any information provided
in the source table will need to be re-specified in a format specific to the other package manager.

To instruct uv to ignore the `tool.uv.sources` table (e.g., to simulate resolving with the package's
published metadata), use the `--no-sources` flag:

```console
$ uv lock --no-sources
```

The use of `--no-sources` will also prevent uv from discovering any
[workspace members](#workspace-member) that could satisfy a given dependency.

### Git

To add a Git dependency source, prefix a Git-compatible URL to clone with `git+`.

For example:

```console
$ uv add git+https://github.com/encode/httpx
```

Will result in a `pyproject.toml` with:

```toml title="pyproject.toml"
[project]
dependencies = [
    "httpx",
]

[tool.uv.sources]
httpx = { git = "https://github.com/encode/httpx" }
```

A revision, tag, or branch may also be included:

```console
$ uv add git+https://github.com/encode/httpx --tag 0.27.0
$ uv add git+https://github.com/encode/httpx --branch main
$ uv add git+https://github.com/encode/httpx --rev 326b943
```

Git dependencies can also be manually added or edited in the `pyproject.toml` with the
`{ git = <url> }` syntax. A target revision may be specified with one of: `rev`, `tag`, or `branch`.
A `subdirectory` may be specified if the package isn't in the repository root.

### URL

To add a URL source, provide a `https://` URL to either a wheel (ending in `.whl`) or a source
distribution (typically ending in `.tar.gz` or `.zip`; see
[here](../concepts/resolution.md#source-distribution) for all supported formats).

For example:

```console
$ uv add "https://files.pythonhosted.org/packages/5c/2d/3da5bdf4408b8b2800061c339f240c1802f2e82d55e50bd39c5a881f47f0/httpx-0.27.0.tar.gz"
```

Will result in a `pyproject.toml` with:

```toml title="pyproject.toml"
[project]
dependencies = [
    "httpx",
]

[tool.uv.sources]
httpx = { url = "https://files.pythonhosted.org/packages/5c/2d/3da5bdf4408b8b2800061c339f240c1802f2e82d55e50bd39c5a881f47f0/httpx-0.27.0.tar.gz" }
```

URL dependencies can also be manually added or edited in the `pyproject.toml` with the
`{ url = <url> }` syntax. A `subdirectory` may be specified if the source distribution isn't in the
archive root.

### Path

To add a path source, provide the path of a wheel (ending in `.whl`), a source distribution
(typically ending in `.tar.gz` or `.zip`; see [here](../concepts/resolution.md#source-distribution)
for all supported formats), or a directory containing a `pyproject.toml`.

For example:

```console
$ uv add /example/foo-0.1.0-py3-none-any.whl
```

Will result in a `pyproject.toml` with:

```toml title="pyproject.toml"
[project]
dependencies = [
    "foo",
]

[tool.uv.sources]
foo = { path = "/example/foo-0.1.0-py3-none-any.whl" }
```

The path may also be a relative path:

```console
$ uv add ./foo-0.1.0-py3-none-any.whl
```

Or, a path to a project directory:

```console
$ uv add ~/projects/bar/
```

!!! important

    An [editable installation](#editable-dependencies) is not used for path dependencies by
    default. An editable installation may be requested for project directories:

    ```console
    $ uv add --editable ~/projects/bar/
    ```

    However, it is recommended to use [_workspaces_](./workspaces.md) instead of manual path
    dependencies.

### Workspace member

To declare a dependency on a workspace member, add the member name with `{ workspace = true }`. All
workspace members must be explicitly stated. Workspace members are always
[editable](#editable-dependencies) . See the [workspace](./workspaces.md) documentation for more
details on workspaces.

```toml title="pyproject.toml"
[project]
dependencies = [
  "mollymawk ==0.1.0"
]

[tool.uv.sources]
mollymawk = { workspace = true }

[tool.uv.workspace]
members = [
  "packages/mollymawk"
]
```

### Platform-specific sources

You can limit a source to a given platform or Python version by providing
[PEP 508](https://peps.python.org/pep-0508/#environment-markers)-compatible environment markers for
the source.

For example, to pull `httpx` from GitHub, but only on macOS, use the following:

```toml title="pyproject.toml"
[project]
dependencies = [
  "httpx",
]

[tool.uv.sources]
httpx = { git = "https://github.com/encode/httpx", tag = "0.27.2", marker = "sys_platform == 'darwin'" }
```

By specifying the marker on the source, uv will still include `httpx` on all platforms, but will
download the source from GitHub on macOS, and fall back to PyPI on all other platforms.

### Multiple sources

You can specify multiple sources for a single dependency by providing a list of sources,
disambiguated by [PEP 508](https://peps.python.org/pep-0508/#environment-markers)-compatible
environment markers. For example, to pull in different `httpx` commits on macOS vs. Linux:

```toml title="pyproject.toml"
[project]
dependencies = [
  "httpx",
]

[tool.uv.sources]
httpx = [
  { git = "https://github.com/encode/httpx", tag = "0.27.2", marker = "sys_platform == 'darwin'" },
  { git = "https://github.com/encode/httpx", tag = "0.24.1", marker = "sys_platform == 'linux'" },
]
```

## Optional dependencies

It is common for projects that are published as libraries to make some features optional to reduce
the default dependency tree. For example, Pandas has an
[`excel` extra](https://pandas.pydata.org/docs/getting_started/install.html#excel-files) and a
[`plot` extra](https://pandas.pydata.org/docs/getting_started/install.html#visualization) to avoid
installation of Excel parsers and `matplotlib` unless someone explicitly requires them. Extras are
requested with the `package[<extra>]` syntax, e.g., `pandas[plot, excel]`.

Optional dependencies are specified in `[project.optional-dependencies]`, a TOML table that maps
from extra name to its dependencies, following [PEP 508](#pep-508) syntax.

Optional dependencies can have entries in `tool.uv.sources` the same as normal dependencies.

```toml title="pyproject.toml"
[project]
name = "pandas"
version = "1.0.0"

[project.optional-dependencies]
plot = [
  "matplotlib>=3.6.3"
]
excel = [
  "odfpy>=1.4.1",
  "openpyxl>=3.1.0",
  "python-calamine>=0.1.7",
  "pyxlsb>=1.0.10",
  "xlrd>=2.0.1",
  "xlsxwriter>=3.0.5"
]
```

To add an optional dependency, use the `--optional <extra>` option:

```console
$ uv add httpx --optional network
```

## Development dependencies

Unlike optional dependencies, development dependencies are local-only and will _not_ be included in
the project requirements when published to PyPI or other indexes. As such, development dependencies
are included under `[tool.uv]` instead of `[project]`.

Development dependencies can have entries in `tool.uv.sources` the same as normal dependencies.

```toml title="pyproject.toml"
[tool.uv]
dev-dependencies = [
  "pytest >=8.1.1,<9"
]
```

To add a development dependency, include the `--dev` flag:

```console
$ uv add ruff --dev
```

## PEP 508

[PEP 508](https://peps.python.org/pep-0508/) defines a syntax for dependency specification. It is
composed of, in order:

- The dependency name
- The extras you want (optional)
- The version specifier
- An environment marker (optional)

The version specifiers are comma separated and added together, e.g., `foo >=1.2.3,<2,!=1.4.0` is
interpreted as "a version of `foo` that's at least 1.2.3, but less than 2, and not 1.4.0".

Specifiers are padded with trailing zeros if required, so `foo ==2` matches foo 2.0.0, too.

A star can be used for the last digit with equals, e.g. `foo ==2.1.*` will accept any release from
the 2.1 series. Similarly, `~=` matches where the last digit is equal or higher, e.g., `foo ~=1.2`
is equal to `foo >=1.2,<2`, and `foo ~=1.2.3` is equal to `foo >=1.2.3,<1.3`.

Extras are comma-separated in square bracket between name and version, e.g.,
`pandas[excel,plot] ==2.2`. Whitespace between extra names is ignored.

Some dependencies are only required in specific environments, e.g., a specific Python version or
operating system. For example to install the `importlib-metadata` backport for the
`importlib.metadata` module, use `importlib-metadata >=7.1.0,<8; python_version < '3.10'`. To
install `colorama` on Windows (but omit it on other platforms), use
`colorama >=0.4.6,<5; platform_system == "Windows"`.

Markers are combined with `and`, `or`, and parentheses, e.g.,
`aiohttp >=3.7.4,<4; (sys_platform != 'win32' or implementation_name != 'pypy') and python_version >= '3.10'`.
Note that versions within markers must be quoted, while versions _outside_ of markers must _not_ be
quoted.

## Editable dependencies

A regular installation of a directory with a Python package first builds a wheel and then installs
that wheel into your virtual environment, copying all source files. When the package source files
are edited, the virtual environment will contain outdated versions.

Editable installations solve this problem by adding a link to the project within the virtual
environment (a `.pth` file), which instructs the interpreter to include the source files directly.

There are some limitations to editables (mainly: the build backend needs to support them, and native
modules aren't recompiled before import), but they are useful for development, as the virtual
environment will always use the latest changes to the package.

uv uses editable installation for workspace packages by default.

To add an editable dependency, use the `--editable` flag:

```console
$ uv add --editable ./path/foo
```

Or, to opt-out of using an editable dependency in a workspace:

```console
$ uv add --no-editable ./path/foo
```
