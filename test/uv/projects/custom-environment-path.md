# Custom Environment Path

Tests for `UV_PROJECT_ENVIRONMENT`, which allows customizing where the project virtual environment
is created.

```toml
# mdtest

[environment]
python-version = "3.12"
create-venv = false

[tree]
exclude = ["cache"]
```

## uv venv

### Outside a project

<!-- Derived from [`venv::create_venv_project_environment`](https://github.com/astral-sh/uv/blob/c83066b8ee71432543ec3ff183bec4681beca2e7/crates/uv/tests/it/venv.rs#L95-L211) -->

`uv venv` ignores `UV_PROJECT_ENVIRONMENT` when not in a project.

```toml
# mdtest

[environment]
env = { UV_PROJECT_ENVIRONMENT = "foo" }
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

The venv is created at `.venv`, not `foo`:

```tree depth=1
.
└── .venv/
```

### In a project

<!-- Derived from [`venv::create_venv_project_environment`](https://github.com/astral-sh/uv/blob/c83066b8ee71432543ec3ff183bec4681beca2e7/crates/uv/tests/it/venv.rs#L95-L211) -->

In a project, `UV_PROJECT_ENVIRONMENT` is respected.

```toml
# mdtest

[environment]
env = { UV_PROJECT_ENVIRONMENT = "foo" }
```

```toml
# file: pyproject.toml

[project]
name = "project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["iniconfig"]
```

```console
$ uv venv
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
Creating virtual environment at: foo
Activate with: source foo/[BIN]/activate
```

The venv is created at `foo`:

```tree depth=1
.
├── foo/
└── pyproject.toml
```

### Explicit path overrides environment variable

<!-- Derived from [`venv::create_venv_project_environment`](https://github.com/astral-sh/uv/blob/c83066b8ee71432543ec3ff183bec4681beca2e7/crates/uv/tests/it/venv.rs#L95-L211) -->

An explicit path overrides `UV_PROJECT_ENVIRONMENT`.

```toml
# mdtest

[environment]
env = { UV_PROJECT_ENVIRONMENT = "foo" }
```

```toml
# file: pyproject.toml

[project]
name = "project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["iniconfig"]
```

```console
$ uv venv bar
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
Creating virtual environment at: bar
Activate with: source bar/[BIN]/activate
```

The venv is created at `bar`, not `foo`:

```tree depth=1
.
├── bar/
└── pyproject.toml
```

### Using `--no-workspace` ignores environment variable

<!-- Derived from [`venv::create_venv_project_environment`](https://github.com/astral-sh/uv/blob/c83066b8ee71432543ec3ff183bec4681beca2e7/crates/uv/tests/it/venv.rs#L95-L211) -->

Using `--no-workspace` ignores `UV_PROJECT_ENVIRONMENT`.

```toml
# mdtest

[environment]
env = { UV_PROJECT_ENVIRONMENT = "foo" }
```

```toml
# file: pyproject.toml

[project]
name = "project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["iniconfig"]
```

```console
$ uv venv --no-workspace
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
Creating virtual environment at: .venv
Activate with: source .venv/[BIN]/activate
```

The venv is created at `.venv`, not `foo`:

```tree depth=1
.
├── .venv/
└── pyproject.toml
```
