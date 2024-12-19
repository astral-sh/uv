# Pyproject.toml

`pyproject.toml` is a
[standardized](https://packaging.python.org/en/latest/specifications/pyproject-toml/) file for
specifying the metadata and build system of a Python project. See [Projects](../guides/projects.md)
for an introduction.

Most of uv's subcommands only read the name, version, (optional) dependencies and build system of a
project from its `pyproject.toml`. The `project.name` is always required, while `project.version`
may be dynamic in some build backends. If `project.dependencies` is not specified, it means that the
project has no dependencies. If you need to use dynamic dependencies (discouraged), you must add
`dependencies` to `project.dynamic`. The same applies to `optional-dependencies`.

When building a package, these fields get translated to
[Core Metadata](https://packaging.python.org/en/latest/specifications/core-metadata) in the source
distribution and wheel.

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

The recommended way to specify license information is using the `license` field, which takes an
[SPDX Expression](https://spdx.org/licenses/), and `license-files`, which takes a list of globs of
license files to include:

```toml
license = "MIT OR Apache-2.0"
license-files = ["LICENSE.apache", "LICENSE.mit", "_vendor/licenses/*"]
```

There are two old variant of the `license` key that are also supported. `license.file` specifies the
path to a license file to be included, `license.text` is included in the wheel metadata.

```toml
license = { file = "LICENSE" }
```

```toml
license = { text = "Lorem ipsum dolor sit amet\nconsetetur sadipscing elitr." }
```

These variants may not be combined with setting `license-files`.

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

Links to important pages of the project. The following labels are known to be supported:

- `changelog` (Changelog): The project's comprehensive changelog
- `documentation` (Documentation): The project's online documentation
- `download` (Download): A download URL for the current distribution
- `funding` (Funding): Funding Information
- `homepage` (Homepage): The project's home page
- `issues` (Issue Tracker): The project's bug tracker
- `releasenotes` (Release Notes): The project's curated release notes
- `source` (Source Code): The project's hosted source code or repository

```toml
[project.urls]
changelog = "https://github.com/astral-sh/uv/blob/main/CHANGELOG.md"
documentation = "https://docs.astral.sh/uv"
releases = "https://github.com/astral-sh/uv/releases"
repository = "https://github.com/astral-sh/uv"
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

Dynamic metadata is not supported. Please specify all metadata statically.

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
foo-gui = "foo.gui:main"

[project.entry-points.bar_group]
foo-bar = "foo:bar"

[build-system]
requires = ["uv>=0.4.15,<5"]
build-backend = "uv"
```
