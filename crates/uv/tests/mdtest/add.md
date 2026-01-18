# Add

Tests for the `uv add` command.

```toml title="mdtest.toml"
[environment]
python-version = "3.12"
```

## Basic add

Adding a dependency to a project.

```toml title="pyproject.toml"
[project]
name = "test-project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = []
```

```
$ uv add iniconfig
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Resolved 2 packages in [TIME]
Prepared 1 package in [TIME]
Installed 1 package in [TIME]
 + iniconfig==2.0.0
```

After adding, the pyproject.toml should be updated.

```toml title="pyproject.toml" snapshot=true
[project]
name = "test-project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = [
    "iniconfig>=2.0.0",
]
```

## Add with version constraint

Adding a dependency with a specific version constraint.

```toml title="pyproject.toml"
[project]
name = "test-project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = []
```

```
$ uv add iniconfig==2.0.0
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Resolved 2 packages in [TIME]
Prepared 1 package in [TIME]
Installed 1 package in [TIME]
 + iniconfig==2.0.0
```

## Add dev dependency

Adding a dev dependency.

```toml title="pyproject.toml"
[project]
name = "test-project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = []
```

```
$ uv add --dev iniconfig
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Resolved 2 packages in [TIME]
Prepared 1 package in [TIME]
Installed 1 package in [TIME]
 + iniconfig==2.0.0
```

The dev dependency is added to the dev group.

```toml title="pyproject.toml" snapshot=true
[project]
name = "test-project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = []

[dependency-groups]
dev = [
    "iniconfig>=2.0.0",
]
```
