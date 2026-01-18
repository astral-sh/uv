# Sync

Tests for the `uv sync` command.

```toml title="mdtest.toml"
[environment]
python-version = "3.12"
```

## Basic sync

Syncing a simple project.

```toml title="pyproject.toml"
[project]
name = "test-project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["iniconfig"]
```

```
$ uv sync
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Resolved 2 packages in [TIME]
Prepared 1 package in [TIME]
Installed 1 package in [TIME]
 + iniconfig==2.0.0
```

## Sync with no dependencies

A project with no dependencies should sync cleanly.

```toml title="pyproject.toml"
[project]
name = "empty-project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = []
```

```
$ uv sync
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Resolved 1 package in [TIME]
Audited in [TIME]
```

## Sync with dev dependencies

Syncing a project with dev dependencies.

```toml title="pyproject.toml"
[project]
name = "dev-deps"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = []

[dependency-groups]
dev = ["iniconfig"]
```

```
$ uv sync
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Resolved 2 packages in [TIME]
Prepared 1 package in [TIME]
Installed 1 package in [TIME]
 + iniconfig==2.0.0
```

## Sync without dev dependencies

Syncing without dev dependencies using --no-dev.

```toml title="pyproject.toml"
[project]
name = "dev-deps"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = []

[dependency-groups]
dev = ["iniconfig"]
```

```
$ uv sync --no-dev
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Resolved 2 packages in [TIME]
Audited in [TIME]
```

## Sync locked

Using --locked ensures the lockfile is up to date.

```toml title="pyproject.toml"
[project]
name = "locked-test"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["iniconfig"]
```

First create a lock file.

```
$ uv lock
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Resolved 2 packages in [TIME]
```

Then sync with --locked.

```
$ uv sync --locked
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Resolved 2 packages in [TIME]
Prepared 1 package in [TIME]
Installed 1 package in [TIME]
 + iniconfig==2.0.0
```
