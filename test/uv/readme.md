# MDTest Features

Tests to verify mdtest features are working correctly.

```toml title="mdtest.toml"
[environment]
python-versions = "3.12"
```

## Configuration Reference

### `[environment]` options

| Option                | Type            | Description                                                                              |
| --------------------- | --------------- | ---------------------------------------------------------------------------------------- |
| `python-versions`     | string or array | Python version(s) to use (e.g., `"3.12"` or `["3.11", "3.12"]`). Alias: `python-version` |
| `exclude-newer`       | string          | Exclude packages newer than this date                                                    |
| `http-timeout`        | string          | HTTP timeout for requests                                                                |
| `concurrent-installs` | string          | Number of concurrent installs                                                            |
| `target-os`           | string or array | Target OS(es) for this test (e.g., `"linux"`, `["macos", "linux"]`)                      |
| `target-family`       | string or array | Target OS family (e.g., `"unix"`, `"windows"`)                                           |
| `required-features`   | string or array | Required features to run this test (e.g., `"python-patch"`)                              |
| `env`                 | table           | Extra environment variables to set (e.g., `env = { FOO = "bar" }`)                       |
| `env-remove`          | array           | Environment variables to remove                                                          |
| `create-venv`         | bool            | Whether to create a virtual environment (default: true)                                  |

### `[filters]` options

All filter options are booleans (default: false):

| Option                   | Description                                     |
| ------------------------ | ----------------------------------------------- |
| `counts`                 | Replace package counts with `[N]`               |
| `exe-suffix`             | Remove `.exe` suffix on Windows                 |
| `python-names`           | Replace Python executable names with `[PYTHON]` |
| `virtualenv-bin`         | Replace virtualenv bin directory with `[BIN]`   |
| `python-install-bin`     | Filter Python installation bin directory        |
| `python-sources`         | Filter Python source messages                   |
| `pyvenv-cfg`             | Filter pyvenv.cfg file content                  |
| `link-mode-warning`      | Filter hardlink/copy mode warnings              |
| `not-executable`         | Filter "not executable" permission errors       |
| `python-keys`            | Filter Python platform keys                     |
| `latest-python-versions` | Replace latest Python versions with `[LATEST]`  |
| `compiled-file-count`    | Filter compiled file counts                     |
| `cyclonedx`              | Filter CycloneDX UUIDs                          |
| `collapse-whitespace`    | Collapse multiple spaces/tabs to single space   |
| `cache-size`             | Filter cache size output                        |
| `missing-file-error`     | Filter missing file errors (OS error 2/3)       |

### `[tree]` options

| Option            | Type  | Description                                                       |
| ----------------- | ----- | ----------------------------------------------------------------- |
| `exclude`         | array | Patterns to exclude from tree output (e.g., `["cache", "*.pyc"]`) |
| `default-filters` | bool  | Apply cross-platform normalization (default: true)                |

## Basic command execution

Basic command execution should capture success/failure and output.

```toml title="pyproject.toml"
[project]
name = "test-project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = []
```

```console
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

```console
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

```console
$ uv lock
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Resolved 1 package in [TIME]
```

```console
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

```console
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

```console
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

```console
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

```console
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

```console
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

```console
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

```console
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

```console
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

## Content assertion with assert=contains

The `assert=contains` attribute checks that a file contains specific content without requiring an
exact match. This is useful for checking specific lines in configuration files.

```toml title="mdtest.toml"
[environment]
create-venv = false
```

```toml title="pyproject.toml"
[project]
name = "assert-test"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = []
```

```console
$ uv venv
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
Creating virtual environment at: .venv
Activate with: source .venv/[BIN]/activate
```

The pyvenv.cfg file should contain the uv version:

```text title=".venv/pyvenv.cfg" assert=contains
uv =
```

## Tree snapshots

Tree snapshots verify the directory structure after commands run. Use the `tree` language identifier
with an optional `depth` parameter.

In tree output:

- Directories are shown with a trailing `/` (e.g., `packages/`)
- Symlinks are shown with `-> target` (e.g., `link -> target/path`)
- Regular files have no suffix

The `[tree]` configuration section allows excluding paths and toggling default filters:

- `exclude` - patterns to exclude from tree output (e.g., `cache`, `*.pyc`)
- `default-filters` - whether to apply cross-platform normalization (default: true)
  - Normalizes `bin`/`Scripts` to `[BIN]` inside virtual environments

```toml title="mdtest.toml"
[environment]
create-venv = false

[tree]
exclude = ["cache"]
```

```toml title="pyproject.toml"
[project]
name = "tree-test"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = []
```

```console
$ uv venv
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
Creating virtual environment at: .venv
Activate with: source .venv/[BIN]/activate
```

The directory should contain a .venv folder with the standard structure:

```tree depth=2
.
├── .venv/
│   ├── .gitignore
│   ├── CACHEDIR.TAG
│   ├── [BIN]/
│   ├── [LIB]/
│   └── pyvenv.cfg
└── pyproject.toml
```

## Tree creation

Tree creation allows you to pre-create directory structures (including symlinks) before running
commands. Use `create=true` on a tree block to create the structure instead of verifying it.

```toml title="mdtest.toml"
[environment]
create-venv = false

[tree]
exclude = ["cache"]
```

```tree create=true
.
├── packages/
│   ├── alpha/
│   └── beta/
└── src/
```

In tree creation blocks:

- Lines ending with `/` create directories
- Lines with `-> target` create symlinks
- Other lines create empty files

This is useful for setting up complex directory structures before adding file content:

```toml title="packages/alpha/pyproject.toml"
[project]
name = "alpha"
version = "0.1.0"
requires-python = ">=3.12"
```

```console
$ uv lock --directory packages/alpha
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
Resolved 1 package in [TIME]
```

Verify the resulting structure (note directories have `/` suffix):

```tree
.
├── packages/
│   ├── alpha/
│   │   ├── pyproject.toml
│   │   └── uv.lock
│   └── beta/
└── src/
```

## Document order execution

Steps (file creation, commands, snapshots) execute in document order. This allows testing scenarios
where commands depend on the state of files created before them.

In this example, we first run `uv venv` with only the mdtest.toml (no pyproject.toml), then add a
pyproject.toml and run `uv venv --clear` to verify the behavior changes.

```toml title="mdtest.toml"
[environment]
create-venv = false
```

First command runs without pyproject.toml:

```console
$ uv venv
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
Creating virtual environment at: .venv
Activate with: source .venv/[BIN]/activate
```

Now create pyproject.toml with requires-python:

```toml title="pyproject.toml"
[project]
name = "order-test"
version = "0.1.0"
requires-python = ">=3.11"
```

Second command sees the pyproject.toml and respects requires-python:

```console
$ uv venv --clear
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
Creating virtual environment at: .venv
Activate with: source .venv/[BIN]/activate
```

Snapshots also execute in document order. Here we verify the pyvenv.cfg exists:

```text title=".venv/pyvenv.cfg" assert=contains
home =
```
