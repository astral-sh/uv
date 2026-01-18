# Version

Tests for the `uv version` command.

```toml title="mdtest.toml"
[environment]
python-version = "3.12"
```

## Show project version

The `uv version` command shows the project version.

```toml title="pyproject.toml"
[project]
name = "test-project"
version = "1.2.3"
requires-python = ">=3.12"
dependencies = []
```

```
$ uv version
success: true
exit_code: 0
----- stdout -----
test-project 1.2.3

----- stderr -----
```

## Show uv version

The `uv --version` flag shows the uv version.

```toml title="pyproject.toml"
[project]
name = "test-project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = []
```

```
$ uv --version
success: true
exit_code: 0
----- stdout -----
uv [VERSION] ([COMMIT] DATE)

----- stderr -----
```
