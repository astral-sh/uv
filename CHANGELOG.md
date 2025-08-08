# Changelog

<!-- prettier-ignore-start -->


## 0.8.7

### Python

- On Mac/Linux, libtcl, libtk, and _tkinter are built as separate shared objects, which fixes matplotlib's `tkagg` backend (the default on Linux), Pillow's `PIL.ImageTk` library, and other extension modules that need to use libtcl/libtk directly.
- Tix is no longer provided on Linux. This is a deprecated Tk extension that appears to have been previously broken.

See the [`python-build-standalone` release notes](https://github.com/astral-sh/python-build-standalone/releases/tag/20250808) for details.

### Enhancements

- Do not update `uv.lock` when using `--isolated` ([#15154](https://github.com/astral-sh/uv/pull/15154))
- Add support for `--prefix` and `--with` installations in `find_uv_bin` ([#14184](https://github.com/astral-sh/uv/pull/14184))
- Add support for discovering base prefix installations in `find_uv_bin` ([#14181](https://github.com/astral-sh/uv/pull/14181))
- Improve error messages in `find_uv_bin` ([#14182](https://github.com/astral-sh/uv/pull/14182))
- Warn when two packages write to the same module ([#13437](https://github.com/astral-sh/uv/pull/13437))

### Preview features

- Add support for `package`-level conflicts in workspaces ([#14906](https://github.com/astral-sh/uv/pull/14906))

### Configuration

- Add `UV_DEV` and `UV_NO_DEV` environment variables (for `--dev` and `--no-dev`) ([#15010](https://github.com/astral-sh/uv/pull/15010))

### Bug fixes

- Fix regression where `--require-hashes` applied to build dependencies in `uv pip install` ([#15153](https://github.com/astral-sh/uv/pull/15153))
- Ignore GraalPy devtags ([#15013](https://github.com/astral-sh/uv/pull/15013))
- Include all site packages directories in ephemeral environment overlays ([#15121](https://github.com/astral-sh/uv/pull/15121))
- Search in the user scheme scripts directory last in `find_uv_bin` ([#14191](https://github.com/astral-sh/uv/pull/14191))

### Documentation

- Add missing periods (`.`) to list elements in `Features` docs page ([#15138](https://github.com/astral-sh/uv/pull/15138))

## 0.8.6

This release contains hardening measures to address differentials in behavior between uv and Python's built-in ZIP parser ([CVE-2025-54368](https://github.com/astral-sh/uv/security/advisories/GHSA-8qf3-x8v5-2pj8)).

Prior to this release, attackers could construct ZIP files that would be extracted differently by pip, uv, and other tools. As a result, ZIPs could be constructed that would be considered harmless by (e.g.) scanners, but contain a malicious payload when extracted by uv. As of v0.8.6, uv now applies additional checks to reject such ZIPs.

Thanks to a triage effort with the [Python Security Response Team](https://devguide.python.org/developer-workflow/psrt/) and PyPI maintainers, we were able to determine that these differentials **were not exploited** via PyPI during the time they were present. The PyPI team has also implemented similar checks and now guards against these parsing differentials on upload.

Although the practical risk of exploitation is low, we take the *hypothetical* risk of parser differentials very seriously. Out of an abundance of caution, we have assigned this advisory a CVE identifier and have given it a "moderate" severity suggestion.

These changes have been validated against the top 15,000 PyPI packages; however, it's plausible that a non-malicious ZIP could be falsely rejected with this additional hardening. As an escape hatch, users who do encounter breaking changes can enable `UV_INSECURE_NO_ZIP_VALIDATION` to restore the previous behavior. If you encounter such a rejection, please file an issue in uv and to the upstream package.

For additional information, please refer to the following blog posts:

* [Astral: uv security advisory: ZIP payload obfuscation](https://astral.sh/blog/uv-security-advisory-cve-2025-54368)
* [PyPI: Preventing ZIP parser confusion attacks on Python package installers](https://blog.pypi.org/posts/2025-08-07-wheel-archive-confusion-attacks/)

### Security

- Harden ZIP streaming to reject repeated entries and other malformed ZIP files ([#15136](https://github.com/astral-sh/uv/pull/15136))

### Python

- Add CPython 3.13.6

### Configuration

- Add support for per-project build-time environment variables ([#15095](https://github.com/astral-sh/uv/pull/15095))

### Bug fixes

- Avoid invalid simplification with conflict markers  ([#15041](https://github.com/astral-sh/uv/pull/15041))
- Respect `UV_HTTP_RETRIES` in `uv publish` ([#15106](https://github.com/astral-sh/uv/pull/15106))
- Support `UV_NO_EDITABLE` where `--no-editable` is supported ([#15107](https://github.com/astral-sh/uv/pull/15107))
- Upgrade `cargo-dist` to add `UV_INSTALLER_URL` to PowerShell installer ([#15114](https://github.com/astral-sh/uv/pull/15114))
- Upgrade `h2` again to avoid `too_many_internal_resets` errors ([#15111](https://github.com/astral-sh/uv/pull/15111))
- Consider `pythonw` when copying entry points in uv run ([#15134](https://github.com/astral-sh/uv/pull/15134))

### Documentation

- Ensure symlink warning is shown ([#15126](https://github.com/astral-sh/uv/pull/15126))

## 0.8.5

### Enhancements

- Enable `uv run` with a GitHub Gist ([#15058](https://github.com/astral-sh/uv/pull/15058))
- Improve HTTP response caching log messages ([#15067](https://github.com/astral-sh/uv/pull/15067))
- Show wheel tag hints in install plan ([#15066](https://github.com/astral-sh/uv/pull/15066))
- Support installing additional executables in `uv tool install` ([#14014](https://github.com/astral-sh/uv/pull/14014))

### Preview features

- Enable extra build dependencies to 'match runtime' versions ([#15036](https://github.com/astral-sh/uv/pull/15036))
- Remove duplicate `extra-build-dependencies` warnings for `uv pip` ([#15088](https://github.com/astral-sh/uv/pull/15088))
- Use "option" instead of "setting" in `pylock` warning ([#15089](https://github.com/astral-sh/uv/pull/15089))
- Respect extra build requires when reading from wheel cache ([#15030](https://github.com/astral-sh/uv/pull/15030))
- Preserve lowered extra build dependencies ([#15038](https://github.com/astral-sh/uv/pull/15038))

### Bug fixes

- Add Python versions to markers implied from wheels ([#14913](https://github.com/astral-sh/uv/pull/14913))
- Ensure consistent indentation when adding dependencies ([#14991](https://github.com/astral-sh/uv/pull/14991))
- Fix handling of `python-preference = system` when managed interpreters are on the PATH ([#15059](https://github.com/astral-sh/uv/pull/15059))
- Fix symlink preservation in virtual environment creation ([#14933](https://github.com/astral-sh/uv/pull/14933))
- Gracefully handle entrypoint permission errors ([#15026](https://github.com/astral-sh/uv/pull/15026))
- Include wheel hashes from local Simple indexes ([#14993](https://github.com/astral-sh/uv/pull/14993))
- Prefer system Python installations over managed ones when `--system` is used ([#15061](https://github.com/astral-sh/uv/pull/15061))
- Remove retry wrapper when matching on error kind ([#14996](https://github.com/astral-sh/uv/pull/14996))
- Revert `h2` upgrade ([#15079](https://github.com/astral-sh/uv/pull/15079))

### Documentation

- Improve visibility of copy and line separator in dark mode ([#14987](https://github.com/astral-sh/uv/pull/14987))

## 0.8.4

### Enhancements

- Improve styling of warning cause chains  ([#14934](https://github.com/astral-sh/uv/pull/14934))
- Extend wheel filtering to Android tags ([#14977](https://github.com/astral-sh/uv/pull/14977))
- Perform wheel lockfile filtering based on platform and OS intersection ([#14976](https://github.com/astral-sh/uv/pull/14976))
- Clarify messaging when a new resolution needs to be performed ([#14938](https://github.com/astral-sh/uv/pull/14938))

### Preview features

- Add support for extending package's build dependencies with `extra-build-dependencies` ([#14735](https://github.com/astral-sh/uv/pull/14735))
- Split preview mode into separate feature flags ([#14823](https://github.com/astral-sh/uv/pull/14823))

### Configuration

- Add support for package specific `exclude-newer` dates via `exclude-newer-package` ([#14489](https://github.com/astral-sh/uv/pull/14489))

### Bug fixes

- Avoid invalidating lockfile when path or workspace dependencies define explicit indexes ([#14876](https://github.com/astral-sh/uv/pull/14876))
- Copy entrypoints that have a shebang that differs in `python` vs `python3` ([#14970](https://github.com/astral-sh/uv/pull/14970))
- Fix incorrect file permissions in wheel packages ([#14930](https://github.com/astral-sh/uv/pull/14930))
- Update validation for `environments` and `required-environments` in `uv.toml` ([#14905](https://github.com/astral-sh/uv/pull/14905))

### Documentation

- Show `uv_build` in projects documentation ([#14968](https://github.com/astral-sh/uv/pull/14968))
- Add `UV_` prefix to installer environment variables ([#14964](https://github.com/astral-sh/uv/pull/14964))
- Un-hide `uv` from `--build-backend` options ([#14939](https://github.com/astral-sh/uv/pull/14939))
- Update documentation for preview flags ([#14902](https://github.com/astral-sh/uv/pull/14902))

## 0.8.3

### Python

- Add CPython 3.14.0rc1

See the [`python-build-standalone` release notes](https://github.com/astral-sh/python-build-standalone/releases/tag/20250723) for more details.

### Enhancements

- Allow non-standard entrypoint names in `uv_build` ([#14867](https://github.com/astral-sh/uv/pull/14867))
- Publish riscv64 wheels to PyPI ([#14852](https://github.com/astral-sh/uv/pull/14852))

### Bug fixes

- Avoid writing redacted credentials to tool receipt ([#14855](https://github.com/astral-sh/uv/pull/14855))
- Respect `--with` versions over base environment versions ([#14863](https://github.com/astral-sh/uv/pull/14863))
- Respect credentials from all defined indexes ([#14858](https://github.com/astral-sh/uv/pull/14858))
- Fix missed stabilization of removal of registry entry during Python uninstall ([#14859](https://github.com/astral-sh/uv/pull/14859))
- Improve concurrency safety of Python downloads into cache ([#14846](https://github.com/astral-sh/uv/pull/14846))

### Documentation

- Fix typos in `uv_build` reference documentation ([#14853](https://github.com/astral-sh/uv/pull/14853))
- Move the "Cargo" install method further down in docs ([#14842](https://github.com/astral-sh/uv/pull/14842))

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


