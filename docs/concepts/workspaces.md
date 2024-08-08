# Workspaces

Workspaces help organize large codebases by splitting them into multiple packages with independent
dependencies. Each package in a workspace has its own `pyproject.toml`, but they are all locked
together in a shared lockfile and installed to shared virtual environment.

Using the project interface, `uv run` and `uv sync` will install all packages of the workspace,
unless you select a single workspace member with `--package`. When using the `uv pip` interface,
workspace dependencies behave like editable path dependencies.

## When (not) to use workspaces

One common use case for a workspace is that the codebase grows large, and eventually you want some
modules to become independent packages with their own dependency specification. Other use cases are
separating parts of the codebase with different responsibilities, e.g. in a repository with a
library package and CLI package, where the CLI package makes features of the library available but
has additional dependencies, a webserver with a backend and an ingestion package, or a library that
has a performance-critical subroutine implemented in a native language.

Workspaces are not suited when you don't want to install all members together, members have
conflicting requirements, or you simply want individual virtual environments per project. In this
case, use regular (editable) relative path dependencies.

Currently, workspace don't properly support different members having different `requires-python`
values, we apply the highest of all `requires-python` lower bounds to the entire workspace. You need
to use a `uv pip` to install individual member in an older virtual environment.

!!! note

    As Python does not provide dependency isolation, uv can't ensure that a package uses only the dependencies it has declared, and not also imports a package that was installed for another dependency. For workspaces specifically, uv can't ensure that packages don't import dependencies declared by another workspace member.

## Usage

A workspace can be created by adding a `tool.uv.workspace` table to a `pyproject.toml` that will
become the workspace root. This table contains `members` (mandatory) and `exclude` (optional), with
lists of globs of directories:

```toml title="pyproject.toml"
[tool.uv.workspace]
members = ["packages/*", "examples/*"]
exclude = ["example/excluded_example"]
```

`uv.lock` and `.venv` for the entire workspace are created next to this `pyproject.toml`. All
members need to be in directories below it.

If `tool.uv.sources` is defined in the workspace root, it applies to all members, unless overridden
in the `tool.uv.sources` of a specific member.

Using `uv init` inside a workspace will add the newly created package to `members`.

## Common structures

There a two main workspace structures: A **root package with helpers** and a **flat workspace**.

The root workspace layout defines one main package in the root of the repository, with helper
packages in `packages`. In this example `albatross/pyproject.toml` has both a `project` section and
a `tool.uv.workspace` section.

```text
albatross
├── packages
│   ├── provider_a
│   │   ├── pyproject.toml
│   │   └── src
│   │       └── provider_a
│   │           ├── __init__.py
│   │           └── foo.py
│   └── provider_b
│       ├── pyproject.toml
│       └── src
│           └── provider_b
│               ├── __init__.py
│               └── bar.py
├── pyproject.toml
├── README.md
├── uv.lock
└── src
    └── albatross
        └── main.py
```

In the flat layout, all packages are in the `packages` directory, and the root `pyproject.toml`
defines a so-called virtual workspace. In this example `albatross/pyproject.toml` has only a
`tool.uv.workspace` section, but no `project`.

```text
albatross
├── packages
│   ├── albatross
│   │   ├── pyproject.toml
│   │   └── src
│   │       └── albatross
│   │           ├── __init__.py
│   │           └── foo.py
│   ├── provider_a
│   │   ├── pyproject.toml
│   │   └── src
│   │       └── provider_a
│   │           ├── __init__.py
│   │           └── foo.py
│   └── provider_b
│       ├── pyproject.toml
│       └── src
│           └── provider_b
│               ├── __init__.py
│               └── bar.py
├── pyproject.toml
├── README.md
└── uv.lock
```

In the flat layout, you may still define development dependencies in the workspace root
`pyproject.toml`:

```toml title="pyproject.toml"
[tool.uv.workspace]
members = ["packages/*"]

[tool.uv]
dev-dependencies = [
  "pytest >=8.3.2,<9"
]
```
