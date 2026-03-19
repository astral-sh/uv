# Migrating from Poetry to uv

This guide covers converting a Poetry project to uv. Both tools manage dependencies through
`pyproject.toml`, so the transition is mostly about updating configuration and learning the new
commands.

## Command mapping

| Poetry | uv | Notes |
| --- | --- | --- |
| `poetry init` | `uv init` | |
| `poetry install` | `uv sync` | Installs from lockfile |
| `poetry add flask` | `uv add flask` | |
| `poetry add flask --group dev` | `uv add flask --dev` | Or `--group dev` |
| `poetry add flask --group docs` | `uv add flask --group docs` | |
| `poetry remove flask` | `uv remove flask` | |
| `poetry update` | `uv lock --upgrade` | Refreshes the lockfile |
| `poetry update flask` | `uv lock --upgrade-package flask` | |
| `poetry lock` | `uv lock` | |
| `poetry run pytest` | `uv run pytest` | |
| `poetry shell` | `source .venv/bin/activate` | uv doesn't have a shell command |
| `poetry build` | `uv build` | |
| `poetry publish` | `uv publish` | |
| `poetry show` | `uv pip show` or `uv tree` | `uv tree` for dependency tree |
| `poetry env use 3.12` | `uv python pin 3.12` | |

## Converting `pyproject.toml`

Poetry uses a `[tool.poetry]` section. uv uses standard `[project]` metadata (PEP 621).

### Before (Poetry)

```toml
[tool.poetry]
name = "myapp"
version = "0.1.0"
description = "My application"
authors = ["Dev <dev@example.com>"]

[tool.poetry.dependencies]
python = "^3.11"
flask = "^3.0"
sqlalchemy = {version = "^2.0", extras = ["asyncio"]}

[tool.poetry.group.dev.dependencies]
pytest = "^8.0"
ruff = "^0.4"

[tool.poetry.group.docs.dependencies]
sphinx = "^7.0"

[tool.poetry.scripts]
serve = "myapp.cli:main"

[build-system]
requires = ["poetry-core"]
build-backend = "poetry.core.masonry.api"
```

### After (uv)

```toml
[project]
name = "myapp"
version = "0.1.0"
description = "My application"
authors = [{name = "Dev", email = "dev@example.com"}]
requires-python = ">=3.11"
dependencies = [
    "flask>=3.0,<4",
    "sqlalchemy[asyncio]>=2.0,<3",
]

[project.scripts]
serve = "myapp.cli:main"

[dependency-groups]
dev = ["pytest>=8.0", "ruff>=0.4"]
docs = ["sphinx>=7.0"]

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"
```

### What changed

- `[tool.poetry.dependencies]` moves to `[project.dependencies]` using
  [PEP 508](https://peps.python.org/pep-0508/) syntax.
- Poetry's `^3.0` (caret) becomes `>=3.0,<4`. Poetry's `~3.0` (tilde) becomes `>=3.0,<3.1`.
- `python = "^3.11"` becomes `requires-python = ">=3.11"`.
- Dev and optional groups move to `[dependency-groups]`.
- The build backend changes — `hatchling` or `setuptools` are common choices.
- `[tool.poetry.scripts]` becomes `[project.scripts]`.

## Step-by-step migration

1. **Convert the `pyproject.toml`** as shown above. You can do this manually or start fresh:

    ```console
    $ uv init
    $ uv add flask "sqlalchemy[asyncio]"
    $ uv add --dev pytest ruff
    $ uv add --group docs sphinx
    ```

2. **Generate the lockfile:**

    ```console
    $ uv lock
    ```

3. **Install everything:**

    ```console
    $ uv sync --all-groups
    ```

4. **Remove Poetry artifacts:**

    ```console
    $ rm poetry.lock
    ```

5. **Verify:**

    ```console
    $ uv run pytest
    ```

## Handling private indexes

Poetry:

```toml
[[tool.poetry.source]]
name = "private"
url = "https://pypi.example.com/simple/"
```

uv:

```toml
[[tool.uv.index]]
name = "private"
url = "https://pypi.example.com/simple/"
```

See [index configuration](../../concepts/indexes.md) for authentication and priority options.

## What you gain

- **Speed.** uv resolves and installs significantly faster than Poetry.
- **Universal lockfile.** `uv.lock` works across platforms out of the box — Poetry's `poetry.lock`
  is also cross-platform but uses a custom format.
- **No plugin system needed.** Python version management, tool running, and builds are all built in.
- **Standards-based.** PEP 621 metadata works with any build backend, not just Poetry's.
