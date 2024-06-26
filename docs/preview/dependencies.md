**Warning: This documentation refers to experimental features that may change.**

# Specifying dependencies

In uv, dependency specification is divided between two tables: `project.dependencies` and
`tool.uv.sources`.

At a high-level, the former is used to define the standards-compliant dependency metadata,
propagated when uploading to PyPI or building a wheel. The latter is used to specify the _sources_
required to install the dependencies, which can come from a Git repository, a URL, a local path, a
different index, etc.

## `project.dependencies`

The `project.dependencies` table represents the dependencies that are used when uploading to PyPI or
building a wheel. Individual dependencies are specified using [PEP 508](#pep-508), and the table as
a whole follows the [PEP 621](https://packaging.python.org/en/latest/specifications/pyproject-toml/)
standard.

You should think of `project.dependencies` as defining the packages that are required for your
project, along with the version constraints that should be used when installing them.

`project.dependencies` is structured as a list in which each entry includes a dependency name and
version, and optionally extras or environment markers for platform-specific packages, as in:

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

If you only require packages from PyPI or a single `--index-url`, then `project.dependencies` is all
you need. If, however, you depend on local packages, Git dependencies, or packages from a different
index, you should use `tool.uv.sources`.

## `tool.uv.sources`

During development, you may rely on a package that isn't available on PyPI. For example, let’s say
that we need to pull in a version of `tqdm` from a specific Git commit, `importlib_metadata` from
a dedicated URL, `torch` from the PyTorch-specific index, and `mollymawk` from our own workspace.

We can express these requirements by enriching the `project.dependencies` table with
`tool.uv.sources`:

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
# Pin a dependency for a specific registry.
torch = { index = "torch-cu118" }
# Use a package included in the same repository (editable installation).
mollymawk = { workspace = true }

# See "Workspaces".
[tool.uv.workspace]
include = [
  "packages/mollymawk"
]

# See "Indexes".
[tool.uv.indexes]
torch-cu118 = "https://download.pytorch.org/whl/cu118"
```

We support the following sources (which are mutually exclusive for a given dependency):

- Git: Use `git` with a Git URL, optionally one of `rev`, `tag`, or `branch`, and
  optionally a `subdirectoy`, if the package isn't in the repository root.
- URL: A `url` key with an `https://` URL to a wheel (ending in `.whl`) or a source distribution
  (ending in `.zip` or `.tar.gz`), and optionally a `subdirectory` if the source distribution isn't
  in the archive root.
- Path: The `path` is an absolute or relative path to a wheel (ending in `.whl`), a source
  distribution (ending in `.zip` or `.tar.gz`), or a directory containing a `pyproject.toml`. We
  recommend using workspaces over manual path dependencies. For directories, you can specify
  `editable = true` for an [editable](#Editables) installation.
- Index: Set the `index` key to the name of an index name to install it
  from this registry instead of your default index.
- Workspace: Set `workspace = true` to use the workspace dependency. You need to explicitly require
  all workspace dependencies you use. They are [editable](#Editables) by default; specify
  `editable = false` to install them as regular dependencies.

Note that if a non-uv project uses this project as a Git- or path-dependency, only
`project.dependencies` is transferred, and you'll need to apply the information in the source table
using the configuration of the other project's package manager.

## Optional dependencies

For libraries, you may want to make certain features and their dependencies optional. For example,
pandas has an [`excel` extra](https://pandas.pydata.org/docs/getting_started/install.html#excel-files)
and a [`plot` extra](https://pandas.pydata.org/docs/getting_started/install.html#visualization) to limit the installation of Excel parsers and (e.g.) `matplotlib` to
those that explicitly require them. In the case of Pandas, you can install those extras with:
`pandas[plot, excel]`.

Optional dependencies are specified in `[project.optional-dependencies]`, a TOML table that maps
from extra name to its dependencies, following the [PEP 508](#PEP 508) syntax.

`tool.uv.sources` applies to this table equally.

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

_N.B. This feature is not yet implemented._

Unlike optional dependencies, development dependencies are local-only and will _not_ be published
to PyPI or other indexes. As such, development dependencies are included under `[tool.uv]` instead
of `[project]`. `tool.uv.sources` applies to them equally.

```toml
[tool.uv]
dev-dependencies = [
  "pytest >=8.1.1,<9"
]
```

You can also put development dependencies into groups and install them individually:

```toml
[tool.uv.dev-dependencies]
test = [
  "pytest >=8.1.1,<9"
]
lint = [
  "mypy >=1,<2"
]

[tool.uv]
default-dev-dependencies = ["test"]
```

## PEP 508

The [PEP 508](https://peps.python.org/pep-0508/) syntax allows you to specify, in order:

- The dependency name
- The extras you want (optional)
- The version specifier
- An environment marker (optional)

The version specifiers are comma separated and added together, e.g., `foo >=1.2.3,<2,!=1.4.0` is
interpreted as "a version of `foo` that's at least 1.2.3, but less than 2, and not 1.4.0".

Specifiers are padded with trailing zeros if required, so `foo ==2` matches foo 2.0.0, too.

You can use a star for the last digit with equals, e.g. `foo ==2.1.*` will accept any release from
the 2.1 series. Similarly, `~=` matches where the last digit is equal or higher, e.g., `foo ~=1.2`
is equal to `foo >=1.2,<2`, and `foo ~=1.2.3` is equal to `foo >=1.2.3,<1.3`.

Extras are comma-separated in square bracket between name and version, e.g., `pandas[excel,plot] ==2.2`.

Some dependencies are only required in specific environments, e.g., a specific Python version or
operating system. For example to install the `importlib-metadata` backport for the
`importlib.metadata` module, you would use `importlib-metadata >=7.1.0,<8; python_version < '3.10'`.
To install `colorama` on Windows (but omit it on other platforms), use
`colorama >=0.4.6,<5; platform_system == "Windows"`.

You combine markers with `and` and `or` and parentheses, e.g., `aiohttp >=3.7.4,<4; (sys_platform != 'win32' or implementation_name != 'pypy') and python_version >= '3.10'`.
Note that versions within markers must be quoted, while versions _outside_ of markers must _not_ be
quoted.

## Editables

A regular installation of a directory with a Python package first builds a wheel and then installs
that wheel into your virtual environment, copying all source files. When you edit the source files,
the virtual environment will contain outdated versions.

Editable installations instead add a link to the project within the virtual environment
(a `.pth` file), which instructs the interpreter to include your sources directly.

There are some limitations to editables (mainly: your build backend needs to support them, and
native modules aren't recompiled before import), but they are useful for development, as your
virtual environment will always use the latest version of your package.

uv uses editable installation for workspace packages and patched dependencies by default.
