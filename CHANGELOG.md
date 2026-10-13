# Changelog

<!-- prettier-ignore-start -->


## 0.13.0

Released on 2026-10-09.

uv 0.13.0 makes Python 3.15 the default stable Python version. We've also included several breaking changes to improve correctness, performance, and compatibility, described below.

**We expect most users to be able to upgrade without making changes.**

While not a breaking change, this release also updates the format of many of uv's cache entries to improve performance. **uv may download or rebuild dependencies after upgrading**, because some cached entries from earlier versions cannot be reused. Multiple versions of uv can still safely share the same cache directory.

There are no breaking changes to the configuration of the [uv build backend](https://docs.astral.sh/uv/concepts/build-backend/). If your `[build-system]` table includes an upper bound on `uv_build`, update it to allow `uv_build` 0.13, e.g., `uv_build>=0.13.0,<0.14`.

### Breaking changes

- **Use Python 3.15 as the default stable version**

  The default stable Python version has changed from 3.14 to 3.15. This affects Python downloads when no version is requested or pinned, e.g., when running `uv python install`.

  uv continues to use compatible Python installations that are already present. For example, `uv venv` can still use an installed Python 3.14. If no suitable interpreter is installed and automatic downloads are enabled, commands such as `uv venv` and `uvx python` can now download Python 3.15.

  You can opt out of this behavior by requesting Python 3.14 explicitly, e.g., `uv venv --python 3.14`. For projects, use `uv python pin 3.14` to record the version in `.python-version`.

- **Honor `--require-hashes` in included constraints files** ([#22275](https://github.com/astral-sh/uv/pull/22275))

  Previously, uv ignored `--require-hashes` in constraints files included with `-c` from a requirements file. Now, uv honors the directive and requires hashes for all requirements in the installation. Installs that previously succeeded can now fail if a requirement is missing a hash.

  You cannot opt out while the directive is present. Add the missing hashes to your requirements, or remove the `--require-hashes` directive from the included constraints file if hash checking is not intended.

- **Prefer native Python on Windows ARM64** ([#22100](https://github.com/astral-sh/uv/pull/22100))

  Previously, ARM64 builds of uv preferred emulated `x86_64` Python installations because native wheel support was limited. Now, uv prefers native ARM64 (`aarch64`) interpreters across Python versions.

  This follows similar changes in [CPython](https://discuss.python.org/t/python-on-windows-arm64/104524), the official Windows [Python install manager](https://discuss.python.org/t/python-install-manager-26-4/108846), and GitHub's [actions/setup-python](https://github.com/actions/setup-python#supported-architectures).

  When a native interpreter is unavailable, uv continues to fall back to `x86_64`, then 32-bit `x86`.

  You can opt out of this behavior by setting `UV_PYTHON_ARCH=x86_64` or requesting an explicit architecture, e.g., `cpython-3.14-windows-x86_64`. If you are using `setup-uv`, you can set [`python-arch: x86_64`](https://github.com/astral-sh/setup-uv#python-architecture) instead.

- **Reject editable requirements in included constraints files** ([#22282](https://github.com/astral-sh/uv/pull/22282))

  Previously, uv silently ignored editable (`-e`) requirements in constraints files included with `-c` from a requirements file. Now, uv rejects these requirements with an error, matching [pip's behavior](https://pip.pypa.io/en/stable/user_guide/#constraints-files).

  You cannot opt out of this behavior. Move editable requirements to a requirements file passed with `-r`, or pass them directly with `--editable`, instead of including them in a constraints file.

- **Omit the distutils startup patch on Python 3.10 and later** ([#22096](https://github.com/astral-sh/uv/pull/22096))

  Previously, uv installed `_virtualenv.py` and `_virtualenv.pth` into every new virtual environment to prevent distutils configuration from changing installation paths. Now, like [`virtualenv` 21.6.0](https://github.com/pypa/virtualenv/releases/tag/21.6.0), uv omits these files on Python 3.10 and later, which already ignore the affected configuration keys. This reduces Python startup overhead. Python 3.9 and earlier retain the patch.

  You cannot opt out of this behavior. Existing virtual environments are not modified automatically. Recreate an environment with Python 3.10 or later to remove the patch.

  This stabilizes the `no-distutils-patch` preview feature.

- **Treat requirement-file option values as single paths** ([#22290](https://github.com/astral-sh/uv/pull/22290))

  Previously, uv split values passed to `--constraint`, `--override`, `--exclude`, and `--build-constraint` on spaces, even when quoted. Now, each value is treated as a single path, allowing file paths containing spaces.

  You cannot opt out of this behavior. Repeat the option to provide multiple files. For example, replace `-c "a.txt b.txt"` with `-c a.txt -c b.txt`.

  Space-separated lists in `UV_CONSTRAINT`, `UV_OVERRIDE`, `UV_EXCLUDE`, and `UV_BUILD_CONSTRAINT` remain supported.

- **Use `tar-codec` for tar archives by default** ([#22094](https://github.com/astral-sh/uv/pull/22094))

  Previously, uv used `astral-tokio-tar` to extract tar archives, build source distributions with `uv_build`, and read their metadata for `uv publish`. Now, uv uses `tar-codec`, which applies stricter validation when reading archives.

  uv may now reject archives containing hard links or unsupported tar extensions that previous versions accepted. Source distributions created by `uv_build` can also have different archive bytes and hashes.

  You can opt out of this behavior by setting `UV_LEGACY_TAR_BACKEND=1`.

  This stabilizes the `tar-codec` preview feature.

- **Reject `uv build --clear` output directories that contain a build source**
  ([#22276](https://github.com/astral-sh/uv/pull/22276))

  Previously, `uv build --clear` could delete a project or input source distribution when the
  output directory contained the source. Now, uv rejects these output directories, including
  equivalent paths reached through symlinks, before clearing any build output.

  Select an output directory that does not contain any build sources, or omit `--clear`.

### Python

- Add CPython 3.15.0 ([#22400](https://github.com/astral-sh/uv/pull/22400))

### Preview features

- Require hashes for build dependencies, including transitive dependencies, with `--require-build-hashes` ([#21411](https://github.com/astral-sh/uv/pull/21411))

### Performance

- Speed up revalidation of cached HTTP responses by avoiding rewrites of unchanged payloads ([#22130](https://github.com/astral-sh/uv/pull/22130))
- Reduce allocations when reading cached HTTP responses ([#22136](https://github.com/astral-sh/uv/pull/22136))
- Reduce cache storage for HTTP policies and package records ([#22135](https://github.com/astral-sh/uv/pull/22135), [#22133](https://github.com/astral-sh/uv/pull/22133))
- Reduce allocations for cached source distribution revisions ([#22131](https://github.com/astral-sh/uv/pull/22131))

### Bug fixes

- Fix incorrect dependency resolution when reusing source metadata with different build settings ([#22404](https://github.com/astral-sh/uv/pull/22404))
- Avoid overlong wheel cache lock filenames on Windows ([#22134](https://github.com/astral-sh/uv/pull/22134))

## 0.12.x

See [changelogs/0.12.x](./changelogs/0.12.x.md)

## 0.11.x

See [changelogs/0.11.x](./changelogs/0.11.x.md)

## 0.10.x

See [changelogs/0.10.x](./changelogs/0.10.x.md)

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
