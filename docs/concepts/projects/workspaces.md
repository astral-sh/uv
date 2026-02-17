# Using workspaces

Inspired by the [Cargo](https://doc.rust-lang.org/cargo/reference/workspaces.html) concept of the
same name, a workspace is "a collection of one or more packages, called _workspace members_, that
are managed together."

Workspaces organize large codebases by splitting them into multiple packages with common
dependencies. Think: a FastAPI-based web application, alongside a series of libraries that are
versioned and maintained as separate Python packages, all in the same Git repository.

In a workspace, each package defines its own `pyproject.toml`, but the workspace shares a single
lockfile, ensuring that the workspace operates with a consistent set of dependencies.

As such, `uv lock` operates on the entire workspace at once, while `uv run` and `uv sync` operate on
the workspace root by default, though both accept a `--package` argument, allowing you to run a
command in a particular workspace member from any workspace directory.

## Getting started

To create a workspace, add a `tool.uv.workspace` table to a `pyproject.toml`, which will implicitly
create a workspace rooted at that package.

!!! tip

    By default, running `uv init` inside an existing package will add the newly created member to the workspace, creating a `tool.uv.workspace` table in the workspace root if it doesn't already exist.

In defining a workspace, you must specify the `members` (required) and `exclude` (optional) keys,
which direct the workspace to include or exclude specific directories as members respectively, and
accept lists of globs:

```toml title="pyproject.toml"
[project]
name = "albatross"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["bird-feeder", "tqdm>=4,<5"]

[tool.uv.sources]
bird-feeder = { workspace = true }

[tool.uv.workspace]
members = ["packages/*"]
exclude = ["packages/seeds"]
```

Every directory included by the `members` globs (and not excluded by the `exclude` globs) must
contain a `pyproject.toml` file. However, workspace members can be _either_
[applications](./init.md#applications) or [libraries](./init.md#libraries); both are supported in
the workspace context.

Every workspace needs a root, which is _also_ a workspace member. In the above example, `albatross`
is the workspace root, and the workspace members include all projects under the `packages`
directory, except `seeds`.

By default, `uv run` and `uv sync` operates on the workspace root. For example, in the above
example, `uv run` and `uv run --package albatross` would be equivalent, while
`uv run --package bird-feeder` would run the command in the `bird-feeder` package.

## Workspace sources

Within a workspace, dependencies on workspace members are facilitated via
[`tool.uv.sources`](./dependencies.md), as in:

```toml title="pyproject.toml"
[project]
name = "albatross"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["bird-feeder", "tqdm>=4,<5"]

[tool.uv.sources]
bird-feeder = { workspace = true }

[tool.uv.workspace]
members = ["packages/*"]

[build-system]
requires = ["uv_build>=0.10.4,<0.11.0"]
build-backend = "uv_build"
```

In this example, the `albatross` project depends on the `bird-feeder` project, which is a member of
the workspace. The `workspace = true` key-value pair in the `tool.uv.sources` table indicates the
`bird-feeder` dependency should be provided by the workspace, rather than fetched from PyPI or
another registry.

!!! note

    Dependencies between workspace members are editable.

Any `tool.uv.sources` definitions in the workspace root apply to all members, unless overridden in
the `tool.uv.sources` of a specific member. For example, given the following `pyproject.toml`:

```toml title="pyproject.toml"
[project]
name = "albatross"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["bird-feeder", "tqdm>=4,<5"]

[tool.uv.sources]
bird-feeder = { workspace = true }
tqdm = { git = "https://github.com/tqdm/tqdm" }

[tool.uv.workspace]
members = ["packages/*"]

[build-system]
requires = ["uv_build>=0.10.4,<0.11.0"]
build-backend = "uv_build"
```

Every workspace member would, by default, install `tqdm` from GitHub, unless a specific member
overrides the `tqdm` entry in its own `tool.uv.sources` table.

!!! note

    If a workspace member provides `tool.uv.sources` for some dependency, it will ignore any
    `tool.uv.sources` for the same dependency in the workspace root, even if the member's source is
    limited by a [marker](dependencies.md#platform-specific-sources) that doesn't match the current
    platform.

## Workspace layouts

The most common workspace layout can be thought of as a root project with a series of accompanying
libraries.

For example, continuing with the above example, this workspace has an explicit root at `albatross`,
with two libraries (`bird-feeder` and `seeds`) in the `packages` directory:

```text
albatross
├── packages
│   ├── bird-feeder
│   │   ├── pyproject.toml
│   │   └── src
│   │       └── bird_feeder
│   │           ├── __init__.py
│   │           └── foo.py
│   └── seeds
│       ├── pyproject.toml
│       └── src
│           └── seeds
│               ├── __init__.py
│               └── bar.py
├── pyproject.toml
├── README.md
├── uv.lock
└── src
    └── albatross
        └── main.py
```

Since `seeds` was excluded in the `pyproject.toml`, the workspace has two members total: `albatross`
(the root) and `bird-feeder`.

## When (not) to use workspaces

Workspaces are intended to facilitate the development of multiple interconnected packages within a
single repository. As a codebase grows in complexity, it can be helpful to split it into smaller,
composable packages, each with their own dependencies and version constraints.

Workspaces help enforce isolation and separation of concerns. For example, in uv, we have separate
packages for the core library and the command-line interface, enabling us to test the core library
independently of the CLI, and vice versa.

Other common use cases for workspaces include:

- A library with a performance-critical subroutine implemented in an extension module (Rust, C++,
  etc.).
- A library with a plugin system, where each plugin is a separate workspace package with a
  dependency on the root.

Workspaces are _not_ suited for cases in which members have conflicting requirements, or desire a
separate virtual environment for each member. In this case, path dependencies are often preferable.
For example, rather than grouping `albatross` and its members in a workspace, you can always define
each package as its own independent project, with inter-package dependencies defined as path
dependencies in `tool.uv.sources`:

```toml title="pyproject.toml"
[project]
name = "albatross"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["bird-feeder", "tqdm>=4,<5"]

[tool.uv.sources]
bird-feeder = { path = "packages/bird-feeder" }

[build-system]
requires = ["uv_build>=0.10.4,<0.11.0"]
build-backend = "uv_build"
```

This approach conveys many of the same benefits, but allows for more fine-grained control over
dependency resolution and virtual environment management (with the downside that `uv run --package`
is no longer available; instead, commands must be run from the relevant package directory).

Finally, uv's workspaces enforce a single `requires-python` for the entire workspace, taking the
intersection of all members' `requires-python` values. If you need to support testing a given member
on a Python version that isn't supported by the rest of the workspace, you may need to use `uv pip`
to install that member in a separate virtual environment.

!!! note

    As Python does not provide dependency isolation, uv can't ensure that a package uses its declared dependencies and nothing else. For workspaces specifically, uv can't ensure that packages don't import dependencies declared by another workspace member.
