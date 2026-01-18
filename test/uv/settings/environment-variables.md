# Environment Variables

Tests for environment variable parsing and validation.

```toml title="mdtest.toml"
[environment]
python-version = "3.12"
create-venv = false
```

## Invalid UV_HTTP_TIMEOUT

<!-- Derived from [`venv::create_venv_with_invalid_http_timeout`](https://github.com/astral-sh/uv/blob/c83066b8ee71432543ec3ff183bec4681beca2e7/crates/uv/tests/it/venv.rs#L885-L899) -->

An invalid `UV_HTTP_TIMEOUT` value produces an error.

```toml title="mdtest.toml"
[environment]
env = { UV_HTTP_TIMEOUT = "not_a_number" }
```

```console
$ uv venv .venv --python 3.12
success: false
exit_code: 2
----- stdout -----

----- stderr -----
error: Failed to parse environment variable `UV_HTTP_TIMEOUT` with invalid value `not_a_number`: invalid digit found in string
```

## Invalid UV_CONCURRENT_INSTALLS

<!-- Derived from [`venv::create_venv_with_invalid_concurrent_installs`](https://github.com/astral-sh/uv/blob/c83066b8ee71432543ec3ff183bec4681beca2e7/crates/uv/tests/it/venv.rs#L901-L915) -->

An invalid `UV_CONCURRENT_INSTALLS` value produces an error.

```toml title="mdtest.toml"
[environment]
env = { UV_CONCURRENT_INSTALLS = "0" }
```

```console
$ uv venv .venv --python 3.12
success: false
exit_code: 2
----- stdout -----

----- stderr -----
error: Failed to parse environment variable `UV_CONCURRENT_INSTALLS` with invalid value `0`: number would be zero for non-zero type
```
