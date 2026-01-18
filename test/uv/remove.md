# Remove

Tests for the `uv remove` command.

```toml title="mdtest.toml"
[environment]
python-version = "3.12"
```

## Basic remove

Removing a dependency from a project.

```toml title="pyproject.toml"
[project]
name = "test-project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["iniconfig"]
```

First sync to install the dependency.

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

Then remove the dependency.

```
$ uv remove iniconfig
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Resolved 1 package in [TIME]
Uninstalled 1 package in [TIME]
 - iniconfig==2.0.0
```

The pyproject.toml should have the dependency removed.

```toml title="pyproject.toml" snapshot=true
[project]
name = "test-project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = []
```

## Remove dev dependency

Removing a dev dependency.

```toml title="pyproject.toml"
[project]
name = "dev-test"
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

```
$ uv remove --dev iniconfig
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Resolved 1 package in [TIME]
Uninstalled 1 package in [TIME]
 - iniconfig==2.0.0
```
