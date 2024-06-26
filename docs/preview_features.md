# Preview features

We are working on a new set of features for uv outside the `uv pip` interface that will give uv similar features to poetry, pdm, hatch or cargo. This includes automatic venv and dependency management, lockfiles, development dependencies, declaring relative path and editable dependencies in pyproject.toml and workspaces. We're interested in hearing from you, does the current design work for your projects, what are we missing and what works great.

## Basics

With the new interface, you configure everything in your pyproject.toml (or uv.toml) and run your code with `uv run path/to/file.py`. There is no more manual venv management or installing dependencies, `uv run` does this under the hood.

You can select the python version to use with `uv run -p 3.x` or select packages on the fly with `--with`.

## Lockfiles

Our platform- and python version independent lockfile format called `uv.lock` makes sure you're running with the same set of dependencies everywhere. The file will live next to your `pyproject.toml`. The universal lockfile contains all possible dependencies and we'll install the subset that's applicable, e.g. we won't install windows-only deps when you're on linux. The lockfile is update automatically when you're changing your dependencies in pyproject.toml. You can also manually update it (`uv lock`) or only install them (`uv sync`).

## Development dependencies

Development dependencies are like an extra/a group in `project.optional-dependencies`, but they are not published with your package. They are installed if you pass `--dev` to `uv run` or `uv sync`:

```toml
[tool.uv]
dev-dependencies = ["pytest >=8.2.2,<9", "ruff>=0.4.8,<0.5"]

```

## `tool.uv.sources`

Currently, you can't specify editable dependencies (`-e`) or relative path dependencies in pyproject.toml, you'd still need to use requirements.txt for that. We want you to be able to configure everything in pyproject.toml. We do so by extending `project.dependencies` with ``tool.uv.sources`. Full documentation: [Specifying Dependencies](https://github.com/astral-sh/uv/blob/main/docs/specifying_dependencies.md)

```toml
[project]
name = "albatross"
version = "0.1.0"
dependencies = [
  "tqdm>=4,<5",
  "mollymawk ==0.1.0",
  "importlib_metadata >=7.1.0,<8; python_version < '3.10'",
]

[tool.uv.sources]
mollymawk = { path = "../mollymawk", editable = true }
importlib_metadata = { git = "<https://github.com/python/importlib_metadata>", tag = "7.2.0" }

```

`tool.uv.sources` applies to `project.dependencies`, `project.optional-dependencies` and `tool.uv.dev-dependencies`.

We will also support pinning a dependencies to a specific index here, but that is not implemented yet.

## Workspaces

A workspace contains multiple packages, each with different dependencies, while being installed and managed together. Full documentation: [Workspaces](https://github.com/astral-sh/uv/blob/main/docs/workspaces.md)

You define a workspace with `tool.uv.workspace`:

```toml
[tool.uv.workspace]
members = ["packages/*"]

```

You can depend on workspace members by name, they will be installed as editables:

```toml
[project]
name = "albatross"
version = "0.1.0"
dependencies = [
  "mollymawk ==0.1.0",
]

[tool.uv.sources]
mollymawk = { workspace = true }

```

## Tool management

We're also working on tool management, which is tracked in https://github.com/astral-sh/uv/issues/3560.

---

Feel free to edit this issues when preview features change.
