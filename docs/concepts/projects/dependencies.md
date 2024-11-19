# Managing dependencies

uv is capable of adding, updating, and removing dependencies using the CLI.

## Adding dependencies

To add a dependency:

```console
$ uv add httpx
```

uv supports adding [editable dependencies](./dependencies.md#editable-dependencies),
[development dependencies](./dependencies.md#development-dependencies),
[optional dependencies](./dependencies.md#optional-dependencies), and alternative
[dependency sources](./dependencies.md#dependency-sources). See the
[dependency specification](./dependencies.md) documentation for more details.

uv will raise an error if the dependency cannot be resolved, e.g.:

```console
$ uv add 'httpx>9999'
error: Because only httpx<=9999 is available and example==0.1.0 depends on httpx>9999, we can conclude that example==0.1.0 cannot be used.
And because only example==0.1.0 is available and you require example, we can conclude that the requirements are unsatisfiable.
```

## Removing dependencies

To remove a dependency:

```console
$ uv remove httpx
```

## Updating dependencies

To update an existing dependency, e.g., to add a lower bound to the `httpx` version:

```console
$ uv add 'httpx>0.1.0'
```

!!! note

    "Updating" a dependency refers to changing the constraints for the dependency in the
    `pyproject.toml`. The locked version of the dependency will only change if necessary to
    satisfy the new constraints. To force the package version to update to the latest within
    the constraints, use `--upgrade-package <name>`, e.g.:

    ```console
    $ uv add 'httpx>0.1.0' --upgrade-package httpx
    ```

    See the [lockfile](./sync.md#upgrading-locked-package-versions) section for more details on upgrading
    package versions.

Or, to change the bounds for `httpx`:

```console
$ uv add 'httpx<0.2.0'
```

To add a dependency source, e.g., to use `httpx` from GitHub during development:

```console
$ uv add git+https://github.com/encode/httpx
```

## Platform-specific dependencies

To ensure that a dependency is only installed on a specific platform or on specific Python versions,
use Python's standardized
[environment markers](https://peps.python.org/pep-0508/#environment-markers) syntax.

For example, to install `jax` on Linux, but not on Windows or macOS:

```console
$ uv add 'jax; sys_platform == "linux"'
```

The resulting `pyproject.toml` will then include the environment marker in the dependency
definition:

```toml title="pyproject.toml" hl_lines="6"
[project]
name = "project"
version = "0.1.0"
requires-python = ">=3.11"
dependencies = ["jax; sys_platform == 'linux'"]
```

Similarly, to include `numpy` on Python 3.11 and later:

```console
$ uv add 'numpy; python_version >= "3.11"'
```

See Python's [environment marker](https://peps.python.org/pep-0508/#environment-markers)
documentation for a complete enumeration of the available markers and operators.

## Dependency tables

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
building a wheel. Individual dependencies are specified using
[dependency specifiers](https://packaging.python.org/en/latest/specifications/dependency-specifiers/)
syntax, and the table follows the
[PEP 621](https://packaging.python.org/en/latest/specifications/pyproject-toml/) standard.

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
standards-compliant `project.dependencies` table.

## Dependency sources

During development, a project may rely on a package that isn't available on PyPI. The following
additional sources are supported by uv:

- Index: A package resolved from a specific package index.
- Git: A Git repository.
- URL: A remote wheel or source distribution.
- Path: A local wheel, source distribution, or project directory.
- Workspace: A member of the current workspace.

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

### Index

To pin a Python package to a specific index, add a named index to the `pyproject.toml`:

```toml title="pyproject.toml"
[project]
dependencies = [
  "torch",
]

[tool.uv.sources]
torch = { index = "pytorch" }

[[tool.uv.index]]
name = "pytorch"
url = "https://download.pytorch.org/whl/cpu"
explicit = true
```

The `explicit` flag is optional and indicates that the index should _only_ be used for packages that
explicitly specify it in `tool.uv.sources`. If `explicit` is not set, other packages may be resolved
from the index, if not found elsewhere.

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

A revision (i.e., commit), tag, or branch may also be included:

```console
$ uv add git+https://github.com/encode/httpx --tag 0.27.0
$ uv add git+https://github.com/encode/httpx --branch master
$ uv add git+https://github.com/encode/httpx --rev 326b9431c761e1ef1e00b9f760d1f654c8db48c6
```

Git dependencies can also be manually added or edited in the `pyproject.toml` with the
`{ git = <url> }` syntax. A target revision may be specified with one of: `rev` (i.e., commit),
`tag`, or `branch`.

=== "tag"

    ```toml title="pyproject.toml"
    [project]
    dependencies = [
        "httpx",
    ]

    [tool.uv.sources]
    httpx = { git = "https://github.com/encode/httpx", tag = "0.27.0" }
    ```

=== "branch"

    ```toml title="pyproject.toml"
    [project]
    dependencies = [
        "httpx",
    ]

    [tool.uv.sources]
    httpx = { git = "https://github.com/encode/httpx", branch = "main" }
    ```

=== "rev"

    ```toml title="pyproject.toml"
    [project]
    dependencies = [
        "httpx",
    ]

    [tool.uv.sources]
    httpx = { git = "https://github.com/encode/httpx", rev = "326b9431c761e1ef1e00b9f760d1f654c8db48c6" }
    ```

A `subdirectory` may be specified if the package isn't in the repository root.

### URL

To add a URL source, provide a `https://` URL to either a wheel (ending in `.whl`) or a source
distribution (typically ending in `.tar.gz` or `.zip`; see
[here](../../concepts/resolution.md#source-distribution) for all supported formats).

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
(typically ending in `.tar.gz` or `.zip`; see
[here](../../concepts/resolution.md#source-distribution) for all supported formats), or a directory
containing a `pyproject.toml`.

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

    For multiple packages in the same repository, [_workspaces_](./workspaces.md) may be a better
    fit.

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
[dependency specifiers](https://packaging.python.org/en/latest/specifications/dependency-specifiers/)-compatible
environment markers for the source.

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
environment markers.

For example, to pull in different `httpx` commits on macOS vs. Linux:

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

This strategy even extends to pulling packages from different indexes based on environment markers.
For example, to pull `torch` from different PyTorch indexes based on the platform:

```toml title="pyproject.toml"
[project]
dependencies = ["torch"]

[tool.uv.sources]
torch = [
  { index = "torch-cpu", marker = "platform_system == 'Darwin'"},
  { index = "torch-gpu", marker = "platform_system == 'Linux'"},
]

[[tool.uv.index]]
name = "torch-cpu"
url = "https://download.pytorch.org/whl/cpu"

[[tool.uv.index]]
name = "torch-gpu"
url = "https://download.pytorch.org/whl/cu124"
```

## Optional dependencies

It is common for projects that are published as libraries to make some features optional to reduce
the default dependency tree. For example, Pandas has an
[`excel` extra](https://pandas.pydata.org/docs/getting_started/install.html#excel-files) and a
[`plot` extra](https://pandas.pydata.org/docs/getting_started/install.html#visualization) to avoid
installation of Excel parsers and `matplotlib` unless someone explicitly requires them. Extras are
requested with the `package[<extra>]` syntax, e.g., `pandas[plot, excel]`.

Optional dependencies are specified in `[project.optional-dependencies]`, a TOML table that maps
from extra name to its dependencies, following
[dependency specifiers](#dependency-specifiers-pep-508) syntax.

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

!!! note

    If you have optional dependencies that conflict with one another, resolution will fail
    unless you explicitly [declare them as conflicting](./config.md#conflicting-dependencies).

Sources can also be declared as applying only to a specific optional dependency. For example, to
pull `torch` from different PyTorch indexes based on an optional `cpu` or `gpu` extra:

```toml title="pyproject.toml"
[project]
dependencies = []

[project.optional-dependencies]
cpu = [
  "torch",
]
gpu = [
  "torch",
]

[tool.uv.sources]
torch = [
  { index = "torch-cpu", extra = "cpu" },
  { index = "torch-gpu", extra = "gpu" },
]

[[tool.uv.index]]
name = "torch-cpu"
url = "https://download.pytorch.org/whl/cpu"

[[tool.uv.index]]
name = "torch-gpu"
url = "https://download.pytorch.org/whl/cu124"
```

## Development dependencies

Unlike optional dependencies, development dependencies are local-only and will _not_ be included in
the project requirements when published to PyPI or other indexes. As such, development dependencies
are not included in the `[project]` table.

Development dependencies can have entries in `tool.uv.sources` the same as normal dependencies.

To add a development dependency, use the `--dev` flag:

```console
$ uv add --dev pytest
```

uv uses the `[dependency-groups]` table (as defined in [PEP 735](https://peps.python.org/pep-0735/))
for declaration of development dependencies. The above command will create a `dev` group:

```toml title="pyproject.toml"
[dependency-groups]
dev = [
  "pytest >=8.1.1,<9"
]
```

The `dev` group is special-cased; there are `--dev`, `--only-dev`, and `--no-dev` flags to toggle
inclusion or exclusion of its dependencies. Additionally, the `dev` group is
[synced by default](#default-groups).

### Dependency groups

Development dependencies can be divided into multiple groups, using the `--group` flag.

For example, to add a development dependency in the `lint` group:

```console
$ uv add --group lint ruff
```

Which results in the following `[dependency-groups]` definition:

```toml title="pyproject.toml"
[dependency-groups]
dev = [
  "pytest"
]
lint = [
  "ruff"
]
```

Once groups are defined, the `--group`, `--only-group`, and `--no-group` options can be used to
include or exclude their dependencies.

!!! tip

    The `--dev`, `--only-dev`, and `--no-dev` flags are equivalent to `--group dev`,
    `--only-group dev`, and `--no-group dev` respectively.

uv requires that all dependency groups are compatible with each other and resolves all groups
together when creating the lockfile.

If dependencies declared in one group are not compatible with those in another group, uv will fail
to resolve the requirements of the project with an error.

!!! note

    If you have dependency groups that conflict with one another, resolution will fail
    unless you explicitly [declare them as conflicting](./config.md#conflicting-dependencies).

### Default groups

By default, uv includes the `dev` dependency group in the environment (e.g., during `uv run` or
`uv sync`). The default groups to include can be changed using the `tool.uv.default-groups` setting.

```toml title="pyproject.toml"
[tool.uv]
default-groups = ["dev", "foo"]
```

!!! tip

    To exclude a default group during `uv run` or `uv sync`, use `--no-group <name>`.

### Legacy `dev-dependencies`

Before `[dependency-groups]` was standardized, uv used the `tool.uv.dev-dependencies` field to
specify development dependencies, e.g.:

```toml title="pyproject.toml"
[tool.uv]
dev-dependencies = [
  "pytest"
]
```

Dependencies declared in this section will be combined with the contents in the
`dependency-groups.dev`. Eventually, the `dev-dependencies` field will be deprecated and removed.

!!! note

    If a `tool.uv.dev-dependencies` field exists, `uv add --dev` will use the existing section
    instead of adding a new `dependency-groups.dev` section.

## Build dependencies

If a project is structured as [Python package](./config.md#build-systems), it may declare
dependencies that are required to build the project, but not required to run it. These dependencies
are specified in the `[build-system]` table under `build-system.requires`, following
[PEP 518](https://peps.python.org/pep-0518/).

For example, if a project uses `setuptools` as its build backend, it should declare `setuptools` as
a build dependency:

```toml title="pyproject.toml"
[project]
name = "pandas"
version = "0.1.0"

[build-system]
requires = ["setuptools>=42"]
build-backend = "setuptools.build_meta"
```

By default, uv will respect `tool.uv.sources` when resolving build dependencies. For example, to use
a local version of `setuptools` for building, add the source to `tool.uv.sources`:

```toml title="pyproject.toml"
[project]
name = "pandas"
version = "0.1.0"

[build-system]
requires = ["setuptools>=42"]
build-backend = "setuptools.build_meta"

[tool.uv.sources]
setuptools = { path = "./packages/setuptools" }
```

When publishing a package, we recommend running `uv build --no-sources` to ensure that the package
builds correctly when `tool.uv.sources` is disabled, as is the case when using other build tools,
like [`pypa/build`](https://github.com/pypa/build).

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

## Dependency specifiers (PEP 508)

uv uses
[dependency specifiers](https://packaging.python.org/en/latest/specifications/dependency-specifiers/),
previously known as [PEP 508](https://peps.python.org/pep-0508/). A dependency specifier is composed
of, in order:

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
