# Workspace

Tests for workspace functionality.

```toml title="mdtest.toml"
[environment]
python-version = "3.12"
```

## Single package

A workspace with a single package.

```toml title="pyproject.toml"
[project]
name = "workspace-root"
version = "0.1.0"
requires-python = ">=3.12"

[tool.uv.workspace]
members = ["packages/*"]
```

```toml title="packages/alpha/pyproject.toml"
[project]
name = "alpha"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = []
```

```
$ uv lock
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Resolved 2 packages in [TIME]
```

## Two packages

A workspace with two packages where one depends on the other.

```toml title="pyproject.toml"
[project]
name = "workspace-root"
version = "0.1.0"
requires-python = ">=3.12"

[tool.uv.workspace]
members = ["packages/*"]
```

```toml title="packages/alpha/pyproject.toml"
[project]
name = "alpha"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = []
```

```toml title="packages/beta/pyproject.toml"
[project]
name = "beta"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["alpha"]

[tool.uv.sources]
alpha = { workspace = true }
```

```
$ uv lock
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Resolved 3 packages in [TIME]
```
