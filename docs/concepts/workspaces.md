# Workspaces

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
[tool.uv.workspace]
members = ["packages/*", "examples/*"]
exclude = ["example/excluded_example"]
```

In this example, the workspace includes all packages in the `packages` directory and all examples in
the `examples` directory, with the exception of the `example/excluded_example` directory.

Every directory included by the `members` globs (and not excluded by the `exclude` globs) must
contain a `pyproject.toml` file; in other words, every member must be a valid Python package, or
workspace discovery will raise an error.

## Workspace roots

Every workspace needs a workspace root, which can either be explicit or "virtual".

An explicit root is a directory that is itself a valid Python package, and thus a valid workspace
member, as in:

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
requires = ["hatchling"]
build-backend = "hatchling.build"
```

A virtual root is a directory that is _not_ a valid Python package, but contains a `pyproject.toml`
with a `tool.uv.workspace` table. In other words, the `pyproject.toml` exists to define the
workspace, but does not itself define a package, as in:

```toml title="pyproject.toml"
[tool.uv.workspace]
members = ["packages/*"]
```

A virtual root _must not_ contain a `[project]` table, as the inclusion of a `[project]` table
implies the directory is a package, and thus an explicit root. As such, virtual roots cannot define
their own dependencies; however, they _can_ define development dependencies as in:

```toml title="pyproject.toml"
[tool.uv.workspace]
members = ["packages/*"]

[tool.uv]
dev-dependencies = ["ruff==0.5.0"]
```

By default, `uv run` and `uv sync` operates on the workspace root, if it's explicit. For example, in
the above example, `uv run` and `uv run --package albatross` would be equivalent. For virtual
workspaces, `uv run` and `uv sync` instead sync all workspace members, since the root is not a
member itself.

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
requires = ["hatchling"]
build-backend = "hatchling.build"
```

In this example, the `albatross` package depends on the `bird-feeder` package, which is a member of
the workspace. The `workspace = true` key-value pair in the `tool.uv.sources` table indicates the
`bird-feeder` dependency should be provided by the workspace, rather than fetched from PyPI or
another registry.

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
requires = ["hatchling"]
build-backend = "hatchling.build"
```

Every workspace member would, by default, install `tqdm` from GitHub, unless a specific member
overrides the `tqdm` entry in its own `tool.uv.sources` table.

## Workspace layouts

In general, there are two common layouts for workspaces, which map to the two kinds of workspace
roots: a **root package with helpers** (for explicit roots) and a **flat workspace** (for virtual
roots).

In the former case, the workspace includes an explicit workspace root, with peripheral packages or
libraries defined in `packages`. For example, here, `albatross` is an explicit workspace root, and
`bird-feeder` and `seeds` are workspace members:

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

In the latter case, _all_ members are located in the `packages` directory, and the root
`pyproject.toml` comprises a virtual root:

```text
albatross
├── packages
│   ├── albatross
│   │   ├── pyproject.toml
│   │   └── src
│   │       └── albatross
│   │           ├── __init__.py
│   │           └── foo.py
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
└── uv.lock
```

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
requires = ["hatchling"]
build-backend = "hatchling.build"
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
