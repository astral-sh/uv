# Changelog

<!-- prettier-ignore-start -->


## 0.13.2

Released on 2026-10-19.

### Enhancements

- Fix venv activation command for ksh ([#22538](https://github.com/astral-sh/uv/pull/22538))
- Include installed packages in workspace metadata ([#22395](https://github.com/astral-sh/uv/pull/22395))
- Include supported environments in `pylock.toml` exports ([#22596](https://github.com/astral-sh/uv/pull/22596))
- Preserve wheel script encodings in shell launchers ([#22526](https://github.com/astral-sh/uv/pull/22526))
- Retain diagnostics for script Python discovery failures ([#22549](https://github.com/astral-sh/uv/pull/22549))
- Retry dropped HTTP dispatch tasks ([#22672](https://github.com/astral-sh/uv/pull/22672))
- Skip script path lookup for non-relocatable environments ([#22572](https://github.com/astral-sh/uv/pull/22572))
- Use PEP 508 URLs for build backend requirements ([#22577](https://github.com/astral-sh/uv/pull/22577))

### Preview features

- Fetch index-level variant metadata ([#22511](https://github.com/astral-sh/uv/pull/22511))
- Recognize variant metadata in package indexes ([#22509](https://github.com/astral-sh/uv/pull/22509))

### Performance

- Avoid cloning resolver metadata for virtual package nodes ([#22565](https://github.com/astral-sh/uv/pull/22565))
- Avoid expanding environment-only conflict markers ([#22615](https://github.com/astral-sh/uv/pull/22615))
- Balance Boolean marker chains during parsing ([#22675](https://github.com/astral-sh/uv/pull/22675))
- Cache workspace member names by path ([#22635](https://github.com/astral-sh/uv/pull/22635))
- Extract marker DNF covers from decision diagrams ([#22651](https://github.com/astral-sh/uv/pull/22651))
- Index dependency endpoints within each resolver fork ([#22625](https://github.com/astral-sh/uv/pull/22625))
- Memoize repeated marker traversals ([#22674](https://github.com/astral-sh/uv/pull/22674))
- Resolve extras and check marker reachability without DNF ([#22608](https://github.com/astral-sh/uv/pull/22608))
- Serialize only the CycloneDX fields emitted by uv ([#22431](https://github.com/astral-sh/uv/pull/22431))
- Store marker range edges outside decision nodes ([#22681](https://github.com/astral-sh/uv/pull/22681))
- Upgrade toml_parser to 1.1.5 ([#22679](https://github.com/astral-sh/uv/pull/22679))
- uv-build: remove jiff dependency ([#22539](https://github.com/astral-sh/uv/pull/22539))

### Bug fixes

- Coalesce marker ranges after mapping their children ([#22680](https://github.com/astral-sh/uv/pull/22680))
- Defer publish credential prompts until authentication lookup fails ([#22593](https://github.com/astral-sh/uv/pull/22593))
- Do not panic on conflicts with fixed-priority packages ([#22589](https://github.com/astral-sh/uv/pull/22589))
- Exclude nonmatching files from recursive build globs ([#22581](https://github.com/astral-sh/uv/pull/22581))
- Handle invalid Python version requests without panicking ([#22727](https://github.com/astral-sh/uv/pull/22727))
- Honor `pylock.toml` environment restrictions during installation ([#22594](https://github.com/astral-sh/uv/pull/22594))
- Only reuse locally built wheels when the resolver selects a source distribution ([#22685](https://github.com/astral-sh/uv/pull/22685))
- Prefer retention of `SafeUrl` across internal APIs ([#22601](https://github.com/astral-sh/uv/pull/22601))
- Reject invalid pylock output paths ([#22725](https://github.com/astral-sh/uv/pull/22725))
- Remove `SafeUrl` dereferencing to `url::Url` ([#22622](https://github.com/astral-sh/uv/pull/22622))
- Resolve fresh managed Python installations to absolute paths ([#22547](https://github.com/astral-sh/uv/pull/22547))
- Resolve license globs before packaging ([#22583](https://github.com/astral-sh/uv/pull/22583))
- Retain `SafeUrl` policies when removing credentials ([#22617](https://github.com/astral-sh/uv/pull/22617))
- Show `pip check` incompatibilities in quiet mode ([#22544](https://github.com/astral-sh/uv/pull/22544))
- Store parsed URLs in direct-origin metadata ([#22646](https://github.com/astral-sh/uv/pull/22646))
- Use full URLs for flat-index cache keys ([#22630](https://github.com/astral-sh/uv/pull/22630))

### Rust API

- Gate raw `uv-pep508` URL APIs behind `unsafe-url` ([#22623](https://github.com/astral-sh/uv/pull/22623))

### Documentation

- Clarify documentation and correct stale guidance ([#22542](https://github.com/astral-sh/uv/pull/22542))

### Other changes

- Adopt stable macOS signing identifiers ([#22648](https://github.com/astral-sh/uv/pull/22648))
- Benchmark marker operations ([#22610](https://github.com/astral-sh/uv/pull/22610))
- Build wall-time benchmarks on a larger runner ([#22659](https://github.com/astral-sh/uv/pull/22659))
- Compress benchmark artifacts with zstd ([#22660](https://github.com/astral-sh/uv/pull/22660))
- Encode URL serialization policies in the type system ([#22600](https://github.com/astral-sh/uv/pull/22600))
- Fix overlapping promotion dispatch rules ([#22540](https://github.com/astral-sh/uv/pull/22540))
- Fold identical functions in Linux uv-build wheels ([#22573](https://github.com/astral-sh/uv/pull/22573))
- Increase the Windows ARM trampoline test timeout ([#22566](https://github.com/astral-sh/uv/pull/22566))
- Reduce marker interner storage and share conjunction cache entries ([#22676](https://github.com/astral-sh/uv/pull/22676))
- Restore merge-revision checkout for walltime benchmarks ([#22562](https://github.com/astral-sh/uv/pull/22562))
- Reuse warm Git benchmark caches ([#22657](https://github.com/astral-sh/uv/pull/22657))
- Seed Python builds used by managed discovery tests ([#22654](https://github.com/astral-sh/uv/pull/22654))
- Seed Python variants used by Unix debug tests ([#22652](https://github.com/astral-sh/uv/pull/22652))
- Sync latest Python releases ([#22578](https://github.com/astral-sh/uv/pull/22578))
- Use a builder for Packse wheel fixtures ([#22689](https://github.com/astral-sh/uv/pull/22689))
- Use one codegen unit for benchmark builds ([#22667](https://github.com/astral-sh/uv/pull/22667))

## 0.13.1

Released on 2026-10-14.

### Enhancements

- Ignore `Requires-Python` upper bounds in `uv pip check` and `uv pip install --strict`, consistent with resolution ([#22500](https://github.com/astral-sh/uv/pull/22500))
- Support Linux SPARC by falling back when filesystem cloning and detailed cache accounting are unavailable ([#22463](https://github.com/astral-sh/uv/pull/22463))
- Report missing trusted hashes in resolution errors ([#22241](https://github.com/astral-sh/uv/pull/22241))

### Python

- Add CPython 3.16.0a1 ([#22527](https://github.com/astral-sh/uv/pull/22527))

### Preview features

- Allow `uv tool upgrade` to upgrade installed tools when `tool-install-locks` is enabled ([#22498](https://github.com/astral-sh/uv/pull/22498))
- Add preliminary support for parsing PEP 825 variant labels in wheel filenames with `wheel-variants` ([#22039](https://github.com/astral-sh/uv/pull/22039))

### Performance

- Reduce uv's binary size ([#22429](https://github.com/astral-sh/uv/pull/22429), [#22430](https://github.com/astral-sh/uv/pull/22430), [#22432](https://github.com/astral-sh/uv/pull/22432))
- Speed up Simple API parsing ([#22429](https://github.com/astral-sh/uv/pull/22429))
- Reduce package metadata parsing overhead ([#22239](https://github.com/astral-sh/uv/pull/22239))

### Bug fixes

- Prevent tool and managed Python uninstalls from deleting unrelated files beside their storage directories ([#22495](https://github.com/astral-sh/uv/pull/22495))
- Restore support for source distributions containing hard links or full-length tar user or group names ([#22516](https://github.com/astral-sh/uv/pull/22516), [#22522](https://github.com/astral-sh/uv/pull/22522))
- Preserve device files and FIFOs used as `uv pip compile` or `uv export` output destinations ([#22496](https://github.com/astral-sh/uv/pull/22496))
- Fix `sys._base_executable` detection for environments created from symlinked interpreters ([#22520](https://github.com/astral-sh/uv/pull/22520))
- Prevent concurrent readers from observing partially written lockfiles, Python version files, or managed Python sysconfig files ([#22399](https://github.com/astral-sh/uv/pull/22399), [#22402](https://github.com/astral-sh/uv/pull/22402), [#22403](https://github.com/astral-sh/uv/pull/22403))
- Serialize concurrent project commands that reach the same environment through real and symlinked paths ([#22390](https://github.com/astral-sh/uv/pull/22390))
- Prevent installs, syncs, and environment creation from failing when they race with `uv cache clean` ([#22387](https://github.com/astral-sh/uv/pull/22387))
- Prevent concurrent tool installations from corrupting an environment when the last tool is uninstalled ([#22391](https://github.com/astral-sh/uv/pull/22391))
- Persist changed upgrade settings even when `uv tool upgrade` leaves the tool version unchanged ([#22499](https://github.com/astral-sh/uv/pull/22499))
- Reuse environments with installed Git dependencies pinned to full commit hashes without resolving again ([#22464](https://github.com/astral-sh/uv/pull/22464))

### Documentation

- Explain how to select an emulated Python interpreter on Windows ARM64 when native wheels are unavailable ([#22424](https://github.com/astral-sh/uv/pull/22424))
- Fix the `.netrc` format documentation link ([#22514](https://github.com/astral-sh/uv/pull/22514))

### Other changes

- Publish Python 3.15-based uv Docker images under stable tags ([#22503](https://github.com/astral-sh/uv/pull/22503))

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

