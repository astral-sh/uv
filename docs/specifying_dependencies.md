## Preview

**This documentation is a draft for a future version of uv. Please refer to the readme for the
current documentation.**

# Specifying dependencies

The dependency specification in uv is split into two parts: The `project.dependencies` list and the `tool.uv.sources` table.

`project.dependencies` is the information that is published when uploading the package to pypi or generally when building a wheel ([full reference](#PEP 508)). It uses the [PEP 621](https://packaging.python.org/en/latest/specifications/pyproject-toml/) standard. In this list, you specify the dependency name and version, and optionally extras you want to install or environment markers for platform specific packages:

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

During development, you sometimes don’t want to use a version from PyPI or your default index, but from a different source. Let’s say for tqdm we want a specific Git commit, we want importlib_metadata from a URL, torch needs to be installed from their [index](TODO(konsti): write index docs) and mollymawk is a package in our [workspace](TODO(konsti): write workspace docs). We can express that with `tool.uv.sources`.

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

[tool.uv.sources]
# Install a specific Git commit
tqdm = { git = "https://github.com/tqdm/tqdm", rev = "cc372d09dcd5a5eabdc6ed4cf365bdb0be004d44" }
# Install a remote source distribution (`.zip`, `.tar.gz`) or wheel (`.whl`)
importlib_metadata = { url = "https://github.com/python/importlib_metadata/archive/refs/tags/v7.1.0.zip" }
# Pin a dependency for a specific registry
torch = { index = "torch-cu118" }
# Use a package included in the same repository (editable installation)
mollymawk = { workspace = true }

# See "Workspaces"
[tool.uv.workspace]
include = [
  "packages/mollymawk"
]

# See "Indexes"
[tool.uv.indexes]
torch-cu118 = "https://download.pytorch.org/whl/cu118"
```

We support the following sources (mutually exclusive):

- Git: Use `git` with a Git url, optionally one of `rev`, `tag` or `branch` and optionally `subdirectoy` if the package isn't in the repository root.
- Url: A `url` key with a `https://` URL to a wheel (ending in `.whl`) or a source distribution (ending in `.zip` or `.tar.gz`), and optionally `subdirectory` for source distributions if the package isn't in the archive root.
- Path: The `path` is an absolute or relative path to a wheel (ending in `.whl`), a source distribution (ending in `.zip` or `.tar.gz`) or a directory containing a `pyproject.toml`. We recommend using workspaces, the replace section or a [flat index](TODO(konsti): Flat index docs) over manual path dependencies. For directories, you can specify `editable = true` for an [editable](TODO(konsti): Editable install docs) install.
- Index: Set the `index` key to the name of an [index](TODO(konsti): Index docs) name to install it from this registry instead of your default index.
- Workspace: Set `workspace = true` to use the workspace dependency. You need to explicitly require all workspace dependencies you use. They are [editable](TODO(konsti): Editable install docs) by default, use `editable = false` to install them as regular dependencies.

Note that if a non-uv project uses this project as a git- or path-dependency, only `project.dependencies` is transferred, you need to apply the information in the source table using the configuration of the other project’s package manager.

## Optional dependencies

For libraries, you may want to make certain features and their dependencies optional. For example, pandas has an [excel extra](https://pandas.pydata.org/docs/getting_started/install.html#excel-files) and a [plot extra](https://pandas.pydata.org/docs/getting_started/install.html#visualization) to only install excel parsers and matplotlib when you need them. You can install them with `pandas[plot,excel]`. Optional dependencies are specified in `[project.optional-dependencies]`, a dict from extra name to its dependencies ([PEP 508](#PEP 508) syntax). `tool.uv.sources` applies to this table equally.

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

TODO: recursive inclusion?

## Development dependencies

NOTE: This feature is not implemented

Unlike optional dependencies, dev dependencies are local-only and do not get published. That’s why they are under `[tool.uv]` instead of `[project]`. `tool.uv.sources` applies to them equally.

```toml
[tool.uv]
dev-dependencies = [
  "pytest >=8.1.1,<9"
]
```

You can also put dev dependencies into groups and install them individually with […]

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

* The dependency name
* The extras you want (optional)
* The version specifier
* An environment marker (optional)

The version specifiers are comma separated and added together, e.g. `foo >=1.2.3,<2,!=1.4.0` means a version of foo of at least 1.2.3 but lower than 2, but not 1.4.0. They are padded with trailing zeros if required, so `foo ==2` matches foo 2.0.0, too. You can use a star for the last digit with equals, e.g. `foo ==2.1.*` will accept any release from the 2.1 series. Similarly, `~=` matches where the last digit is equal or higher, e.g. `foo ~=1.2` is equal to `foo >=1.2,<2` and `foo ~=1.2.3` is equal to `foo >=1.2.3,<1.3`.

Extras are comma separated in square bracket between name and version, e.g. `pandas[excel,plot] ==2.2`.

Some dependencies are only required in specific environment, e.g. a specific python version or operating system. For example to install the `importlib-metadata` backport for the `importlib.metadata` std module, you would use `importlib-metadata >=7.1.0,<8; python_version < '3.10'`. To install colorama only on windows, use `colorama >=0.4.6,<5; platform_system == "Windows"`. You combine markers with `and` and `or` and parentheses, e.g. `aiohttp >=3.7.4,<4; (sys_platform != 'win32' or implementation_name != 'pypy') and python_version >= '3.10'`. Note that in markers, you have to quote versions, while in the regular version specifier, you must not quote versions.

## Editables

A regular installation of a directory with a Python package first builds a wheel and then installs that wheel into your virtual environment, copying all source files. When you edit the source files, the virtual environment will have outdated versions. Editable installations instead don't copy the python files, but put a link into the virtual environment (a `.pth` file) that makes the Python interpreter include your sources directly. This is helpful when working with multiple python packages at the same time. There are limitations to editables, mainly that your build backend needs to support them and that they don't work with native modules (the native module isn't recompiled before import). Uv uses editable installation for workspace packages and patched dependencies by default.
