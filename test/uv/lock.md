# Lock

Tests for the `uv lock` command.

```toml title="mdtest.toml"
[environment]
python-version = "3.12"
```

## Basic locking

A simple project with no dependencies should lock successfully.

```toml title="pyproject.toml"
[project]
name = "test-project"
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
Resolved 1 package in [TIME]
```

## With a single dependency

A project with a single dependency should resolve correctly.

```toml title="pyproject.toml"
[project]
name = "test-project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["iniconfig"]
```

```
$ uv lock
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Resolved 2 packages in [TIME]
```
