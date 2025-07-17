# Changelog

<!-- prettier-ignore-start -->


## 0.8.0

Since we released uv [0.7.0](https://github.com/astral-sh/uv/releases/tag/0.5.0) in April, we've accumulated various changes that improve correctness and user experience, but could break some workflows. This release contains those changes; many have been marked as breaking out of an abundance of caution. We expect most users to be able to upgrade without making changes.

This release also includes the stabilization of a couple `uv python install` features, which have been available under preview since late last year.

### Breaking changes

- **Install Python executables into a directory on the `PATH` ([#14626](https://github.com/astral-sh/uv/pull/14626))**

  `uv python install` now installs a versioned Python executable (e.g., `python3.13`) into a directory on the `PATH` (e.g., `~/.local/bin`) by default. This behavior has been available under the `--preview` flag since [Oct 2024](https://github.com/astral-sh/uv/pull/8458). This change should not be breaking unless it shadows a Python executable elsewhere on the `PATH`.

  To install unversioned executables, i.e., `python3` and `python`, use the `--default` flag. The `--default` flag has also been in preview, but is not stabilized in this release.

  Note that these executables point to the base Python installation and only include the standard library. That means they will not include dependencies from your current project (use `uv run python` instead) and you cannot install packages into their environment (use `uvx --with <package> python` instead).

  As with tool installation, the target directory respects common variables like `XDG_BIN_HOME` and can be overridden with a `UV_PYTHON_BIN_DIR` variable.

  You can opt out of this behavior with `uv python install --no-bin` or `UV_PYTHON_INSTALL_BIN=0`.

  See the [documentation on installing Python executables](https://docs.astral.sh/uv/concepts/python-versions/#installing-python-executables) for more details.

- **Register Python versions with the Windows registry ([#14625](https://github.com/astral-sh/uv/pull/14625))**

  `uv python install` now registers the installed Python version with the Windows Registry as specified by [PEP 514](https://peps.python.org/pep-0514/). This allows using uv installed Python versions via the `py` launcher. This behavior has been available under the `--preview` flag since [Jan 2025](https://github.com/astral-sh/uv/pull/10634). This change should not be breaking, as using the uv Python versions with `py` requires explicit opt in.

  You can opt out of this behavior with `uv python install --no-registry` or `UV_PYTHON_INSTALL_REGISTRY=0`.

- **Prompt before removing an existing directory in `uv venv` ([#14309](https://github.com/astral-sh/uv/pull/14309))**

  Previously, `uv venv` would remove an existing virtual environment without confirmation. While this is consistent with the behavior of project commands (e.g., `uv sync`), it's surprising to users that are using imperative workflows (i.e., `uv pip`). Now, `uv venv` will prompt for confirmation before removing an existing virtual environment. **If not in an interactive context, uv will still remove the virtual environment for backwards compatibility. However, this behavior is likely to change in a future release.**

  The behavior for other commands (e.g., `uv sync`) is unchanged.

  You can opt out of this behavior by setting `UV_VENV_CLEAR=1` or passing the `--clear` flag.

- **Validate that discovered interpreters meet the Python preference ([#7934](https://github.com/astral-sh/uv/pull/7934))**

  uv allows opting out of its managed Python versions with the `--no-managed-python` and `python-preference` options.

  Previously, uv would not enforce this option for Python interpreters discovered on the `PATH`. For example, if a symlink to a managed Python interpreter was created, uv would allow it to be used even if `--no-managed-python` was provided. Now, uv ignores Python interpreters that do not match the Python preference _unless_ they are in an active virtual environment or are explicitly requested, e.g., with `--python /path/to/python3.13`.

  Similarly, uv would previously not invalidate existing project environments if they did not match the Python preference. Now, uv will invalidate and recreate project environments when the Python preference changes.

  You can opt out of this behavior by providing the explicit path to the Python interpreter providing `--managed-python` / `--no-managed-python` matching the interpreter you want.

- **Install dependencies without build systems when they are `path` sources ([#14413](https://github.com/astral-sh/uv/pull/14413))**

  When working on a project, uv uses the [presence of a build system](https://docs.astral.sh/uv/concepts/projects/config/#build-systems) to determine if it should be built and installed into the environment. However, when a project is a dependency of another project, it can be surprising for the dependency to be missing from the environment.

  Previously, uv would not build and install dependencies with [`path` sources](https://docs.astral.sh/uv/concepts/projects/dependencies/#path) unless they declared a build system or set `tool.uv.package = true`. Now, dependencies with `path` sources are built and installed regardless of the presence of a build system. If a build system is not present, the `setuptools.build_meta:__legacy__ ` backend will be used (per [PEP 517](https://peps.python.org/pep-0517/#source-trees)).

  You can opt out of this behavior by setting `package = false` in the source declaration, e.g.:

  ```toml
  [tool.uv.sources]
  foo = { path = "./foo", package = false }
  ```

  Or, by setting `tool.uv.package = false` in the dependent `pyproject.toml`.

  See the documentation on [virtual dependencies](https://docs.astral.sh/uv/concepts/projects/dependencies/#virtual-dependencies) for details.

- **Install dependencies without build systems when they are workspace members ([#14663](https://github.com/astral-sh/uv/pull/14663))**

  As described above for dependencies with `path` sources, uv previously would not build and install workspace members that did not declare a build system. Now, uv will build and install workspace members that are a dependency of _another_ workspace member regardless of the presence of a build system. The behavior is unchanged for workspace members that are not included in the `project.dependencies`, `project.optional-dependencies`, or `dependency-groups` tables of another workspace member.

  You can opt out of this behavior by setting `tool.uv.package = false` in the workspace member's `pyproject.toml`.

  See the documentation on [virtual dependencies](https://docs.astral.sh/uv/concepts/projects/dependencies/#virtual-dependencies) for details.

- **Bump `--python-platform linux` to `manylinux_2_28` ([#14300](https://github.com/astral-sh/uv/pull/14300))**

  uv allows performing [platform-specific resolution](https://docs.astral.sh/uv/concepts/resolution/#platform-specific-resolution) for explicit targets and provides short aliases, e.g., `linux`, for common targets.

  Previously, the default target for `--python-platform linux` was `manylinux_2_17`, which is compatible with most Linux distributions from 2014 or newer. We now default to `manylinux_2_28`, which is compatible with most Linux distributions from 2017 or newer.  This change follows the lead of other tools, such as `cibuildwheel`, which changed their default to `manylinux_2_28` in [Mar 2025](https://github.com/pypa/cibuildwheel/pull/2330).

  This change only affects users requesting a specific target platform. Otherwise, uv detects the `manylinux` target from your local glibc version.

  You can opt out of this behavior by using `--python-platform x86_64-manylinux_2_17` instead.

- **Remove `uv version` fallback ([#14161](https://github.com/astral-sh/uv/pull/14161))**

  In [Apr 2025](https://github.com/astral-sh/uv/pull/12349), uv changed the `uv version` command to an interface for viewing and updating the version of the current project. However, when outside a project, `uv version` would continue to display uv's version for backwards compatibility. Now, when used outside of a project, `uv version` will fail.

  You cannot opt out of this behavior. Use `uv self version` instead.

- **Require `--global` for removal of the global Python pin ([#14169](https://github.com/astral-sh/uv/pull/14169))**

  Previously, `uv python pin --rm` would allow you to remove the global Python pin without opt in. Now, uv requires the `--global` flag to remove the global Python pin.

  You cannot opt out of this behavior. Use the `--global` flag instead.

- **Support conflicting editable settings across groups ([#14197](https://github.com/astral-sh/uv/pull/14197))**

  Previously, uv would always treat a package as editable if any requirement requested it as editable. However, this prevented users from declaring `path` sources that toggled the `editable` setting across dependency groups. Now, uv allows declaring different `editable` values for conflicting groups. However, if a project includes a path dependency twice, once with `editable = true` and once without any editable annotation, those are now considered conflicting, and uv will exit with an error.

  You cannot opt out of this behavior. Use consistent `editable` settings or [mark groups as conflicting](https://docs.astral.sh/uv/concepts/projects/config/#conflicting-dependencies).

- **Make `uv_build` the default build backend in `uv init` ([#14661](https://github.com/astral-sh/uv/pull/14661))**

  The uv build backend (`uv_build`) was [stabilized in uv 0.7.19](https://github.com/astral-sh/uv/releases/tag/0.7.19). Now, it is the default build backend for `uv init --package` and `uv init --lib`. Previously, `hatchling` was the default build backend. A build backend is still not used without opt-in in `uv init`, but we expect to change this in a future release.

  You can opt out of this behavior with `uv init --build-backend hatchling`.

- **Set default `UV_TOOL_BIN_DIR` on Docker images ([#13391](https://github.com/astral-sh/uv/pull/13391))**

  Previously, `UV_TOOL_BIN_DIR` was not set in Docker images which meant that `uv tool install` did not install tools into a directory on the `PATH` without additional configuration. Now, `UV_TOOL_BIN_DIR` is set to `/usr/local/bin` in all Docker derived images.

  When the default image user is overridden (e.g. `USER <UID>`) with a less privileged user, this may cause `uv tool install` to fail.

  You can opt out of this behavior by setting an alternative `UV_TOOL_BIN_DIR`.

- **Update `--check` to return an exit code of 1 ([#14167](https://github.com/astral-sh/uv/pull/14167))**

  uv uses an exit code of 1 to indicate a "successful failure" and an exit code of 2 to indicate an "error".

  Previously, `uv lock --check` and `uv sync --check` would exit with a code of 2 when the lockfile or environment were outdated. Now, uv will exit with a code of 1.

  You cannot opt out of this behavior.

- **Use an ephemeral environment for `uv run --with` invocations ([#14447](https://github.com/astral-sh/uv/pull/14447))**

  When using `uv run --with`, uv layers the requirements requested using `--with` into another virtual environment and caches it. Previously, uv would invoke the Python interpreter in this layered environment. However, this allows poisoning the cached environment and introduces race conditions for concurrent invocations. Now, uv will layer _another_ empty virtual environment on top of the cached environment and invoke the Python interpreter there. This should only cause breakage in cases where the environment is being inspected at runtime.

  You cannot opt out of this behavior.

- **Restructure the `uv venv` command output and exit codes ([#14546](https://github.com/astral-sh/uv/pull/14546))**

  Previously, uv used `miette` to format the `uv venv` output. However, this was inconsistent with most of the uv CLI. Now, the output is a little different and the exit code has switched from 1 to 2 for some error cases.

  You cannot opt out of this behavior.

- **Default to `--workspace` when adding subdirectories ([#14529](https://github.com/astral-sh/uv/pull/14529))**

  When using `uv add` to add a subdirectory in a workspace, uv now defaults to adding the target as a workspace member.

  You can opt out of this behavior by providing `--no-workspace`.

- **Add missing validations for disallowed `uv.toml` fields ([#14322](https://github.com/astral-sh/uv/pull/14322))**

  uv does not allow some settings in the `uv.toml`. Previously, some settings were silently ignored when present in the `uv.toml`. Now, uv will error.

  You cannot opt out of this behavior. Use `--no-config` or remove the invalid settings.

### Configuration

- Add support for toggling Python bin and registry install options via env vars ([#14662](https://github.com/astral-sh/uv/pull/14662))

## 0.7.22

### Python

- Upgrade GraalPy to 24.2.2

See the [GraalPy release notes](https://github.com/oracle/graalpython/releases/tag/graal-24.2.2) for more details.

### Configuration

- Add `UV_COMPILE_BYTECODE_TIMEOUT` environment variable ([#14369](https://github.com/astral-sh/uv/pull/14369))
- Allow users to override index `cache-control` headers ([#14620](https://github.com/astral-sh/uv/pull/14620))
- Add `UV_LIBC` to override libc selection in multi-libc environment ([#14646](https://github.com/astral-sh/uv/pull/14646))

### Bug fixes

- Fix `--all-arches` when paired with `--only-downloads` ([#14629](https://github.com/astral-sh/uv/pull/14629))
- Skip Windows Python interpreters that return a broken MSIX package code ([#14636](https://github.com/astral-sh/uv/pull/14636))
- Warn on invalid `uv.toml` when provided via direct path ([#14653](https://github.com/astral-sh/uv/pull/14653))
- Improve async signal safety in Windows exception handler ([#14619](https://github.com/astral-sh/uv/pull/14619))

### Documentation

- Mention the `revision` in the lockfile versioning doc ([#14634](https://github.com/astral-sh/uv/pull/14634))
- Move "Conflicting dependencies" to the "Resolution" page ([#14633](https://github.com/astral-sh/uv/pull/14633))
- Rename "Dependency specifiers" section to exclude PEP 508 reference ([#14631](https://github.com/astral-sh/uv/pull/14631))
- Suggest `uv cache clean` prior to `--reinstall` ([#14659](https://github.com/astral-sh/uv/pull/14659))

### Preview features

- Make preview Python registration on Windows non-fatal ([#14614](https://github.com/astral-sh/uv/pull/14614))
- Update preview installation of Python executables to be non-fatal ([#14612](https://github.com/astral-sh/uv/pull/14612))
- Add `uv python update-shell` ([#14627](https://github.com/astral-sh/uv/pull/14627))

## 0.7.21

### Python

- Restore the SQLite `fts4`, `fts5`, `rtree`, and `geopoly` extensions on macOS and Linux

See the
[`python-build-standalone` release notes](https://github.com/astral-sh/python-build-standalone/releases/tag/20250712)
for more details.

### Enhancements

- Add `--python-platform` to `uv sync` ([#14320](https://github.com/astral-sh/uv/pull/14320))
- Support pre-releases in `uv version --bump` ([#13578](https://github.com/astral-sh/uv/pull/13578))
- Add `-w` shorthand for `--with` ([#14530](https://github.com/astral-sh/uv/pull/14530))
- Add an exception handler on Windows to display information on crash ([#14582](https://github.com/astral-sh/uv/pull/14582))
- Add hint when Python downloads are disabled ([#14522](https://github.com/astral-sh/uv/pull/14522))
- Add `UV_HTTP_RETRIES` to customize retry counts ([#14544](https://github.com/astral-sh/uv/pull/14544))
- Follow leaf symlinks matched by globs in `cache-key` ([#13438](https://github.com/astral-sh/uv/pull/13438))
- Support parent path components (`..`) in globs in `cache-key` ([#13469](https://github.com/astral-sh/uv/pull/13469))
- Improve `cache-key` performance ([#13469](https://github.com/astral-sh/uv/pull/13469))

### Preview features

- Add `uv sync --output-format json` ([#13689](https://github.com/astral-sh/uv/pull/13689))

### Bug fixes

- Do not re-resolve with a new Python version in `uv tool` if it is incompatible with `--python` ([#14606](https://github.com/astral-sh/uv/pull/14606))

### Documentation

- Document how to nest dependency groups with `include-group` ([#14539](https://github.com/astral-sh/uv/pull/14539))
- Fix repeated word in Pyodide doc ([#14554](https://github.com/astral-sh/uv/pull/14554))
- Update CONTRIBUTING.md with instructions to format Markdown files via Docker ([#14246](https://github.com/astral-sh/uv/pull/14246))
- Fix version number for `setup-python` ([#14533](https://github.com/astral-sh/uv/pull/14533))

## 0.7.20

### Python

- Add Python 3.14.0b4
- Add zstd support to Python 3.14 on Unix (it already was available on Windows)
- Add PyPy 7.3.20 (for Python 3.11.13)

See the [PyPy](https://pypy.org/posts/2025/07/pypy-v7320-release.html) and [`python-build-standalone`](https://github.com/astral-sh/python-build-standalone/releases/tag/20250708) release notes for more details.

### Enhancements

- Add `--workspace` flag to `uv add` ([#14496](https://github.com/astral-sh/uv/pull/14496))
- Add auto-detection for Intel GPUs ([#14386](https://github.com/astral-sh/uv/pull/14386))
- Drop trailing arguments when writing shebangs ([#14519](https://github.com/astral-sh/uv/pull/14519))
- Add debug message when skipping Python downloads ([#14509](https://github.com/astral-sh/uv/pull/14509))
- Add support for declaring multiple modules in namespace packages ([#14460](https://github.com/astral-sh/uv/pull/14460))

### Bug fixes

- Revert normalization of trailing slashes on index URLs ([#14511](https://github.com/astral-sh/uv/pull/14511))
- Fix forced resolution with all extras in `uv version` ([#14434](https://github.com/astral-sh/uv/pull/14434))
- Fix handling of pre-releases in preferences ([#14498](https://github.com/astral-sh/uv/pull/14498))
- Remove transparent variants in `uv-extract` to enable retries ([#14450](https://github.com/astral-sh/uv/pull/14450))

### Rust API

- Add method to get packages involved in a `NoSolutionError` ([#14457](https://github.com/astral-sh/uv/pull/14457))
- Make `ErrorTree` for `NoSolutionError` public ([#14444](https://github.com/astral-sh/uv/pull/14444))

### Documentation

- Finish incomplete sentence in pip migration guide ([#14432](https://github.com/astral-sh/uv/pull/14432))
- Remove `cache-dependency-glob` examples for `setup-uv` ([#14493](https://github.com/astral-sh/uv/pull/14493))
- Remove `uv pip sync` suggestion with `pyproject.toml` ([#14510](https://github.com/astral-sh/uv/pull/14510))
- Update documentation for GitHub to use `setup-uv@v6` ([#14490](https://github.com/astral-sh/uv/pull/14490))

## 0.7.19

The **[uv build backend](https://docs.astral.sh/uv/concepts/build-backend/) is now stable**, and considered ready for production use.

The uv build backend is a great choice for pure Python projects. It has reasonable defaults, with the goal of requiring zero configuration for most users, but provides flexible configuration to accommodate most Python project structures. It integrates tightly with uv, to improve messaging and user experience. It validates project metadata and structures, preventing common mistakes. And, finally, it's very fast — `uv sync` on a new project (from `uv init`) is 10-30x faster than with other build backends.

To use uv as a build backend in an existing project, add `uv_build` to the `[build-system]` section in your `pyproject.toml`:

```toml
[build-system]
requires = ["uv_build>=0.7.19,<0.8.0"]
build-backend = "uv_build"
```

In a future release, it will replace `hatchling` as the default in `uv init`. As before, uv will remain compatible with all standards-compliant build backends.

### Python

- Add PGO distributions of Python for aarch64 Linux, which are more optimized for better performance

See the [python-build-standalone release](https://github.com/astral-sh/python-build-standalone/releases/tag/20250702) for more details.

### Enhancements

- Ignore Python patch version for `--universal` pip compile ([#14405](https://github.com/astral-sh/uv/pull/14405))
- Update the tilde version specifier warning to include more context ([#14335](https://github.com/astral-sh/uv/pull/14335))
- Clarify behavior and hint on tool install when no executables are available ([#14423](https://github.com/astral-sh/uv/pull/14423))

### Bug fixes

- Make project and interpreter lock acquisition non-fatal ([#14404](https://github.com/astral-sh/uv/pull/14404))
- Includes `sys.prefix` in cached environment keys to avoid `--with` collisions across projects ([#14403](https://github.com/astral-sh/uv/pull/14403))

### Documentation

- Add a migration guide from pip to uv projects ([#12382](https://github.com/astral-sh/uv/pull/12382))

## 0.7.18

### Python

- Added arm64 Windows Python 3.11, 3.12, 3.13, and 3.14

  These are not downloaded by default, since x86-64 Python has broader ecosystem support on Windows.
However, they can be requested with `cpython-<version>-windows-aarch64`.

See the [python-build-standalone release](https://github.com/astral-sh/python-build-standalone/releases/tag/20250630) for more details.

### Enhancements

- Keep track of retries in `ManagedPythonDownload::fetch_with_retry` ([#14378](https://github.com/astral-sh/uv/pull/14378))
- Reuse build (virtual) environments across resolution and installation ([#14338](https://github.com/astral-sh/uv/pull/14338))
- Improve trace message for cached Python interpreter query ([#14328](https://github.com/astral-sh/uv/pull/14328))
- Use parsed URLs for conflicting URL error message ([#14380](https://github.com/astral-sh/uv/pull/14380))

### Preview features

- Ignore invalid build backend settings when not building ([#14372](https://github.com/astral-sh/uv/pull/14372))

### Bug fixes

- Fix equals-star and tilde-equals with `python_version` and `python_full_version` ([#14271](https://github.com/astral-sh/uv/pull/14271))
- Include the canonical path in the interpreter query cache key ([#14331](https://github.com/astral-sh/uv/pull/14331))
- Only drop build directories on program exit ([#14304](https://github.com/astral-sh/uv/pull/14304))
- Error instead of panic on conflict between global and subcommand flags ([#14368](https://github.com/astral-sh/uv/pull/14368))
- Consistently normalize trailing slashes on URLs with no path segments ([#14349](https://github.com/astral-sh/uv/pull/14349))

### Documentation

- Add instructions for publishing to JFrog's Artifactory ([#14253](https://github.com/astral-sh/uv/pull/14253))
- Edits to the build backend documentation ([#14376](https://github.com/astral-sh/uv/pull/14376))

## 0.7.17

### Bug fixes

- Apply build constraints when resolving `--with` dependencies ([#14340](https://github.com/astral-sh/uv/pull/14340))
- Drop trailing slashes when converting index URL from URL ([#14346](https://github.com/astral-sh/uv/pull/14346))
- Ignore `UV_PYTHON_CACHE_DIR` when empty ([#14336](https://github.com/astral-sh/uv/pull/14336))
- Fix error message ordering for `pyvenv.cfg` version conflict ([#14329](https://github.com/astral-sh/uv/pull/14329))

## 0.7.16

### Python

- Add Python 3.14.0b3

See the
[`python-build-standalone` release notes](https://github.com/astral-sh/python-build-standalone/releases/tag/20250626)
for more details.

### Enhancements

- Include path or URL when failing to convert in lockfile ([#14292](https://github.com/astral-sh/uv/pull/14292))
- Warn when `~=` is used as a Python version specifier without a patch version ([#14008](https://github.com/astral-sh/uv/pull/14008))

### Preview features

- Ensure preview default Python installs are upgradeable ([#14261](https://github.com/astral-sh/uv/pull/14261))

### Performance

- Share workspace cache between lock and sync operations ([#14321](https://github.com/astral-sh/uv/pull/14321))

### Bug fixes

- Allow local indexes to reference remote files ([#14294](https://github.com/astral-sh/uv/pull/14294))
- Avoid rendering desugared prefix matches in error messages ([#14195](https://github.com/astral-sh/uv/pull/14195))
- Avoid using path URL for workspace Git dependencies in `requirements.txt` ([#14288](https://github.com/astral-sh/uv/pull/14288))
- Normalize index URLs to remove trailing slash ([#14245](https://github.com/astral-sh/uv/pull/14245))
- Respect URL-encoded credentials in redirect location ([#14315](https://github.com/astral-sh/uv/pull/14315))
- Lock the source tree when running setuptools, to protect concurrent builds ([#14174](https://github.com/astral-sh/uv/pull/14174))

### Documentation

- Note that GCP Artifact Registry download URLs must have `/simple` component ([#14251](https://github.com/astral-sh/uv/pull/14251))

## 0.7.15

### Enhancements

- Consistently use `Ordering::Relaxed` for standalone atomic use cases ([#14190](https://github.com/astral-sh/uv/pull/14190))
- Warn on ambiguous relative paths for `--index` ([#14152](https://github.com/astral-sh/uv/pull/14152))
- Skip GitHub fast path when rate-limited ([#13033](https://github.com/astral-sh/uv/pull/13033))
- Preserve newlines in `schema.json` descriptions ([#13693](https://github.com/astral-sh/uv/pull/13693))

### Bug fixes

- Add check for using minor version link when creating a venv on Windows ([#14252](https://github.com/astral-sh/uv/pull/14252))
- Strip query parameters when parsing source URL ([#14224](https://github.com/astral-sh/uv/pull/14224))

### Documentation

- Add a link to PyPI FAQ to clarify what per-project token is ([#14242](https://github.com/astral-sh/uv/pull/14242))

### Preview features

- Allow symlinks in the build backend ([#14212](https://github.com/astral-sh/uv/pull/14212))

## 0.7.14

### Enhancements

- Add XPU to `--torch-backend` ([#14172](https://github.com/astral-sh/uv/pull/14172))
- Add ROCm backends to `--torch-backend` ([#14120](https://github.com/astral-sh/uv/pull/14120))
- Remove preview label from `--torch-backend` ([#14119](https://github.com/astral-sh/uv/pull/14119))
- Add `[tool.uv.dependency-groups].mygroup.requires-python` ([#13735](https://github.com/astral-sh/uv/pull/13735))
- Add auto-detection for AMD GPUs ([#14176](https://github.com/astral-sh/uv/pull/14176))
- Show retries for HTTP status code errors ([#13897](https://github.com/astral-sh/uv/pull/13897))
- Support transparent Python patch version upgrades ([#13954](https://github.com/astral-sh/uv/pull/13954))
- Warn on empty index directory ([#13940](https://github.com/astral-sh/uv/pull/13940))
- Publish to DockerHub ([#14088](https://github.com/astral-sh/uv/pull/14088))

### Performance

- Make cold resolves about 10% faster ([#14035](https://github.com/astral-sh/uv/pull/14035))

### Bug fixes

- Don't use walrus operator in interpreter query script ([#14108](https://github.com/astral-sh/uv/pull/14108))
- Fix handling of changes to `requires-python` ([#14076](https://github.com/astral-sh/uv/pull/14076))
- Fix implied `platform_machine` marker for `win_amd64` platform tag ([#14041](https://github.com/astral-sh/uv/pull/14041))
- Only update existing symlink directories on preview uninstall ([#14179](https://github.com/astral-sh/uv/pull/14179))
- Serialize Python requests for tools as canonicalized strings ([#14109](https://github.com/astral-sh/uv/pull/14109))
- Support netrc and same-origin credential propagation on index redirects ([#14126](https://github.com/astral-sh/uv/pull/14126))
- Support reading `dependency-groups` from pyproject.tomls with no `[project]` ([#13742](https://github.com/astral-sh/uv/pull/13742))
- Handle an existing shebang in `uv init --script` ([#14141](https://github.com/astral-sh/uv/pull/14141))
- Prevent concurrent updates of the environment in `uv run` ([#14153](https://github.com/astral-sh/uv/pull/14153))
- Filter managed Python distributions by platform before querying when included in request ([#13936](https://github.com/astral-sh/uv/pull/13936))

### Documentation

- Replace cuda124 with cuda128 ([#14168](https://github.com/astral-sh/uv/pull/14168))
- Document the way member sources shadow workspace sources ([#14136](https://github.com/astral-sh/uv/pull/14136))
- Sync documented PyTorch integration index for CUDA and ROCm versions from PyTorch website ([#14100](https://github.com/astral-sh/uv/pull/14100))

## 0.7.13

### Python

- Add Python 3.14.0b2
- Add Python 3.13.5
- Fix stability of `uuid.getnode` on 3.13

See the
[`python-build-standalone` release notes](https://github.com/astral-sh/python-build-standalone/releases/tag/20250612)
for more details.

### Enhancements

- Download versions in `uv python pin` if not found ([#13946](https://github.com/astral-sh/uv/pull/13946))
- Use TTY detection to determine if SIGINT forwarding is enabled ([#13925](https://github.com/astral-sh/uv/pull/13925))
- Avoid fetching an exact, cached Git commit, even if it isn't locked ([#13748](https://github.com/astral-sh/uv/pull/13748))
- Add `zstd` and `deflate` to `Accept-Encoding` ([#13982](https://github.com/astral-sh/uv/pull/13982))
- Build binaries for riscv64  ([#12688](https://github.com/astral-sh/uv/pull/12688))

### Bug fixes

- Check if relative URL is valid directory before treating as index ([#13917](https://github.com/astral-sh/uv/pull/13917))
- Ignore Python discovery errors during `uv python pin` ([#13944](https://github.com/astral-sh/uv/pull/13944))
- Do not allow `uv add --group ... --script` ([#13997](https://github.com/astral-sh/uv/pull/13997))

### Preview changes

- Build backend: Support namespace packages ([#13833](https://github.com/astral-sh/uv/pull/13833))

### Documentation

- Add 3.14 to the supported platform reference ([#13990](https://github.com/astral-sh/uv/pull/13990))
- Add an `llms.txt` to uv ([#13929](https://github.com/astral-sh/uv/pull/13929))
- Add supported macOS version to the platform reference ([#13993](https://github.com/astral-sh/uv/pull/13993))
- Update platform support reference to include Python implementation list ([#13991](https://github.com/astral-sh/uv/pull/13991))
- Update pytorch.md ([#13899](https://github.com/astral-sh/uv/pull/13899))
- Update the CLI help and reference to include references to the Python bin directory ([#13978](https://github.com/astral-sh/uv/pull/13978))

## 0.7.12

### Enhancements

- Add `uv python pin --rm` to remove `.python-version` pins ([#13860](https://github.com/astral-sh/uv/pull/13860))
- Don't hint at versions removed by `excluded-newer` ([#13884](https://github.com/astral-sh/uv/pull/13884))
- Add hint to use `tool.uv.environments` on resolution error ([#13455](https://github.com/astral-sh/uv/pull/13455))
- Add hint to use `tool.uv.required-environments` on resolution error ([#13575](https://github.com/astral-sh/uv/pull/13575))
- Improve `python pin` error messages ([#13862](https://github.com/astral-sh/uv/pull/13862))

### Bug fixes

- Lock environments during `uv sync`, `uv add` and `uv remove` to prevent race conditions ([#13869](https://github.com/astral-sh/uv/pull/13869))
- Add `--no-editable` to `uv export` for `pylock.toml` ([#13852](https://github.com/astral-sh/uv/pull/13852))

### Documentation

- List `.gitignore` in project init files ([#13855](https://github.com/astral-sh/uv/pull/13855))
- Move the pip interface documentation into the concepts section ([#13841](https://github.com/astral-sh/uv/pull/13841))
- Remove the configuration section in favor of concepts / reference ([#13842](https://github.com/astral-sh/uv/pull/13842))
- Update Git and GitHub Actions docs to mention `gh auth login` ([#13850](https://github.com/astral-sh/uv/pull/13850))

### Preview

- Fix directory glob traversal fallback preventing exclusion of all files ([#13882](https://github.com/astral-sh/uv/pull/13882))

## 0.7.11

### Python

- Add Python 3.14.0b1
- Add Python 3.13.4
- Add Python 3.12.11
- Add Python 3.11.13
- Add Python 3.10.18
- Add Python 3.9.23

### Enhancements

- Add Pyodide support ([#12731](https://github.com/astral-sh/uv/pull/12731))
- Better error message for version specifier with missing operator ([#13803](https://github.com/astral-sh/uv/pull/13803))

### Bug fixes

- Downgrade `reqwest` and `hyper-util` to resolve connection reset errors over IPv6 ([#13835](https://github.com/astral-sh/uv/pull/13835))
- Prefer `uv`'s binary's version when checking if it's up to date ([#13840](https://github.com/astral-sh/uv/pull/13840))

### Documentation

- Use "terminal driver" instead of "shell" in `SIGINT` docs ([#13787](https://github.com/astral-sh/uv/pull/13787))

## 0.7.10

### Enhancements

- Add `--show-extras` to `uv tool list` ([#13783](https://github.com/astral-sh/uv/pull/13783))
- Add dynamically generated sysconfig replacement mappings ([#13441](https://github.com/astral-sh/uv/pull/13441))
- Add data locations to install wheel logs ([#13797](https://github.com/astral-sh/uv/pull/13797))

### Bug fixes

- Avoid redaction of placeholder `git` username when using SSH authentication ([#13799](https://github.com/astral-sh/uv/pull/13799))
- Propagate credentials to files on devpi indexes ending in `/+simple` ([#13743](https://github.com/astral-sh/uv/pull/13743))
- Restore retention of credentials for direct URLs in `uv export` ([#13809](https://github.com/astral-sh/uv/pull/13809))

## 0.7.9

### Python

The changes reverted in [0.7.8](#078) have been restored.

See the
[`python-build-standalone` release notes](https://github.com/astral-sh/python-build-standalone/releases/tag/20250529)
for more details.

### Enhancements

- Improve obfuscation of credentials in URLs ([#13560](https://github.com/astral-sh/uv/pull/13560))
- Allow running non-default Python implementations via `uvx` ([#13583](https://github.com/astral-sh/uv/pull/13583))
- Add `uvw` as alias for `uv` without console window on Windows ([#11786](https://github.com/astral-sh/uv/pull/11786))
- Allow discovery of x86-64 managed Python builds on macOS ([#13722](https://github.com/astral-sh/uv/pull/13722))
- Differentiate between implicit vs explicit architecture requests ([#13723](https://github.com/astral-sh/uv/pull/13723))
- Implement ordering for Python architectures to prefer native installations ([#13709](https://github.com/astral-sh/uv/pull/13709))
- Only show the first match per platform (and architecture) by default in `uv python list`  ([#13721](https://github.com/astral-sh/uv/pull/13721))
- Write the path of the parent environment to an `extends-environment` key in the `pyvenv.cfg` file of an ephemeral environment ([#13598](https://github.com/astral-sh/uv/pull/13598))
- Improve the error message when libc cannot be found, e.g., when using the distroless containers ([#13549](https://github.com/astral-sh/uv/pull/13549))

### Performance

- Avoid rendering info log level ([#13642](https://github.com/astral-sh/uv/pull/13642))
- Improve performance of `uv-python` crate's manylinux submodule ([#11131](https://github.com/astral-sh/uv/pull/11131))
- Optimize `Version` display ([#13643](https://github.com/astral-sh/uv/pull/13643))
- Reduce number of reference-checks for `uv cache clean` ([#13669](https://github.com/astral-sh/uv/pull/13669))

### Bug fixes

- Avoid reinstalling dependency group members with `--all-packages` ([#13678](https://github.com/astral-sh/uv/pull/13678))
- Don't fail direct URL hash checking with dependency metadata ([#13736](https://github.com/astral-sh/uv/pull/13736))
- Exit early on `self update` if global `--offline` is set ([#13663](https://github.com/astral-sh/uv/pull/13663))
- Fix cases where the uv lock is incorrectly marked as out of date ([#13635](https://github.com/astral-sh/uv/pull/13635))
- Include pre-release versions in `uv python install --reinstall` ([#13645](https://github.com/astral-sh/uv/pull/13645))
- Set `LC_ALL=C` for git when checking git worktree ([#13637](https://github.com/astral-sh/uv/pull/13637))
- Avoid rejecting Windows paths for remote Python download JSON targets ([#13625](https://github.com/astral-sh/uv/pull/13625))

### Preview

- Add `uv add --bounds` to configure version constraints ([#12946](https://github.com/astral-sh/uv/pull/12946))

### Documentation

- Add documentation about Python versions to Tools concept page ([#7673](https://github.com/astral-sh/uv/pull/7673))
- Add example of enabling Dependabot ([#13692](https://github.com/astral-sh/uv/pull/13692))
- Fix `exclude-newer` date format for persistent configuration files ([#13706](https://github.com/astral-sh/uv/pull/13706))
- Quote versions variables in GitLab documentation ([#13679](https://github.com/astral-sh/uv/pull/13679))
- Update Dependabot support status ([#13690](https://github.com/astral-sh/uv/pull/13690))
- Explicitly specify to add a new repo entry to the repos list item in the `.pre-commit-config.yaml` ([#10243](https://github.com/astral-sh/uv/pull/10243))
- Add integration with marimo guide ([#13691](https://github.com/astral-sh/uv/pull/13691))
- Add pronunciation to README ([#5336](https://github.com/astral-sh/uv/pull/5336))

## 0.7.8

### Python

We are reverting most of our Python changes from `uv 0.7.6` and `uv 0.7.7` due to
a miscompilation that makes the Python interpreter behave incorrectly, resulting
in spurious type-errors involving str. This issue seems to be isolated to
x86_64 Linux, and affected at least Python 3.12, 3.13, and 3.14.

The following changes that were introduced in those versions of uv are temporarily
being reverted while we test and deploy a proper fix for the miscompilation:

- Add Python 3.14 on musl
- free-threaded Python on musl
- Add Python 3.14.0a7
- Statically link `libpython` into the interpreter on Linux for a significant performance boost

See [the issue for details](https://github.com/astral-sh/uv/issues/13610).

### Documentation

- Remove misleading line in pin documentation ([#13611](https://github.com/astral-sh/uv/pull/13611))

## 0.7.7

### Python

- Work around third-party packages that (incorrectly) assume the interpreter is dynamically linking libpython
- Allow the experimental JIT to be enabled at runtime on Python 3.13 and 3.14 on macOS on aarch64 aka Apple Silicon

See the
[`python-build-standalone` release notes](https://github.com/astral-sh/python-build-standalone/releases/tag/20250521)
for more details.

### Bug fixes

- Make `uv version` lock and sync ([#13317](https://github.com/astral-sh/uv/pull/13317))
- Fix references to `ldd` in diagnostics to correctly refer to `ld.so` ([#13552](https://github.com/astral-sh/uv/pull/13552))

### Documentation

- Clarify adding SSH Git dependencies ([#13534](https://github.com/astral-sh/uv/pull/13534))

## 0.7.6

### Python

- Add Python 3.14 on musl
- Add free-threaded Python on musl
- Add Python 3.14.0a7
- Statically link `libpython` into the interpreter on Linux for a significant performance boost

See the
[`python-build-standalone` release notes](https://github.com/astral-sh/python-build-standalone/releases/tag/20250517)
for more details.

### Enhancements

- Improve compatibility of `VIRTUAL_ENV_PROMPT` value ([#13501](https://github.com/astral-sh/uv/pull/13501))
- Bump MSRV to 1.85 and Edition 2024 ([#13516](https://github.com/astral-sh/uv/pull/13516))

### Bug fixes

- Respect default extras in uv remove ([#13380](https://github.com/astral-sh/uv/pull/13380))

### Documentation

- Fix PowerShell code blocks ([#13511](https://github.com/astral-sh/uv/pull/13511))

## 0.7.5

### Bug fixes

- Support case-sensitive module discovery in the build backend ([#13468](https://github.com/astral-sh/uv/pull/13468))
- Bump Simple cache bucket to v16 ([#13498](https://github.com/astral-sh/uv/pull/13498))
- Don't error when the script is too short for the buffer ([#13488](https://github.com/astral-sh/uv/pull/13488))
- Add missing word in "script not supported" error ([#13483](https://github.com/astral-sh/uv/pull/13483))

## 0.7.4

### Enhancements

- Add more context to external errors ([#13351](https://github.com/astral-sh/uv/pull/13351))
- Align indentation of long arguments ([#13394](https://github.com/astral-sh/uv/pull/13394))
- Preserve order of dependencies which are sorted naively ([#13334](https://github.com/astral-sh/uv/pull/13334))
- Align progress bars by largest name length ([#13266](https://github.com/astral-sh/uv/pull/13266))
- Reinstall local packages in `uv add` ([#13462](https://github.com/astral-sh/uv/pull/13462))
- Rename `--raw-sources` to `--raw` ([#13348](https://github.com/astral-sh/uv/pull/13348))
- Show 'Downgraded' when `self update` is used to install an older version ([#13340](https://github.com/astral-sh/uv/pull/13340))
- Suggest `uv self update` if required uv version is newer ([#13305](https://github.com/astral-sh/uv/pull/13305))
- Add 3.14 beta images to uv Docker images ([#13390](https://github.com/astral-sh/uv/pull/13390))
- Add comma after "i.e." in Conda environment error ([#13423](https://github.com/astral-sh/uv/pull/13423))
- Be more precise in unpinned packages warning ([#13426](https://github.com/astral-sh/uv/pull/13426))
- Fix detection of sorted dependencies when include-group is used ([#13354](https://github.com/astral-sh/uv/pull/13354))
- Fix display of HTTP responses in trace logs for retry of errors ([#13339](https://github.com/astral-sh/uv/pull/13339))
- Log skip reasons during Python installation key interpreter match checks ([#13472](https://github.com/astral-sh/uv/pull/13472))
- Redact credentials when displaying URLs ([#13333](https://github.com/astral-sh/uv/pull/13333))

### Bug fixes

- Avoid erroring on `pylock.toml` dependency entries ([#13384](https://github.com/astral-sh/uv/pull/13384))
- Avoid panics for cannot-be-a-base URLs ([#13406](https://github.com/astral-sh/uv/pull/13406))
- Ensure cached realm credentials are applied if no password is found for index URL ([#13463](https://github.com/astral-sh/uv/pull/13463))
- Fix `.tgz` parsing to respect true extension ([#13382](https://github.com/astral-sh/uv/pull/13382))
- Fix double self-dependency ([#13366](https://github.com/astral-sh/uv/pull/13366))
- Reject `pylock.toml` in `uv add -r` ([#13421](https://github.com/astral-sh/uv/pull/13421))
- Retain dot-separated wheel tags during cache prune ([#13379](https://github.com/astral-sh/uv/pull/13379))
- Retain trailing comments after PEP 723 metadata block ([#13460](https://github.com/astral-sh/uv/pull/13460))

### Documentation

- Use "export" instead of "install" in `uv export` arguments ([#13430](https://github.com/astral-sh/uv/pull/13430))
- Remove extra newline ([#13461](https://github.com/astral-sh/uv/pull/13461))

### Preview features

- Build backend: Normalize glob paths ([#13465](https://github.com/astral-sh/uv/pull/13465))

## 0.7.3

### Enhancements

- Add `--dry-run` support to `uv self update` ([#9829](https://github.com/astral-sh/uv/pull/9829))
- Add `--show-with` to `uv tool list` to list packages included by `--with` ([#13264](https://github.com/astral-sh/uv/pull/13264))
- De-duplicate fetched index URLs ([#13205](https://github.com/astral-sh/uv/pull/13205))
- Support more zip compression formats: bzip2, lzma, xz, zstd ([#13285](https://github.com/astral-sh/uv/pull/13285))
- Add support for downloading GraalPy ([#13172](https://github.com/astral-sh/uv/pull/13172))
- Improve error message when a virtual environment Python symlink is broken ([#12168](https://github.com/astral-sh/uv/pull/12168))
- Use `fs_err` for paths in symlinking errors ([#13303](https://github.com/astral-sh/uv/pull/13303))
- Minify and embed managed Python JSON at compile time ([#12967](https://github.com/astral-sh/uv/pull/12967))

### Preview features

- Build backend: Make preview default and add configuration docs ([#12804](https://github.com/astral-sh/uv/pull/12804))
- Build backend: Allow escaping in globs ([#13313](https://github.com/astral-sh/uv/pull/13313))
- Build backend: Make builds reproducible across operating systems ([#13171](https://github.com/astral-sh/uv/pull/13171))

### Configuration

- Add `python-downloads-json-url` option for `uv.toml` to configure custom Python installations via JSON URL ([#12974](https://github.com/astral-sh/uv/pull/12974))

### Bug fixes

- Check nested IO errors for retries ([#13260](https://github.com/astral-sh/uv/pull/13260))
- Accept `musllinux_1_0` as a valid platform tag ([#13289](https://github.com/astral-sh/uv/pull/13289))
- Fix discovery of pre-release managed Python versions in range requests ([#13330](https://github.com/astral-sh/uv/pull/13330))
- Respect locked script preferences in `uv run --with` ([#13283](https://github.com/astral-sh/uv/pull/13283))
- Retry streaming downloads on broken pipe errors ([#13281](https://github.com/astral-sh/uv/pull/13281))
- Treat already-installed base environment packages as preferences in `uv run --with` ([#13284](https://github.com/astral-sh/uv/pull/13284))
- Avoid enumerating sources in errors for path Python requests ([#13335](https://github.com/astral-sh/uv/pull/13335))
- Avoid re-creating virtual environment with `--no-sync` ([#13287](https://github.com/astral-sh/uv/pull/13287))

### Documentation

- Remove outdated description of index strategy ([#13326](https://github.com/astral-sh/uv/pull/13326))
- Update "Viewing the version" docs ([#13241](https://github.com/astral-sh/uv/pull/13241))

## 0.7.2

### Enhancements

- Improve trace log for retryable errors ([#13228](https://github.com/astral-sh/uv/pull/13228))
- Use "error" instead of "warning" for self-update message ([#13229](https://github.com/astral-sh/uv/pull/13229))
- Error when `uv version` is used with project-specific flags but no project is found ([#13203](https://github.com/astral-sh/uv/pull/13203))

### Bug fixes

- Fix incorrect virtual environment invalidation for pre-release Python versions ([#13234](https://github.com/astral-sh/uv/pull/13234))
- Fix patching of `clang` in managed Python sysconfig ([#13237](https://github.com/astral-sh/uv/pull/13237))
- Respect `--project` in `uv version` ([#13230](https://github.com/astral-sh/uv/pull/13230))

## 0.7.1

### Enhancement

- Add support for BLAKE2b-256 ([#13204](https://github.com/astral-sh/uv/pull/13204))

### Bugfix

- Revert fix handling of authentication when encountering redirects ([#13215](https://github.com/astral-sh/uv/pull/13215))

## 0.7.0

This release contains various changes that improve correctness and user experience, but could break some workflows; many changes have been marked as breaking out of an abundance of caution. We expect most users to be able to upgrade without making changes.

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

  Since PyTorch's indexes always return a HTTP 403 for missing packages, uv special-cases indexes on the `pytorch.org` domain to ignore that error code by default.
- **Require the command in `uvx <name>` to be available in the Python environment ([#11603](https://github.com/astral-sh/uv/pull/11603))**

  Previously, `uvx` would attempt to execute a command even if it was not provided by a Python package. For example, if we presume `foo` is an empty Python package which provides no command, `uvx foo` would invoke the `foo` command on the `PATH` (if present). Now, uv will error early if the `foo` executable is not provided by the requested Python package. This check is not enforced when `--from` is used, so patterns like `uvx --from foo bash -c "..."` are still valid. uv also still allows `uvx foo` where the `foo` executable is provided by a dependency of `foo` instead of `foo` itself, as this is fairly common for packages which depend on a dedicated package for their command-line interface.
- **Use index URL instead of package URL for keyring credential lookups ([#12651](https://github.com/astral-sh/uv/pull/12651))**

  When determining credentials for querying a package URL, uv previously sent the full URL to the `keyring` command. However, some keyring plugins expect to receive the *index URL* (which is usually a parent of the package URL). Now, uv requests credentials for the index URL instead. This behavior matches `pip`.
- **Remove `--version` from subcommands ([#13108](https://github.com/astral-sh/uv/pull/13108))**

  Previously, uv allowed the `--version` flag on arbitrary subcommands, e.g., `uv run --version`. However, the `--version` flag is useful for other operations since uv is a package manager. Consequently, we've removed the `--version` flag from subcommands — it is only available as `uv --version`.
- **Omit Python 3.7 downloads from managed versions ([#13022](https://github.com/astral-sh/uv/pull/13022))**

  Python 3.7 is EOL and not formally supported by uv; however, Python 3.7 was previously available for download on a subset of platforms.
- **Reject non-PEP 751 TOML files in install, compile, and export commands ([#13120](https://github.com/astral-sh/uv/pull/13120), [#13119](https://github.com/astral-sh/uv/pull/13119))**

  Previously, uv treated arbitrary `.toml` files passed to commands (e.g., `uv pip install -r foo.toml` or `uv pip compile -o foo.toml`) as `requirements.txt`-formatted files. Now, uv will error instead. If using PEP 751 lockfiles, use the standardized format for custom names instead, e.g., `pylock.foo.toml`.
- **Ignore arbitrary Python requests in version files ([#12909](https://github.com/astral-sh/uv/pull/12909))**

  uv allows arbitrary strings to be used for Python version requests, in which they are treated as an executable name to search for in the `PATH`. However, using this form of request in `.python-version` files is non-standard and conflicts with `pyenv-virtualenv` which writes environment names to `.python-version` files. In this release, uv will now ignore requests that are arbitrary strings when found in `.python-version` files.
- **Error on unknown dependency object specifiers ([12811](https://github.com/astral-sh/uv/pull/12811))**

  The `[dependency-groups]` entries can include "object specifiers", e.g. `set-phasers-to = ...` in:

  ```toml
  [dependency-groups]
  foo = ["pyparsing"]
  bar = [{set-phasers-to = "stun"}]
  ```

  However, the only current spec-compliant object specifier is `include-group`. Previously, uv would ignore unknown object specifiers. Now, uv will error.
- **Make `--frozen` and `--no-sources` conflicting options ([#12671](https://github.com/astral-sh/uv/pull/12671))**

  Using `--no-sources` always requires a new resolution and `--frozen` will always fail when used with it. Now, this conflict is encoded in the CLI options for clarity.
- **Treat empty `UV_PYTHON_INSTALL_DIR` and `UV_TOOL_DIR` as unset ([#12907](https://github.com/astral-sh/uv/pull/12907), [#12905](https://github.com/astral-sh/uv/pull/12905))**

  Previously, these variables were treated as set to the current working directory when set to an empty string. Now, uv will ignore these variables when empty. This matches uv's behavior for other environment variables which configure directories.

### Enhancements

- Disallow mixing requirements across PyTorch indexes ([#13179](https://github.com/astral-sh/uv/pull/13179))
- Add optional managed Python archive download cache ([#12175](https://github.com/astral-sh/uv/pull/12175))
- Add `poetry-core` as a `uv init` build backend option ([#12781](https://github.com/astral-sh/uv/pull/12781))
- Show tag hints when failing to find a compatible wheel in `pylock.toml` ([#13136](https://github.com/astral-sh/uv/pull/13136))
- Report Python versions in `pyvenv.cfg` version mismatch ([#13027](https://github.com/astral-sh/uv/pull/13027))

### Bug fixes

- Avoid erroring on omitted wheel-only packages in `pylock.toml` ([#13132](https://github.com/astral-sh/uv/pull/13132))
- Fix display name for `uvx --version` ([#13109](https://github.com/astral-sh/uv/pull/13109))
- Restore handling of authentication when encountering redirects ([#13050](https://github.com/astral-sh/uv/pull/13050))
- Respect build options (`--no-binary` et al) in `pylock.toml` ([#13134](https://github.com/astral-sh/uv/pull/13134))
- Use `upload-time` rather than `upload_time` in `uv.lock` ([#13176](https://github.com/astral-sh/uv/pull/13176))

### Documentation

- Changed `fish` completions append `>>` to overwrite `>` ([#13130](https://github.com/astral-sh/uv/pull/13130))
- Add `pylock.toml` mentions where relevant ([#13115](https://github.com/astral-sh/uv/pull/13115))
- Add ROCm example to the PyTorch guide ([#13200](https://github.com/astral-sh/uv/pull/13200))
- Upgrade PyTorch guide to CUDA 12.8 and PyTorch 2.7 ([#13199](https://github.com/astral-sh/uv/pull/13199))

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


