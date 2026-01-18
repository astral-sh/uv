# Writing CLI Tests in Markdown

This crate provides a framework for writing uv CLI tests in Markdown format, inspired by
[ty's mdtest framework](https://github.com/astral-sh/ruff/tree/main/crates/ty_test).

## Core Concept

Any Markdown file can serve as a test suite. Tests consist of embedded code blocks containing files
to create (with `title="filename"`) and commands to execute (starting with `$ `).

## Basic Structure

The simplest test creates a project file and runs a command:

````markdown
## Basic locking

```toml title="pyproject.toml"
[project]
name = "test"
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
````

The framework writes the `pyproject.toml` to a temporary directory, runs `uv lock`, and compares the
output against the expected result.

## Command Output Format

Command blocks start with `$ ` followed by the command. The expected output follows the
`uv_snapshot` format:

```
$ uv lock
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Resolved 2 packages in [TIME]
```

The `[TIME]` placeholder matches timing output like `1.23s` or `0.5ms`.

## File Snapshots

Verify file contents after commands run using `snapshot=true`:

````markdown
```
$ uv add requests
success: true
...
```

```toml title="pyproject.toml" snapshot=true
[project]
name = "test"
version = "0.1.0"
dependencies = [
    "requests>=2.31.0",
]
```
````

## Test Organization

Markdown headers organize tests into a hierarchy. Each leaf section (containing code blocks) becomes
an independent test with its own temporary directory:

```markdown
# Lock

## Basic locking

<!-- This is a test -->

## With dependencies

<!-- This is another test -->

## Edge cases

### Empty project

<!-- This is a test -->

### Invalid config

<!-- This is a test -->
```

The test names are derived from the header hierarchy (e.g., "Lock - Edge cases - Empty project").

## Configuration

TOML blocks with `title="mdtest.toml"` configure test behavior:

````markdown
```toml title="mdtest.toml"
[environment]
python-version = "3.12"
exclude-newer = "2024-03-25T00:00:00Z"

[filters]
counts = true
```
````

Configuration is inherited by nested sections and can be overridden.

### Environment Options

| Option                | Description                                       |
| --------------------- | ------------------------------------------------- |
| `python-version`      | Python version to use (e.g., "3.12")              |
| `exclude-newer`       | Exclude packages newer than date                  |
| `http-timeout`        | HTTP timeout for requests                         |
| `concurrent-installs` | Number of concurrent installs                     |
| `target-os`           | Target OS(es) - matches Rust's `target_os` values |
| `target-family`       | Target family - matches Rust's `target_family`    |
| `env`                 | Extra environment variables                       |

### Platform-Specific Tests

Tests can be restricted to specific platforms using `target-os` or `target-family`:

````markdown
```toml title="mdtest.toml"
[environment]
# Run only on Unix-like systems (Linux, macOS, BSD, etc.)
target-family = "unix"
```
````

````markdown
```toml title="mdtest.toml"
[environment]
# Run only on specific operating systems
target-os = ["linux", "macos"]
```
````

The values match Rust's cfg attributes exactly:

- `target-family`: `"unix"`, `"windows"`, `"wasm"`
- `target-os`: `"linux"`, `"macos"`, `"windows"`, `"freebsd"`, `"netbsd"`, `"openbsd"`, etc.

### Filter Options

Filters normalize output for reproducible tests:

| Filter                   | Description                                     |
| ------------------------ | ----------------------------------------------- |
| `counts`                 | Replace package counts with `[N]`               |
| `exe-suffix`             | Remove `.exe` suffix on Windows                 |
| `python-names`           | Replace Python executable names with `[PYTHON]` |
| `virtualenv-bin`         | Replace `Scripts`/`bin` with `[BIN]`            |
| `latest-python-versions` | Replace latest Python versions with `[LATEST]`  |
| `collapse-whitespace`    | Collapse multiple spaces/tabs to single space   |

## Running Tests

Tests live in `test/uv/` at the workspace root and are discovered at runtime. Run with:

```bash
cargo test -p uv mdtest
```

Filter to specific tests:

```bash
cargo test -p uv mdtest -- "Lock - Basic"
```

Update snapshots when output changes:

```bash
UV_UPDATE_MDTEST_SNAPSHOTS=1 cargo test -p uv mdtest
```
