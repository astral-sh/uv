# MDTest Features

Tests to verify mdtest features are working correctly.

```toml title="mdtest.toml"
[environment]
python-version = "3.12"
```

## Basic command execution

Basic command execution should capture success/failure and output.

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

## Command failure handling

Commands that fail should report the correct exit code.

```toml title="pyproject.toml"
[project]
name = "will-fail"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["nonexistent-package-xyz-12345"]
```

```
$ uv lock
success: false
exit_code: 1
----- stdout -----

----- stderr -----
  × No solution found when resolving dependencies:
  ╰─▶ Because nonexistent-package-xyz-12345 was not found in the package registry and your project depends on nonexistent-package-xyz-12345, we can conclude that your project's requirements are unsatisfiable.
```

## Multiple commands in sequence

Multiple commands in the same section should run in sequence.

```toml title="pyproject.toml"
[project]
name = "multi-cmd"
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

```
$ uv sync
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Resolved 1 package in [TIME]
Audited in [TIME]
```

## File creation with nested paths

Files can be created in nested directories.

```toml title="pyproject.toml"
[project]
name = "nested-files"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = []

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

## File snapshot verification

File snapshots verify that output files have expected content.

```toml title="pyproject.toml"
[project]
name = "snapshot-test"
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

The pyproject.toml should be updated with the new dependency.

```toml title="pyproject.toml" snapshot=true
[project]
name = "snapshot-test"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = [
    "iniconfig>=2.0.0",
]
```

## Section independence

Each section is independent - files don't carry over from previous sections.

### First section

```toml title="pyproject.toml"
[project]
name = "section-a"
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

### Second section

This section has its own pyproject.toml - it doesn't inherit from previous section.

```toml title="pyproject.toml"
[project]
name = "section-b"
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

## Time filter application

The [TIME] filter is applied to timing output.

```toml title="pyproject.toml"
[project]
name = "time-filter"
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

## Path filter application

Temporary directory paths should be filtered.

```toml title="pyproject.toml"
[project]
name = "path-filter"
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

## Python version from section config

Python version can be overridden in a section-level mdtest.toml config.

```toml title="mdtest.toml"
[environment]
python-version = "3.11"
```

```toml title="pyproject.toml"
[project]
name = "py-version"
version = "0.1.0"
requires-python = ">=3.11"
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

## Filter configuration - counts

The `[filters]` section can enable additional output filters. With `counts = true`, package counts
are replaced with `[N]`.

```toml title="mdtest.toml"
[environment]
python-version = "3.12"

[filters]
counts = true
```

```toml title="pyproject.toml"
[project]
name = "filter-counts"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["requests"]
```

```
$ uv sync
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Resolved [N] packages in [TIME]
Prepared [N] packages in [TIME]
Installed [N] packages in [TIME]
 + certifi==2024.2.2
 + charset-normalizer==3.3.2
 + idna==3.6
 + requests==2.31.0
 + urllib3==2.2.1
```
