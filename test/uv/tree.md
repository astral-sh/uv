# Tree

Tests for the `uv tree` command.

```toml title="mdtest.toml"
[environment]
python-version = "3.12"
```

## Basic tree

A simple dependency tree.

```toml title="pyproject.toml"
[project]
name = "test-project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["iniconfig"]
```

```
$ uv tree --universal
success: true
exit_code: 0
----- stdout -----
test-project v0.1.0
└── iniconfig v2.0.0

----- stderr -----
Resolved 2 packages in [TIME]
```

## Nested dependencies

A project with nested dependencies.

```toml title="pyproject.toml"
[project]
name = "project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["requests"]
```

```
$ uv tree --universal
success: true
exit_code: 0
----- stdout -----
project v0.1.0
└── requests v2.31.0
    ├── certifi v2024.2.2
    ├── charset-normalizer v3.3.2
    ├── idna v3.6
    └── urllib3 v2.2.1

----- stderr -----
Resolved 6 packages in [TIME]
```

## Inverted tree

The `--invert` flag shows reverse dependencies.

```toml title="pyproject.toml"
[project]
name = "project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["requests"]
```

```
$ uv tree --universal --invert
success: true
exit_code: 0
----- stdout -----
certifi v2024.2.2
└── requests v2.31.0
    └── project v0.1.0
charset-normalizer v3.3.2
└── requests v2.31.0 (*)
idna v3.6
└── requests v2.31.0 (*)
urllib3 v2.2.1
└── requests v2.31.0 (*)
(*) Package tree already displayed

----- stderr -----
Resolved 6 packages in [TIME]
```
