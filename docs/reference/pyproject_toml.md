# Pyproject.toml

`pyproject.toml` is a
[standardized](https://packaging.python.org/en/latest/specifications/pyproject-toml/) file for
specifying the metadata and build system of a Python project. See [Projects](../guides/projects.md)
for an introduction.

Most parts of uv only consider the name, version, (optional) dependencies and build system of a
project, and read only those fields from `pyproject.toml`. The `project.name` is always required,
while `project.version`. If `project.dependencies` is not specified, it means that the project has
no dependencies. If you need to use dynamic dependencies (discouraged), you must add `dependencies`
to `project.dynamic`. The same applies to `optional-dependencies`.

For the build backend (`build-system = "uv"`), all fields are relevant and get translated to
[Core Metadata](https://packaging.python.org/en/latest/specifications/core-metadata) in the final
field. uv supports the
[living standard](https://packaging.python.org/en/latest/specifications/core-metadata) in addition
to the provisional [PEP 639](https://peps.python.org/pep-0639/) for better upload metadata. When
using the uv build backend, the `project.name`, `project.version`, `build-system.requires` and
`build-system.build-backend` keys are required, `project.dynamic` is not supported, and all other
fields are optional.

## The `[build-system]` table

The may be breaking changes to the uv build backend configuration in future uv versions, so you
constrain the uv version with lower and upper bounds.

```toml
[build-system]
requires = ["uv>=0.4.15,<5"]
build-backend = "uv"
```

## The `[project]` table

The following fields are recognized in the `[project]` table.

### `name`

The name of the project. The name of the package should match the name of the Python module it
contains. The name can contain runs of `-`, `_`, and `.`, which uv internally replaces by a single
`-` (or `_` in filenames).

### `version`

The version of the project, following the
[Version Specifiers](https://packaging.python.org/en/latest/specifications/version-specifiers/)
rules. Examples: `1.2.3`, `1.2.3-alpha.4`, `1.2.3-beta.1`, `1.2.3-rc.3` and `2.0.0+cpu`.

### `description`

A short, single-line description of the project.

### `readme`

The path to the Readme, relative to project root.

Three forms are supported:

1. A single string value, containing the relative path to the Readme.

   ```toml
   readme = "path/to/Readme.md"
   ```

2. A table with the relative path to the Readme and a content type, one of `text/plain`,
   `text/x-rst`, `text/markdown`.

   ```toml
   readme = { file = "path/to/Readme.md", content-type = "text/markdown"  }
   ```

3. A table with the Readme text inline and a content type, one of `text/plain`, `text/x-rst`,
   `text/markdown`.

   ```toml
   readme = { text = "# Description\n\nThe project description", content-type = "text/markdown" }
   ```

### `requires-python`

The minimum supported Python version as version specifiers, for example `>=3.10`. Adding an upper
bound or anything other than a lower bound is not recommended. While uv does preserve the specifiers
as written, the resolver does only use the lower bound on published packages.

### `license` and `license-files`

The packaging ecosystem is currently transitioning from packaging core metadata version 2.3 to
version 2.4, which brings an overhaul of the license metadata.

In version 2.3, the two variants of the `license` key are supported, while `license-files` is not
supported:

```toml
license = { file = "LICENSE" }
```

```toml
license = { text = "Lorem ipsum dolor sit amet\nconsetetur sadipscing elitr." }
```

In version 2.4, the `license` key is an [SPDX Expression](https://spdx.org/licenses/) and
`license-files` is a list of glob expression of license files to include:

```toml
license = "MIT OR Apache-2.0"
license-files = ["LICENSE.apache", "LICENSE.mit", "_vendor/licenses/*"]
```

When using both `license-files` and `license`, `license` must be a valid SPDX expression. Using
`license` with a string or specifying `license-files` increases the default metadata version from
2.3 to 2.4. At time of writing, PyPI does not support publishing packages using version 2.4.

### `authors` and `maintainers`

Name and/or email address for the authors or maintainers of the project. Either a `name` key or an
`email` key must be present.

```toml
authors = [
  { name = "Ferris the crab", email = "ferris@example.net" },
  { name = "The project authors" },
  { email = "say-hi@example.org" }
]
```

### `keywords`

List of terms that make the project easier to discover.

```toml
keywords = ["uv", "requirements", "packaging"]
```

### `classifiers`

List of [Trove classifiers](https://pypi.org/classifiers/) describing the project.

```toml
classifiers = [
  "Development Status :: 4 - Beta",
  "Programming Language :: Python :: 3.11",
  "Programming Language :: Python :: 3.12",
]
```

To prevent a private project from accidentally being uploaded to PyPI, add the
`"Private :: Do Not Upload"` classifier.

### `urls`

Links to important pages of the project

```toml
[project.urls]
Repository = "https://github.com/astral-sh/uv"
Documentation = "https://docs.astral.sh/uv"
Changelog = "https://github.com/astral-sh/uv/blob/main/CHANGELOG.md"
Releases = "https://github.com/astral-sh/uv/releases"
```

### `scripts`, `gui-scripts` and `entry-points`

`scripts` define a mapping from a name to a Python function. When installing the package, the
installer adds a launcher in `.venv/bin` (Unix) or `.venv\Scripts` (Windows) with that name that
launches the Python function. The Python function is given as the import-path to a module, separated
by dots, followed by a colon (`:`) and an argument-less function inside that module that will be
called. On Windows, starting a script by default creates a terminal. You can suppress this by using
`gui-scripts` instead. On other platforms, there is no difference between `scripts` and
`gui-scripts`.

`entry-points` can define additional name/Python function mappings that can be read across packages
with [`importlib.metadata`](https://docs.python.org/3/library/importlib.metadata.html#entry-points),
which is useful for plugin interfaces.

```toml
[project.scripts]
foo = "foo.cli:launch"

[project.entry-points.bar_group]
foo-bar = "foo:bar"
```

### `dependencies` and `optional-dependencies`

See [Dependencies](../concepts/dependencies.md).

### `dynamic`

Dynamic metadata is not support. Please specify all metadata statically.

## Full example

```toml
[project]
name = "foo"
version = "0.1.0"
description = "A Python package"
readme = "Readme.md"
requires_python = ">=3.12"
license = { file = "License.txt" }
authors = [{ name = "Ferris the crab", email = "ferris@rustacean.net" }]
maintainers = [{ name = "Konsti", email = "konstin@mailbox.org" }]
keywords = ["demo", "example", "package"]
classifiers = [
  "Development Status :: 6 - Mature",
  "License :: OSI Approved :: MIT License",
  "Programming Language :: Python",
]
dependencies = ["flask>=3,<4", "sqlalchemy[asyncio]>=2.0.35<3"]

[project.optional-dependencies]
postgres = ["psycopg>=3.2.2,<4"]
mysql = ["pymysql>=1.1.1,<2"]

[project.urls]
"Homepage" = "https://github.com/astral-sh/uv"
"Repository" = "https://astral.sh"

[project.scripts]
foo = "foo.cli:__main__"

[project.gui-scripts]
foo-gui = "foo.gui"

[project.entry-points.bar_group]
foo-bar = "foo:bar"

[build-system]
requires = ["uv>=0.4.15,<5"]
build-backend = "uv"
```
