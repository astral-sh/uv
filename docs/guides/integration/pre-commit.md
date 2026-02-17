---
title: Using uv with pre-commit
description:
  A guide to using uv with pre-commit to automatically update lock files, export requirements, and
  compile requirements files.
---

# Using uv in pre-commit

An official pre-commit hook is provided at
[`astral-sh/uv-pre-commit`](https://github.com/astral-sh/uv-pre-commit).

To use uv with pre-commit, add one of the following examples to the `repos` list in the
`.pre-commit-config.yaml`.

To make sure your `uv.lock` file is up to date even if your `pyproject.toml` file was changed:

```yaml title=".pre-commit-config.yaml"
repos:
  - repo: https://github.com/astral-sh/uv-pre-commit
    # uv version.
    rev: 0.10.4
    hooks:
      - id: uv-lock
```

To keep a `requirements.txt` file in sync with your `uv.lock` file:

```yaml title=".pre-commit-config.yaml"
repos:
  - repo: https://github.com/astral-sh/uv-pre-commit
    # uv version.
    rev: 0.10.4
    hooks:
      - id: uv-export
```

To compile requirements files:

```yaml title=".pre-commit-config.yaml"
repos:
  - repo: https://github.com/astral-sh/uv-pre-commit
    # uv version.
    rev: 0.10.4
    hooks:
      # Compile requirements
      - id: pip-compile
        args: [requirements.in, -o, requirements.txt]
```

To compile alternative requirements files, modify `args` and `files`:

```yaml title=".pre-commit-config.yaml"
repos:
  - repo: https://github.com/astral-sh/uv-pre-commit
    # uv version.
    rev: 0.10.4
    hooks:
      # Compile requirements
      - id: pip-compile
        args: [requirements-dev.in, -o, requirements-dev.txt]
        files: ^requirements-dev\.(in|txt)$
```

To run the hook over multiple files at the same time, add additional entries:

```yaml title=".pre-commit-config.yaml"
repos:
  - repo: https://github.com/astral-sh/uv-pre-commit
    # uv version.
    rev: 0.10.4
    hooks:
      # Compile requirements
      - id: pip-compile
        name: pip-compile requirements.in
        args: [requirements.in, -o, requirements.txt]
      - id: pip-compile
        name: pip-compile requirements-dev.in
        args: [requirements-dev.in, -o, requirements-dev.txt]
        files: ^requirements-dev\.(in|txt)$
```
