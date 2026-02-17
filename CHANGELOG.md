# Changelog

<!-- prettier-ignore-start -->


## 0.10.4

Released on 2026-02-17.

### Enhancements

- Remove duplicate references to the affected paths when showing `uv python` errors ([#18008](https://github.com/astral-sh/uv/pull/18008))
- Skip discovery of workspace members that contain only git-ignored files, including in sub-directories ([#18051](https://github.com/astral-sh/uv/pull/18051))

### Bug fixes

- Don't panic when initialising a package at the filesystem root (e.g. `uv init / --name foo`) ([#17983](https://github.com/astral-sh/uv/pull/17983))
- Fix permissions on `wheel` and `sdist` files produced by the `uv_build` build backend ([#18020](https://github.com/astral-sh/uv/pull/18020))
- Revert locked file change to fix locked files on NFS mounts ([#18071](https://github.com/astral-sh/uv/pull/18071))

## 0.10.3

Released on 2026-02-16.

### Python

- Add CPython 3.15.0a6

### Enhancements

- Don't open file locks for writing ([#17956](https://github.com/astral-sh/uv/pull/17956))
- Make Windows trampoline error messages consistent with uv proper ([#17969](https://github.com/astral-sh/uv/pull/17969))
- Log which preview features are enabled ([#17968](https://github.com/astral-sh/uv/pull/17968))

### Preview features

- Add support for ruff version constraints and `exclude-newer` in `uv format` ([#17651](https://github.com/astral-sh/uv/pull/17651))
- Fix script path handling when `target-workspace-discovery` is enabled ([#17965](https://github.com/astral-sh/uv/pull/17965))
- Use version constraints to select the default ruff version used by `uv format` ([#17977](https://github.com/astral-sh/uv/pull/17977))

### Bug fixes

- Avoid matching managed Python versions by prefixes, e.g. don't match CPython 3.10 when `cpython-3.1` is specified ([#17972](https://github.com/astral-sh/uv/pull/17972))
- Fix handling of `--allow-existing` with minor version links on Windows ([#17978](https://github.com/astral-sh/uv/pull/17978))
- Fix panic when encountering unmanaged workspace members ([#17974](https://github.com/astral-sh/uv/pull/17974))
- Improve accuracy of request timing ([#18007](https://github.com/astral-sh/uv/pull/18007))
- Reject `u64::MAX` in version segments to prevent overflow ([#17985](https://github.com/astral-sh/uv/pull/17985))

### Documentation

- Reference Debian Trixie instead of Bookworm ([#17991](https://github.com/astral-sh/uv/pull/17991))


## 0.10.2

Released on 2026-02-10.

### Enhancements

- Deprecate unexpected ZIP compression methods ([#17946](https://github.com/astral-sh/uv/pull/17946))

### Bug fixes

- Fix `cargo-install` failing due to missing `uv-test` dependency ([#17954](https://github.com/astral-sh/uv/pull/17954))

## 0.10.1

Released on 2026-02-10.

### Enhancements

- Don't panic on metadata read errors ([#17904](https://github.com/astral-sh/uv/pull/17904))
- Skip empty workspace members instead of failing ([#17901](https://github.com/astral-sh/uv/pull/17901))
- Don't fail creating a read-only `sdist-vX/.git` if it already exists ([#17825](https://github.com/astral-sh/uv/pull/17825))

### Documentation

- Suggest `uv python update-shell` over `uv tool update-shell` in Python docs ([#17941](https://github.com/astral-sh/uv/pull/17941))

## 0.10.0

Since we released uv [0.9.0](https://github.com/astral-sh/uv/releases/tag/0.9.0) in October of 2025, we've accumulated various changes that improve correctness and user experience, but could break some workflows. This release contains those changes; many have been marked as breaking out of an abundance of caution. We expect most users to be able to upgrade without making changes.

This release also includes the stabilization of preview features. Python upgrades are now stable, including the `uv python upgrade` command, `uv python install --upgrade`, and automatically upgrading Python patch versions in virtual environments when a new version is installed. The `add-bounds` and `extra-build-dependencies` settings are now stable. Finally, the `uv workspace dir` and `uv workspace list` utilities for writing scripts against workspace members are now stable.

There are no breaking changes to [`uv_build`](https://docs.astral.sh/uv/concepts/build-backend/). If you have an upper bound in your `[build-system]` table, you should update it, e.g., from `<0.10.0` to `<0.11.0`.

### Breaking changes

- **Require `--clear` to remove existing virtual environments in `uv venv`** ([#17757](https://github.com/astral-sh/uv/pull/17757))

  Previously, `uv venv` would prompt for confirmation before removing an existing virtual environment in interactive contexts, and remove it without confirmation in non-interactive contexts. Now, `uv venv` requires the `--clear` flag to remove an existing virtual environment. A warning for this change was added in [uv 0.8](https://github.com/astral-sh/uv/blob/main/changelogs/0.8.x.md#breaking-changes).

  You can opt out of this behavior by passing the `--clear` flag or setting `UV_VENV_CLEAR=1`.
- **Error if multiple indexes include `default = true`** ([#17011](https://github.com/astral-sh/uv/pull/17011))

  Previously, uv would silently accept multiple indexes with `default = true` and use the first one. Now, uv will error if multiple indexes are marked as the default.

  You cannot opt out of this behavior. Remove `default = true` from all but one index.
- **Error when an `explicit` index is unnamed** ([#17777](https://github.com/astral-sh/uv/pull/17777))

  Explicit indexes can only be used via the `[tool.uv.sources]` table, which requires referencing the index by name. Previously, uv would silently accept unnamed explicit indexes, which could never be referenced. Now, uv will error if an explicit index does not have a name.

  You cannot opt out of this behavior. Add a `name` to the explicit index or remove the entry.
- **Install alternative Python executables using their implementation name** ([#17756](https://github.com/astral-sh/uv/pull/17756), [#17760](https://github.com/astral-sh/uv/pull/17760))

  Previously, `uv python install` would install PyPy, GraalPy, and Pyodide executables with names like `python3.10` into the bin directory. Now, these executables will be named using their implementation name, e.g., `pypy3.10`, `graalpy3.10`, and `pyodide3.12`, to avoid conflicting with CPython installations.

  You cannot opt out of this behavior.
- **Respect global Python version pins in `uv tool run` and `uv tool install`** ([#14112](https://github.com/astral-sh/uv/pull/14112))

  Previously, `uv tool run` and `uv tool install` did not respect the global Python version pin (set via `uv python pin --global`). Now, these commands will use the global Python version when no explicit version is requested.

  For `uv tool install`, if the tool is already installed, the Python version will not change unless `--reinstall` or `--python` is provided. If the tool was previously installed with an explicit `--python` flag, the global pin will not override it.

  You can opt out of this behavior by providing an explicit `--python` flag.
- **Remove Debian Bookworm, Alpine 3.21, and Python 3.8 Docker images** ([#17755](https://github.com/astral-sh/uv/pull/17755))

  The Debian Bookworm and Alpine 3.21 images were replaced by Debian Trixie and Alpine 3.22 as defaults in [uv 0.9](https://github.com/astral-sh/uv/pull/15352). These older images are now removed. Python 3.8 images are also removed, as Python 3.8 is no longer supported in the Trixie or Alpine base images.

  The following image tags are no longer published:
  - `uv:bookworm`, `uv:bookworm-slim`
  - `uv:alpine3.21`
  - `uv:python3.8-*`

  Use `uv:debian` or `uv:trixie` instead of `uv:bookworm`, `uv:alpine` or `uv:alpine3.22` instead of `uv:alpine3.21`, and a newer Python version instead of `uv:python3.8-*`.
- **Drop PPC64 (big endian) builds** ([#17626](https://github.com/astral-sh/uv/pull/17626))

  uv no longer provides pre-built binaries for PPC64 (big endian). This platform appears to be largely unused and is only supported on a single manylinux version. PPC64LE (little endian) builds are unaffected.

  Building uv from source is still supported for this platform.
- **Skip generating `activate.csh` for relocatable virtual environments** ([#17759](https://github.com/astral-sh/uv/pull/17759))

  Previously, `uv venv --relocatable` would generate an `activate.csh` script that contained hardcoded paths, making it incompatible with relocation. Now, the `activate.csh` script is not generated for relocatable virtual environments.

  You cannot opt out of this behavior.
- **Require username when multiple credentials match a URL** ([#16983](https://github.com/astral-sh/uv/pull/16983))

  When using `uv auth login` to store credentials, you can register multiple username and password combinations for the same host. Previously, when uv needed to authenticate and multiple credentials matched the URL (e.g., when retrieving a token with `uv auth token`), uv would pick the first match. Now, uv will error instead.

  You cannot opt out of this behavior. Include the username in the request, e.g., `uv auth token --username foo example.com`.
- **Avoid invalidating the lockfile versions after an `exclude-newer` change** ([#17721](https://github.com/astral-sh/uv/pull/17721))

  Previously, changing the `exclude-newer` setting would cause package versions to be upgraded, ignoring the lockfile entirely. Now, uv will only change package versions if they are no longer within the `exclude-newer` range.

  You can restore the previous behavior by using `--upgrade` or `--upgrade-package` to opt-in to package version changes.
- **Upgrade `uv format` to Ruff 0.15.0** ([#17838](https://github.com/astral-sh/uv/pull/17838))

  `uv format` now uses [Ruff 0.15.0](https://github.com/astral-sh/ruff/releases/tag/0.15.0), which uses the [2026 style guide](https://astral.sh/blog/ruff-v0.15.0#the-ruff-2026-style-guide). See the blog post for details.

  The formatting of code is likely to change. You can opt out of this behavior by requesting an older Ruff version, e.g., `uv format --version 0.14.14`.
- **Update uv crate test features to use `test-` as a prefix** ([#17860](https://github.com/astral-sh/uv/pull/17860))

  This change only affects redistributors of uv. The Cargo features used to gate test dependencies, e.g., `pypi`, have been renamed with a `test-` prefix for clarity, e.g., `test-pypi`.

### Stabilizations

- **`uv python upgrade` and `uv python install --upgrade`** ([#17766](https://github.com/astral-sh/uv/pull/17766))

  When installing Python versions, an [intermediary directory](https://docs.astral.sh/uv/concepts/python-versions/#minor-version-directories) without the patch version attached will be created, and virtual environments will be transparently upgraded to new patch versions.

  See the [Python version documentation](https://docs.astral.sh/uv/concepts/python-versions/#upgrading-python-versions) for more details.
- **`uv add --bounds` and the `add-bounds` configuration option** ([#17660](https://github.com/astral-sh/uv/pull/17660))

  This does not come with any behavior changes. You will no longer see an experimental warning when using `uv add --bounds` or `add-bounds` in configuration.
- **`uv workspace list` and `uv workspace dir`** ([#17768](https://github.com/astral-sh/uv/pull/17768))

  This does not come with any behavior changes. You will no longer see an experimental warning when using these commands.
- **`extra-build-dependencies`** ([#17767](https://github.com/astral-sh/uv/pull/17767))

  This does not come with any behavior changes. You will no longer see an experimental warning when using `extra-build-dependencies` in configuration.

### Enhancements

- Improve ABI tag error message phrasing ([#17878](https://github.com/astral-sh/uv/pull/17878))
- Introduce a 10s connect timeout ([#17733](https://github.com/astral-sh/uv/pull/17733))
- Allow using `pyx.dev` as a target in `uv auth` commands despite `PYX_API_URL` differing ([#17856](https://github.com/astral-sh/uv/pull/17856))

### Bug fixes

- Support all CPython ABI tag suffixes properly  ([#17817](https://github.com/astral-sh/uv/pull/17817))
- Add support for detecting PowerShell on Linux and macOS ([#17870](https://github.com/astral-sh/uv/pull/17870))
- Retry timeout errors for streams ([#17875](https://github.com/astral-sh/uv/pull/17875))

## 0.9.x

See [changelogs/0.9.x](./changelogs/0.9.x.md)

## 0.8.x

See [changelogs/0.8.x](./changelogs/0.8.x.md)

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


