# Changelog

<!-- prettier-ignore-start -->


## 0.7.0

### Breaking changes

- **Update `uv version` to display and update project versions ([#12349](https://github.com/astral-sh/uv/pull/12349))**

  Previously, `uv version` displayed uv's version. Now, `uv version` will display or update the project's version. This interface was [heavily requested](https://github.com/astral-sh/uv/issues/6298) and, after much consideration, we decided that transitioning the top-level command was the best option.

  Here's a brief example:

  ```console
  $ uv init example
  Initialized project `example` at `./example`
  $ cd example
  $ uv version
  example 0.1.0
  $ uv version --bump major
  example 0.1.0 => 1.0.0
  $ uv version --short
  1.0.0
  ```

  If used outside of a project, uv will fallback to showing its own version still:

  ```console
  $ uv version
  warning: failed to read project: No `pyproject.toml` found in current directory or any parent directory
    running `uv self version` for compatibility with old `uv version` command.
    this fallback will be removed soon, pass `--preview` to make this an error.

  uv 0.7.0 (4433f41c9 2025-04-29)
  ```

  As described in the warning, `--preview` can be used to error instead:

  ```console
  $ uv version --preview
  error: No `pyproject.toml` found in current directory or any parent directory
  ```

  The previous functionality of `uv version` was moved to `uv self version`.

- **Avoid fallback to subsequent indexes on authentication failure ([#12805](https://github.com/astral-sh/uv/pull/12805))**

  When using the `first-index` strategy (the default), uv will stop searching indexes for a package once it is found on a single index. Previously, uv considered a package as "missing" from an index during authentication failures, such as an HTTP 401 or HTTP 403 (normally, missing packages are represented by an HTTP 404). This behavior was motivated by unusual responses from some package indexes, but reduces the safety of uv's index strategy when authentication fails. Now, uv will consider an authentication failure as a stop-point when searching for a package across indexes. The `index.ignore-error-codes` option can be used to recover the existing behavior, e.g.:

  ```toml
  [[tool.uv.index]]
  name = "pytorch"
  url = "https://download.pytorch.org/whl/cpu"
  ignore-error-codes = [401, 403]
  ```

  Since `pytorch`'s indexes always return a HTTP 403 for missing packages, uv special-cases indexes on the `pytorch.org` domain to ignore that error code by default.

- **Require the command in `uvx <name>` to be available in the Python environment ([#11603](https://github.com/astral-sh/uv/pull/11603))**

  Previously, `uvx` would execute attempt to execute a command even if it was not provided by a Python package. For example, if we presume `foo` is an empty Python package which provides no command, `uvx foo` would invoke the `foo` command on the `PATH` (if present). Now, uv will error early if the `foo` executable is not provided by the requested Python package. This check is not enforced when `--from` is used, so patterns like `uvx --from foo bash -c "..."` are still valid. uv also still allows `uvx foo` where the `foo` executable is provided by a dependency of `foo` instead of `foo` itself, as this is fairly common for packages which depend on a dedicated package for their command-line interface.

- **Use index URL instead of package URL for keyring credential lookups ([#12651](https://github.com/astral-sh/uv/pull/12651))**

  When determining credentials for querying a package URL, uv previously sent the full URL to the `keyring` command. However, some keyring plugins only work if given the _index URL_ (which is usually a parent of the package URL). Now, uv requests credentials for the index URL instead. This behavior matches `pip`.

- **Remove `--version` from subcommands ([#13108](https://github.com/astral-sh/uv/pull/13108))**

  Previously, uv allowed the `--version` flag on arbitrary subcommands, e.g., `uv run --version`. However, the `--version` flag is useful for other operations since uv is a package manager. Consequently, we've removed the `--version` flag from subcommands — it is only available as `uv --version`.

- **Omit Python 3.7 downloads from managed versions ([#13022](https://github.com/astral-sh/uv/pull/13022))**

  Python 3.7 is EOL and not formally supported by uv, however, Python 3.7 was previously available for download on a subset of platforms. Now, Python 3.7 cannot be installed by uv.

- **Reject non-PEP 751 TOML files in install, compile, and export commands ([#13120](https://github.com/astral-sh/uv/pull/13120), [#13119](https://github.com/astral-sh/uv/pull/13119))**

  Previously, uv treated arbitrary `.toml` files passed to commands (e.g., `uv pip install -r foo.toml` or `uv pip compile -o foo.toml`) as `requirements.txt`-formatted files. Now, uv will error instead. If using PEP 751 lockfiles, use the standardized format for custom names instead, e.g., `pylock.foo.toml`.

- **Ignore arbitrary Python requests in version files ([#12909](https://github.com/astral-sh/uv/pull/12909))**

  uv allows arbitrary strings to be used for Python version requests, in which they are treated as an executable name to search for in the `PATH`. However, using this form of request in `.python-version` files is non-standard and conflicts with `pyenv-virtualenv` which writes environment names to `.python-version` files. In this release, uv will now ignore requests that are arbitrary strings when found in `.python-version` files.

- **Make `--frozen` and `--no-sources` conflicting options ([#12671](https://github.com/astral-sh/uv/pull/12671))**

  Using `--no-sources` always requires a new resolution and `--frozen` will always fail when used with it. Now, this conflict is encoded in the CLI options for clarity.

- **Treat empty `UV_PYTHON_INSTALL_DIR` and `UV_TOOL_DIR` as unset ([#12907](https://github.com/astral-sh/uv/pull/12907), [#12905](https://github.com/astral-sh/uv/pull/12905))**

  Previously, these variables were treated as set to the current working directory when set to an empty string. Now, uv will ignore these variables when empty. This matches uv's behavior for other environment variables which configure directories.

### Enhancements

- Disallow mixing requirements across PyTorch indexes ([#13179](https://github.com/astral-sh/uv/pull/13179))
- Optional managed Python archive download cache ([#12175](https://github.com/astral-sh/uv/pull/12175))
- Add `poetry-core` as a `uv init` build backend option ([#12781](https://github.com/astral-sh/uv/pull/12781))
- Show tag hints when failing to find a compatible wheel in `pylock.toml` ([#13136](https://github.com/astral-sh/uv/pull/13136))
- Report Python versions in `pyvenv.cfg` version mismatch ([#13027](https://github.com/astral-sh/uv/pull/13027))

### Bug fixes

- Avoid erroring on omitted wheel-only packages in `pylock.toml` ([#13132](https://github.com/astral-sh/uv/pull/13132))
- Fix display name for `uvx --version` ([#13109](https://github.com/astral-sh/uv/pull/13109))
- Implement RFC 7231 compliant relative URI and fragment handling in redirects ([#13050](https://github.com/astral-sh/uv/pull/13050))
- Respect build options (`--no-binary` et al) in `pylock.toml` ([#13134](https://github.com/astral-sh/uv/pull/13134))
- Use `upload-time` rather than `upload_time` in `uv.lock` ([#13176](https://github.com/astral-sh/uv/pull/13176))

### Documentation

- Changed `fish` completions append `>>` to overwrite `>` ([#13130](https://github.com/astral-sh/uv/pull/13130))
- Add `pylock.toml` mentions where relevant ([#13115](https://github.com/astral-sh/uv/pull/13115))

## 0.6.x

See [changelogs/0.6.x](./changelogs/0.6.x.md)

## 0.5.x

See [changelogs/0.5.x](./changelogs/0.5.x.md)

## 0.4.x

See [changelogs/0.4.x](./changelogs/0.4.x.md)

## 0.3.x

See [changelogs/0.3.x](./changelogs/0.3.x.md)

## 0.2.x

See [changelogs/0.2.x](./changelogs/0.2.x.md)

## 0.1.x

See [changelogs/0.1.x](./changelogs/0.1.x.md)

<!-- prettier-ignore-end -->


