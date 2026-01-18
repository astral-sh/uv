# Init

Tests for the `uv init` command.

```toml title="mdtest.toml"
[environment]
python-version = "3.12"
```

## Basic init

Running `uv init` in an empty directory creates a new project.

```
$ uv init --name test-project --python 3.12
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Initialized project `test-project`
```

After init, the pyproject.toml should exist with expected content.

```toml title="pyproject.toml" snapshot=true
[project]
name = "test-project"
version = "0.1.0"
description = "Add your description here"
readme = "README.md"
requires-python = ">=3.12"
dependencies = []
```
