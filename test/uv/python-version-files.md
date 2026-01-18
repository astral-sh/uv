# Python Version Files

Tests for `.python-version` and `.python-versions` file discovery in `uv venv`.

```toml title="mdtest.toml"
[environment]
python-versions = ["3.11", "3.12"]
create-venv = false
```

## Reading .python-version file

<!-- Derived from [`venv::create_venv_reads_request_from_python_version_file`](https://github.com/astral-sh/uv/blob/c83066b8ee71432543ec3ff183bec4681beca2e7/crates/uv/tests/it/venv.rs#L307-L344) -->

`uv venv` reads the Python version from a `.python-version` file if present.

First create a venv with 3.12:

```console
$ uv venv --python 3.12
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
Creating virtual environment at: .venv
Activate with: source .venv/[BIN]/activate
```

With a `.python-version` file specifying 3.11, that version is preferred:

```text title=".python-version"
3.11
```

```console
$ uv venv --clear
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
Creating virtual environment at: .venv
Activate with: source .venv/[BIN]/activate
```

## Reading .python-versions file

<!-- Derived from [`venv::create_venv_reads_request_from_python_versions_file`](https://github.com/astral-sh/uv/blob/c83066b8ee71432543ec3ff183bec4681beca2e7/crates/uv/tests/it/venv.rs#L346-L383) -->

`uv venv` reads the Python version from a `.python-versions` file, using the first listed version.

First create a venv with 3.12:

```console
$ uv venv --python 3.12
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Using CPython 3.12.[X] interpreter at: [PYTHON-3.12]
Creating virtual environment at: .venv
Activate with: source .venv/[BIN]/activate
```

With a `.python-versions` file listing 3.11 first, that version is preferred:

```text title=".python-versions"
3.11
3.12
```

```console
$ uv venv --clear
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
Creating virtual environment at: .venv
Activate with: source .venv/[BIN]/activate
```

## Explicit --python overrides .python-version file

<!-- Derived from [`venv::create_venv_explicit_request_takes_priority_over_python_version_file`](https://github.com/astral-sh/uv/blob/c83066b8ee71432543ec3ff183bec4681beca2e7/crates/uv/tests/it/venv.rs#L810-L833) -->

An explicit `--python` flag takes priority over the `.python-version` file.

```text title=".python-version"
3.12
```

```console
$ uv venv --python 3.11
success: true
exit_code: 0
----- stdout -----

----- stderr -----
Using CPython 3.11.[X] interpreter at: [PYTHON-3.11]
Creating virtual environment at: .venv
Activate with: source .venv/[BIN]/activate
```
