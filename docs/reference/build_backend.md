# Build backend

uv comes with a build backend implementation (`build-system = "uv"`). You can use uv with this build
backend, or with any other build backend.

The uv build backend is configured through `pyproject.toml`. It uses the standard `[project]` and
`[build-system]` tables as well as the `[tool.uv.build-backend]` table for configuration . See
[pyproject.toml](#pyproject_toml) for the `[project]` and `[build-system]` table.

The uv build backend requires `project.name`, `project.version`, `build-system.requires` and
`build-system.build-backend`, does not support `project.dynamic`, and all other fields are optional.

### Example

```toml
[project]
name = "built-by-uv"
version = "0.1.0"
description = "A package that is built with the uv build backend"
readme = "README.md"
requires-python = ">=3.12"
dependencies = ["anyio>=4,<5"]
license-files = ["LICENSE*", "third-party-licenses/*"]

[tool.uv.build-backend]
# A file we need for the source dist -> wheel step, but not in the wheel itself (currently unused)
source-include = ["data/build-script.py"]
# A temporary or generated file we want to ignore
source-exclude = ["/src/built_by_uv/not-packaged.txt"]
# Headers are build-only
wheel-exclude = ["build-*.h"]

[tool.uv.build-backend.data]
scripts = "scripts"
data = "assets"

[build-system]
requires = ["uv>=0.5,<0.6"]
build-backend = "uv"
```

## The `[build-system]` table

There may be breaking changes to the uv build backend configuration in future uv versions, so you
need to constrain the uv version with lower and upper bounds.

```toml
[build-system]
requires = ["uv>=0.5.5,<6"]
build-backend = "uv"
```

## Include and exclude configuration

By default, uv expects your code to be in `src/<package_name_with_underscores>`.

To select which files to include in the source distribution, we first add the includes, then remove
the excludes from that. You can check the file list with `uv build --preview --list`.

When building the source distribution, the following files and directories are included:

- `pyproject.toml`
- The module under `tool.uv.build-backend.module-root`, by default
  `src/<project_name_with_underscores>/**`.
- `project.license-files` and `project.readme`.
- All directories under `tool.uv.build-backend.data`.
- All patterns from `tool.uv.build-backend.source-include`.

From these, we remove the `tool.uv.build-backend.source-exclude` matches.

When building the wheel, the following files and directories are included:

- The module under `tool.uv.build-backend.module-root`, by default
  `src/<project_name_with_underscores>/**`.
- `project.license-files` and `project.readme`, as part of the project metadata.
- Each directory under `tool.uv.build-backend.data`, as data directories.

From these, we remove the `tool.uv.build-backend.source-exclude` and
`tool.uv.build-backend.wheel-exclude` matches. The source dist excludes are applied to avoid source
tree -> wheel source including more files than source tree -> source distribution -> wheel.

There are no specific wheel includes. There must only be one top level module, and all data files
must either be under the module root or in a data directory. Most packages store small data in the
module root alongside the source code.

## Include and exclude syntax

Includes are anchored, which means that `pyproject.toml` includes only
`<project root>/pyproject.toml`. Use for example `assets/**/sample.csv` to include for all
`sample.csv` files in `<project root>/assets` or any child directory. To recursively include all
files under a directory, use a `/**` suffix, e.g. `src/**`. For performance and reproducibility,
avoid unanchored matches such as `**/sample.csv`.

Excludes are not anchored, which means that `__pycache__` excludes all directories named
`__pycache__` and it's children anywhere. To anchor a directory, use a `/` prefix, e.g., `/dist`
will exclude only `<project root>/dist`.

The glob syntax is the reduced portable glob from
[PEP 639](https://peps.python.org/pep-0639/#add-license-FILES-key).

## Options

**tool.uv.build-backend.module-root**

The directory that contains the module directory, usually `src`, or an empty path when using the
flat layout over the src layout.

**tool.uv.build-backend.source-include**

Glob expressions which files and directories to additionally include in the source distribution.

`pyproject.toml` and the contents of the module directory are always included.

The glob syntax is the reduced portable glob from
[PEP 639](https://peps.python.org/pep-0639/#add-license-FILES-key).

**tool.uv.build-backend.default-excludes**

If set to `false`, the default excludes aren't applied.

Default excludes: `__pycache__`, `*.pyc`, and `*.pyo`.

**tool.uv.build-backend.source-excludes**

Glob expressions which files and directories to exclude from the source distribution.

**tool.uv.build-backend.wheel-excludes**

Glob expressions which files and directories to exclude from the wheel.

**tool.uv.build-backend.data**

Data includes for wheels.

The directories included here are also included in the source distribution. They are copied to the
right wheel subdirectory on build.
