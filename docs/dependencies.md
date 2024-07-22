# Specifying dependencies

In uv, project dependency specification is divided between two `pyproject.toml` tables: `project.dependencies` and
`tool.uv.sources`.

`project.dependencies` is used to define the standards-compliant dependency metadata,
propagated when uploading to PyPI or building a wheel. `tool.uv.sources` is used to specify the _sources_
required to install the dependencies, which can come from a Git repository, a URL, a local path, a
different index, etc. This metadata must be expressed separately because the `project.dependencies` standard does not allow these common patterns.

## Project dependencies

The `project.dependencies` table represents the dependencies that are used when uploading to PyPI or
building a wheel. Individual dependencies are specified using [PEP 508](#pep-508) syntax, and the table follows the [PEP 621](https://packaging.python.org/en/latest/specifications/pyproject-toml/)
standard.

`project.dependencies` defines the packages that are required for the project, along with the version constraints that should be used when installing them.

`project.dependencies` is structured as a list. Each entry includes a dependency name and
version. An entry may include extras or environment markers for platform-specific packages. For example:

```toml
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

If the project only requires packages from standaard package indexes, then `project.dependencies` is sufficient. If, the project depends on packages from Git, remote URLs, or local sources, `tool.uv.sources` is needed.

## Dependency sources

During development, the project may rely on a package that isn't available on PyPI. In the following example, the project will require several package sources:

- `tqdm` from a specific Git commit
- `importlib_metadata` from a dedicated URL
- `torch` from the PyTorch-specific index
- `mollymawk` from the current workspace

These requirements can be expressed by extending the definitions in the `project.dependencies` table with `tool.uv.sources` entries:

```toml
[project]
name = "albatross"
version = "0.1.0"
dependencies = [
  # Any version in this range.
  "tqdm >=4.66.2,<5",
  # Exactly this version of torch.
  "torch ==2.2.2",
  # Install transformers with the torch extra.
  "transformers[torch] >=4.39.3,<5",
  # Only install this package on Python versions prior to 3.10.
  "importlib_metadata >=7.1.0,<8; python_version < '3.10'",
  "mollymawk ==0.1.0"
]

[tool.uv.sources]
# Install a specific Git commit.
tqdm = { git = "https://github.com/tqdm/tqdm", rev = "cc372d09dcd5a5eabdc6ed4cf365bdb0be004d44" }
# Install a remote source distribution (`.zip`, `.tar.gz`) or wheel (`.whl`).
importlib_metadata = { url = "https://github.com/python/importlib_metadata/archive/refs/tags/v7.1.0.zip" }
# Use a package included in the same workspace (as an editable installation).
mollymawk = { workspace = true }

[tool.uv.workspace]
include = [
  "packages/mollymawk"
]
```

uv supports the following sources:

- **Git**: `git = <url>`. A git-compatible URL to clone. A target revision may be specified with one of: `rev`, `tag`, or `branch`. A `subdirectory` may be specified if the package isn't in the repository root.
- **URL**: `url = <url>`. An `https://` URL to either a wheel (ending in `.whl`) or a source distribution
  (ending in `.zip` or `.tar.gz`). A `subdirectory` may be specified if the if the source distribution isn't in the archive root.
- **Path**: `path = <path>`. A path to a wheel (ending in `.whl`), a source
  distribution (ending in `.zip` or `.tar.gz`), or a directory containing a `pyproject.toml`. 
  The path may be absolute or relative path. It is recommended to use _workspaces_ instead of manual path dependencies. For directories, `editable = true` may be specified for an [editable](#editables-dependencies) installation.
- **Workspace**: `workspace = true`. All workspace dependencies must be explicitly stated. Workspace dependencies are [editable](#editables-dependencies) by default; `editable = false` may be specified to install them as regular dependencies. See the [workspace](./workspaces.md) documentation for more details on workspaces.

Only a single source may be defined for each dependency.

Note that if a non-uv project uses this project as a Git- or path-dependency, only
`project.dependencies` is respected, the information in the source table
will need to be specified in a format specific to the other package manager.

## Optional dependencies

It is common for projects that are published as libraries to make some features optional to reduce the default dependency tree. For example,
Pandas has an [`excel` extra](https://pandas.pydata.org/docs/getting_started/install.html#excel-files)
and a [`plot` extra](https://pandas.pydata.org/docs/getting_started/install.html#visualization) to avoid installation of Excel parsers and `matplotlib` unless someone explicitly requires them. Extras are requested with the `package[<extra>]` syntax, e.g., `pandas[plot, excel]`.

Optional dependencies are specified in `[project.optional-dependencies]`, a TOML table that maps
from extra name to its dependencies, following [PEP 508](#pep-508) syntax.

Optional dependencies can have entries in `tool.uv.sources` the same as normal dependencies.

```toml
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

## Development dependencies

Unlike optional dependencies, development dependencies are local-only and will _not_ be included in the project requirements when published to PyPI or other indexes. As such, development dependencies are included under `[tool.uv]` instead of `[project]`. 

Development dependencies can have entries in `tool.uv.sources` the same as normal dependencies.

```toml
[tool.uv]
dev-dependencies = [
  "pytest >=8.1.1,<9"
]
```

## PEP 508

[PEP 508](https://peps.python.org/pep-0508/) defines a syntax for dependency specification. It is composed of, in order:

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

Extras are comma-separated in square bracket between name and version, e.g., `pandas[excel,plot] ==2.2`. Whitespace between extra names is ignored.

Some dependencies are only required in specific environments, e.g., a specific Python version or
operating system. For example to install the `importlib-metadata` backport for the
`importlib.metadata` module, use `importlib-metadata >=7.1.0,<8; python_version < '3.10'`.
To install `colorama` on Windows (but omit it on other platforms), use
`colorama >=0.4.6,<5; platform_system == "Windows"`.

Markers are combined with `and`, `or`, and parentheses, e.g., `aiohttp >=3.7.4,<4; (sys_platform != 'win32' or implementation_name != 'pypy') and python_version >= '3.10'`.
Note that versions within markers must be quoted, while versions _outside_ of markers must _not_ be
quoted.

## Editable dependencies

A regular installation of a directory with a Python package first builds a wheel and then installs
that wheel into your virtual environment, copying all source files. When the package source files are edited,
the virtual environment will contain outdated versions.

Editable installations solve this problem by adding a link to the project within the virtual environment
(a `.pth` file), which instructs the interpreter to include the source files directly.

There are some limitations to editables (mainly: the build backend needs to support them, and
native modules aren't recompiled before import), but they are useful for development, as the
virtual environment will always use the latest changes to the package.

uv uses editable installation for workspace packages and patched dependencies by default.
