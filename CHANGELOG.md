# Changelog

<!-- prettier-ignore-start -->


## 0.8.2

### Enhancements

- Add derivation chains for dependency errors ([#14824](https://github.com/astral-sh/uv/pull/14824))

### Configuration

- Add `UV_INIT_BUILD_BACKEND` ([#14821](https://github.com/astral-sh/uv/pull/14821))

### Bug fixes

- Avoid reading files in the environment bin that are not entrypoints ([#14830](https://github.com/astral-sh/uv/pull/14830))
- Avoid removing empty directories when constructing virtual environments ([#14822](https://github.com/astral-sh/uv/pull/14822))
- Preserve index URL priority order when writing to pyproject.toml ([#14831](https://github.com/astral-sh/uv/pull/14831))

### Rust API

- Expose `tls_built_in_root_certs` for client ([#14816](https://github.com/astral-sh/uv/pull/14816))

### Documentation

- Archive the 0.7.x changelog ([#14819](https://github.com/astral-sh/uv/pull/14819))

## 0.8.1

### Enhancements

- Add support for `HF_TOKEN` ([#14797](https://github.com/astral-sh/uv/pull/14797))
- Allow `--config-settings-package` to apply configuration settings at the package level ([#14573](https://github.com/astral-sh/uv/pull/14573))
- Create (e.g.) `python3.13t` executables in `uv venv` ([#14764](https://github.com/astral-sh/uv/pull/14764))
- Disallow writing symlinks outside the source distribution target directory ([#12259](https://github.com/astral-sh/uv/pull/12259))
- Elide traceback when `python -m uv` in interrupted with Ctrl-C on Windows ([#14715](https://github.com/astral-sh/uv/pull/14715))
- Match `--bounds` formatting for `uv_build` bounds in `uv init` ([#14731](https://github.com/astral-sh/uv/pull/14731))
- Support `extras` and `dependency_groups` markers in PEP 508 grammar ([#14753](https://github.com/astral-sh/uv/pull/14753))
- Support `extras` and `dependency_groups` markers on `uv pip install` and `uv pip sync` ([#14755](https://github.com/astral-sh/uv/pull/14755))
- Add hint to use `uv self version` when `uv version` cannot find a project ([#14738](https://github.com/astral-sh/uv/pull/14738))
- Improve error reporting when removing Python versions from the Windows registry ([#14722](https://github.com/astral-sh/uv/pull/14722))
- Make warnings about masked `[tool.uv]` fields more precise ([#14325](https://github.com/astral-sh/uv/pull/14325))

### Preview features

- Emit JSON output in `uv sync` with `--quiet` ([#14810](https://github.com/astral-sh/uv/pull/14810))

### Bug fixes

- Allow removal of virtual environments with missing interpreters ([#14812](https://github.com/astral-sh/uv/pull/14812))
- Apply `Cache-Control` overrides to response, not request headers ([#14736](https://github.com/astral-sh/uv/pull/14736))
- Copy entry points into ephemeral environments to ensure layers are respected ([#14790](https://github.com/astral-sh/uv/pull/14790))
- Workaround Jupyter Lab application directory discovery in ephemeral environments ([#14790](https://github.com/astral-sh/uv/pull/14790))
- Enforce `requires-python` in `pylock.toml` ([#14787](https://github.com/astral-sh/uv/pull/14787))
- Fix kebab casing of `README` variants in build backend ([#14762](https://github.com/astral-sh/uv/pull/14762))
- Improve concurrency resilience of removing Python versions from the Windows registry ([#14717](https://github.com/astral-sh/uv/pull/14717))
- Retry HTTP requests on invalid data errors ([#14703](https://github.com/astral-sh/uv/pull/14703))
- Update virtual environment removal to delete `pyvenv.cfg` last ([#14808](https://github.com/astral-sh/uv/pull/14808))
- Error on unknown fields in `dependency-metadata` ([#14801](https://github.com/astral-sh/uv/pull/14801))

### Documentation

- Recommend installing `setup-uv` after `setup-python` in Github Actions integration guide ([#14741](https://github.com/astral-sh/uv/pull/14741))
- Clarify which portions of `requires-python` behavior are consistent with pip ([#14752](https://github.com/astral-sh/uv/pull/14752))

## 0.8.0

Since we released uv [0.7.0](https://github.com/astral-sh/uv/releases/tag/0.7.0) in April, we've accumulated various changes that improve correctness and user experience, but could break some workflows. This release contains those changes; many have been marked as breaking out of an abundance of caution. We expect most users to be able to upgrade without making changes.

This release also includes the stabilization of a couple `uv python install` features, which have been available under preview since late last year.

### Breaking changes

- **Install Python executables into a directory on the `PATH` ([#14626](https://github.com/astral-sh/uv/pull/14626))**
  
  `uv python install` now installs a versioned Python executable (e.g., `python3.13`) into a directory on the `PATH` (e.g., `~/.local/bin`) by default. This behavior has been available under the `--preview` flag since [Oct 2024](https://github.com/astral-sh/uv/pull/8458). This change should not be breaking unless it shadows a Python executable elsewhere on the `PATH`.
  
  To install unversioned executables, i.e., `python3` and `python`, use the `--default` flag. The `--default` flag has also been in preview, but is not stabilized in this release.
  
  Note that these executables point to the base Python installation and only include the standard library. That means they will not include dependencies from your current project (use `uv run python` instead) and you cannot install packages into their environment (use `uvx --with <package> python` instead).
  
  As with tool installation, the target directory respects common variables like `XDG_BIN_HOME` and can be overridden with a `UV_PYTHON_BIN_DIR` variable.
  
  You can opt out of this behavior with `uv python install --no-bin` or `UV_PYTHON_INSTALL_BIN=0`.
  
  See the [documentation on installing Python executables](https://docs.astral.sh/uv/concepts/python-versions/#installing-python-executables) for more details.
- **Register Python versions with the Windows Registry ([#14625](https://github.com/astral-sh/uv/pull/14625))**
  
  `uv python install` now registers the installed Python version with the Windows Registry as specified by [PEP 514](https://peps.python.org/pep-0514/). This allows using uv installed Python versions via the `py` launcher. This behavior has been available under the `--preview` flag since [Jan 2025](https://github.com/astral-sh/uv/pull/10634). This change should not be breaking, as using the uv Python versions with `py` requires explicit opt in.
  
  You can opt out of this behavior with `uv python install --no-registry` or `UV_PYTHON_INSTALL_REGISTRY=0`.
- **Prompt before removing an existing directory in `uv venv` ([#14309](https://github.com/astral-sh/uv/pull/14309))**
  
  Previously, `uv venv` would remove an existing virtual environment without confirmation. While this is consistent with the behavior of project commands (e.g., `uv sync`), it's surprising to users that are using imperative workflows (i.e., `uv pip`). Now, `uv venv` will prompt for confirmation before removing an existing virtual environment. **If not in an interactive context, uv will still remove the virtual environment for backwards compatibility. However, this behavior is likely to change in a future release.**
  
  The behavior for other commands (e.g., `uv sync`) is unchanged.
  
  You can opt out of this behavior by setting `UV_VENV_CLEAR=1` or passing the `--clear` flag.
- **Validate that discovered interpreters meet the Python preference ([#7934](https://github.com/astral-sh/uv/pull/7934))**
  
  uv allows opting out of its managed Python versions with the `--no-managed-python` and `python-preference` options.
  
  Previously, uv would not enforce this option for Python interpreters discovered on the `PATH`. For example, if a symlink to a managed Python interpreter was created, uv would allow it to be used even if `--no-managed-python` was provided. Now, uv ignores Python interpreters that do not match the Python preference *unless* they are in an active virtual environment or are explicitly requested, e.g., with `--python /path/to/python3.13`.
  
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
  
  As described above for dependencies with `path` sources, uv previously would not build and install workspace members that did not declare a build system. Now, uv will build and install workspace members that are a dependency of *another* workspace member regardless of the presence of a build system. The behavior is unchanged for workspace members that are not included in the `project.dependencies`, `project.optional-dependencies`, or `dependency-groups` tables of another workspace member.
  
  You can opt out of this behavior by setting `tool.uv.package = false` in the workspace member's `pyproject.toml`.
  
  See the documentation on [virtual dependencies](https://docs.astral.sh/uv/concepts/projects/dependencies/#virtual-dependencies) for details.
- **Bump `--python-platform linux` to `manylinux_2_28` ([#14300](https://github.com/astral-sh/uv/pull/14300))**
  
  uv allows performing [platform-specific resolution](https://docs.astral.sh/uv/concepts/resolution/#platform-specific-resolution) for explicit targets and provides short aliases, e.g., `linux`, for common targets.
  
  Previously, the default target for `--python-platform linux` was `manylinux_2_17`, which is compatible with most Linux distributions from 2014 or newer. We now default to `manylinux_2_28`, which is compatible with most Linux distributions from 2019 or newer.  This change follows the lead of other tools, such as `cibuildwheel`, which changed their default to `manylinux_2_28` in [Mar 2025](https://github.com/pypa/cibuildwheel/pull/2330).
  
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
  
  When using `uv run --with`, uv layers the requirements requested using `--with` into another virtual environment and caches it. Previously, uv would invoke the Python interpreter in this layered environment. However, this allows poisoning the cached environment and introduces race conditions for concurrent invocations. Now, uv will layer *another* empty virtual environment on top of the cached environment and invoke the Python interpreter there. This should only cause breakage in cases where the environment is being inspected at runtime.
  
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

## 0.7.x

See [changelogs/0.7.x](./changelogs/0.7.x.md)

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


