# Using uv in pre-commit

An official pre-commit hook is provided at
[`astral-sh/uv-pre-commit`](https://github.com/astral-sh/uv-pre-commit).

To make sure your `uv.lock` file is up to date even if your `pyproject.toml` file was changed via
pre-commit, add the following to the `.pre-commit-config.yaml`:

```yaml title=".pre-commit-config.yaml"
- repo: https://github.com/astral-sh/uv-pre-commit
  # uv version.
  rev: 0.5.8
  hooks:
    - id: uv-lock
```

To keep your `requirements.txt` file updated using pre-commit:

```yaml title=".pre-commit-config.yaml"
- repo: https://github.com/astral-sh/uv-pre-commit
  # uv version.
  rev: 0.5.8
  hooks:
    - id: uv-export
```

To compile requirements via pre-commit, add the following to the `.pre-commit-config.yaml`:

```yaml title=".pre-commit-config.yaml"
- repo: https://github.com/astral-sh/uv-pre-commit
  # uv version.
  rev: 0.5.13
  hooks:
    # Compile requirements
    - id: pip-compile
      args: [requirements.in, -o, requirements.txt]
```

To compile alternative files, modify `args` and `files`:

```yaml title=".pre-commit-config.yaml"
- repo: https://github.com/astral-sh/uv-pre-commit
  # uv version.
  rev: 0.5.13
  hooks:
    # Compile requirements
    - id: pip-compile
      args: [requirements-dev.in, -o, requirements-dev.txt]
      files: ^requirements-dev\.(in|txt)$
```

To run the hook over multiple files at the same time:

```yaml title=".pre-commit-config.yaml"
- repo: https://github.com/astral-sh/uv-pre-commit
  # uv version.
  rev: 0.5.13
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
