# Changelog

<!-- prettier-ignore-start -->


## 0.13.6

Released on 2026-10-26.

### Enhancements

- Accept an empty `UV_RUN_RECURSION_DEPTH` as zero and report invalid values consistently ([#22814](https://github.com/astral-sh/uv/pull/22814))
- Explain how to add project metadata when `uv init` finds a `pyproject.toml` without a `[project]` table ([#22916](https://github.com/astral-sh/uv/pull/22916))

### Python

- Add CPython 3.11.18, 3.12.16, 3.13.18, 3.14.10, and 3.15.2 ([#22938](https://github.com/astral-sh/uv/pull/22938))

### Preview features

- Detect NVIDIA GPU capabilities when selecting compatible wheel variants with `wheel-variants` ([#22690](https://github.com/astral-sh/uv/pull/22690))
- Nest each module's owners in an object in `uv workspace metadata` output ([#22638](https://github.com/astral-sh/uv/pull/22638))
- Report the unsupported properties required by an incompatible wheel variant ([#22719](https://github.com/astral-sh/uv/pull/22719))

### Performance

- Reduce peak memory use when resolving large workspaces ([#22860](https://github.com/astral-sh/uv/pull/22860), [#22863](https://github.com/astral-sh/uv/pull/22863), [#22864](https://github.com/astral-sh/uv/pull/22864), [#22866](https://github.com/astral-sh/uv/pull/22866), [#22870](https://github.com/astral-sh/uv/pull/22870))
- Reduce peak memory use when locking or upgrading large projects ([#22833](https://github.com/astral-sh/uv/pull/22833), [#22834](https://github.com/astral-sh/uv/pull/22834))
- Speed up redirected output from `uv export` and `uv pip compile` ([#22840](https://github.com/astral-sh/uv/pull/22840))
- Reuse cached or active environments when running PEP 723 scripts without dependencies ([#21458](https://github.com/astral-sh/uv/pull/21458))
- Reduce parsing overhead for build dependency and build backend settings ([#22848](https://github.com/astral-sh/uv/pull/22848), [#22871](https://github.com/astral-sh/uv/pull/22871), [#22879](https://github.com/astral-sh/uv/pull/22879), [#22884](https://github.com/astral-sh/uv/pull/22884))
- Reduce parsing overhead for project sources and workspace references ([#22880](https://github.com/astral-sh/uv/pull/22880), [#22883](https://github.com/astral-sh/uv/pull/22883))
- Reduce parsing overhead for cache, exclusion, cutoff, and preview settings ([#22876](https://github.com/astral-sh/uv/pull/22876), [#22877](https://github.com/astral-sh/uv/pull/22877), [#22878](https://github.com/astral-sh/uv/pull/22878), [#22887](https://github.com/astral-sh/uv/pull/22887))
- Reduce PyPI metadata parsing overhead ([#22874](https://github.com/astral-sh/uv/pull/22874), [#22875](https://github.com/astral-sh/uv/pull/22875))
- Speed up parsing tool receipt requirements ([#22885](https://github.com/astral-sh/uv/pull/22885))

### Bug fixes

- Prevent `uv run --active --script` and `uv sync --active --script` from deleting non-virtual-environment directories ([#21464](https://github.com/astral-sh/uv/pull/21464))
- Use `AMD64` for `platform_machine` when resolving for x86-64 Windows ([#22935](https://github.com/astral-sh/uv/pull/22935))
- Preserve free-threaded and other non-default variants in `uv python upgrade` ([#22836](https://github.com/astral-sh/uv/pull/22836))
- Allow `uv init --no-package` and `uv init --script` when `UV_INIT_BUILD_BACKEND` is set ([#22907](https://github.com/astral-sh/uv/pull/22907))
- Prevent panics when rendering containment markers with local versions ([#22815](https://github.com/astral-sh/uv/pull/22815))

### Documentation

- Update the Docker guide and reproduction examples to use uv 0.13.5 ([#22867](https://github.com/astral-sh/uv/pull/22867))

## 0.13.5

Released on 2026-10-21.

Following [uv 0.13.4](https://github.com/astral-sh/uv/releases/tag/0.13.4), which removed the affected eMudhra/emSign root from uv's own certificate store, this release updates the latest managed CPython builds for 3.11 and newer to [python-build-standalone 20261021](https://github.com/astral-sh/python-build-standalone/releases/tag/20261021), including the CPython [3.15.1](https://www.python.org/downloads/release/python-3151/), [3.14.9](https://www.python.org/downloads/release/python-3149/), and [3.13.17](https://www.python.org/downloads/release/python-31317/) security patch releases. These builds include [pip 26.2.2](https://pip.pypa.io/en/stable/news/#v26-2-2) both preinstalled in the managed interpreter and bundled with `ensurepip`, which installs pip by default when you run `python -m venv`. Both copies use certifi 2026.10.19, which removes the same affected root. After upgrading uv, run `uv python upgrade` to install the updated builds. Virtual environments pinned to an older Python patch version do not switch to the updated interpreter automatically. Upgrade pip separately in existing virtual environments.

Please note upstream CPython 3.9 and 3.10 still bundle pip releases whose certificate bundles trust the affected eMudhra/emSign root. We recommend migrating off of those versions and will no longer be releasing Docker images with uv installed for EOL Python versions.

### Enhancements

- Report incompatible self-dependencies directly in resolution errors ([#21845](https://github.com/astral-sh/uv/pull/21845))

### Python

- Add CPython 3.15.1, 3.14.9, and 3.13.17 ([#22802](https://github.com/astral-sh/uv/pull/22802))

### Preview features

- Select and rank wheel variants with no required properties or properties from PEP 850 static providers in platform-specific resolutions with `wheel-variants` ([#22535](https://github.com/astral-sh/uv/pull/22535), [#22536](https://github.com/astral-sh/uv/pull/22536), [#22687](https://github.com/astral-sh/uv/pull/22687), [#22688](https://github.com/astral-sh/uv/pull/22688))

### Performance

- Reduce peak memory use when resolving dependencies across multiple environments ([#22769](https://github.com/astral-sh/uv/pull/22769), [#22770](https://github.com/astral-sh/uv/pull/22770), [#22784](https://github.com/astral-sh/uv/pull/22784))
- Speed up exports with complex transitive extras by resolving activation across the dependency graph ([#22751](https://github.com/astral-sh/uv/pull/22751))
- Reduce uv's binary size by keeping additional error formatters out of line ([#22775](https://github.com/astral-sh/uv/pull/22775))
- Reduce overhead when parsing custom build cache keys ([#22749](https://github.com/astral-sh/uv/pull/22749))

### Bug fixes

- Lock packages distributed only as Android wheels without a source distribution ([#22806](https://github.com/astral-sh/uv/pull/22806))
- Export optional dependencies correctly when transitive extras are activated through different or incompatible environment paths ([#22751](https://github.com/astral-sh/uv/pull/22751))
- Reject invalid custom build cache keys instead of silently falling back to the defaults ([#22749](https://github.com/astral-sh/uv/pull/22749))

### Security

- Reject certificate chains containing compromised eMudhra intermediate keys via `rustls-webpki` 0.103.16 ([#22777](https://github.com/astral-sh/uv/pull/22777))
- Stop publishing uv Docker images based on Python 3.9 and 3.10 images that no longer receive upstream updates ([#22773](https://github.com/astral-sh/uv/pull/22773))

## 0.13.4

Released on 2026-10-20.

### Preview features

- Validate PEP 825 variant metadata and treat package versions with invalid metadata as unavailable under `wheel-variants` ([#22512](https://github.com/astral-sh/uv/pull/22512))

### Other changes

- Update the bundled Mozilla root certificates via `webpki-root-certs` 1.0.10 ([#22760](https://github.com/astral-sh/uv/pull/22760))

## 0.13.3

Released on 2026-10-20.

### Performance

- Avoid severe `uv export` slowdowns in dependency graphs with extras ([#22750](https://github.com/astral-sh/uv/pull/22750))

## 0.13.2

Released on 2026-10-19.

### Enhancements

- Report all installed packages in `uv workspace metadata`, including packages outside the selected resolution ([#22395](https://github.com/astral-sh/uv/pull/22395))
- Include supported environment restrictions when exporting `pylock.toml` ([#22596](https://github.com/astral-sh/uv/pull/22596))
- Retry dropped GET and HEAD requests when fetching package and index data ([#22672](https://github.com/astral-sh/uv/pull/22672))
- Preserve encoding declarations in wheel data scripts that require shell launchers ([#22526](https://github.com/astral-sh/uv/pull/22526))
- Show causes and hints when `uv python find --script` cannot discover an interpreter, including with `-q` ([#22549](https://github.com/astral-sh/uv/pull/22549))
- Print valid ksh activation instructions and support non-relocatable environments under OpenBSD-derived ksh ([#22538](https://github.com/astral-sh/uv/pull/22538), [#22572](https://github.com/astral-sh/uv/pull/22572))
- Allow PEP 508 direct URLs, including Mercurial and extensionless archive URLs, in uv-build's `[build-system].requires` validation ([#22577](https://github.com/astral-sh/uv/pull/22577))

### Python

- Add PyPy 3.11.17 and 3.12.15 ([#22578](https://github.com/astral-sh/uv/pull/22578))

### Preview features

- Fetch and parse PEP 825 index-level variant metadata with `wheel-variants` ([#22509](https://github.com/astral-sh/uv/pull/22509), [#22511](https://github.com/astral-sh/uv/pull/22511))

### Performance

- Avoid exponential work when simplifying, traversing, and exporting complex dependency markers ([#22608](https://github.com/astral-sh/uv/pull/22608), [#22615](https://github.com/astral-sh/uv/pull/22615), [#22651](https://github.com/astral-sh/uv/pull/22651), [#22674](https://github.com/astral-sh/uv/pull/22674))
- Reduce the time and memory used to parse and store complex dependency markers ([#22675](https://github.com/astral-sh/uv/pull/22675), [#22676](https://github.com/astral-sh/uv/pull/22676), [#22681](https://github.com/astral-sh/uv/pull/22681))
- Reduce resolver memory use for large dependency graphs, forks, extras, and dependency groups ([#22565](https://github.com/astral-sh/uv/pull/22565), [#22625](https://github.com/astral-sh/uv/pull/22625))
- Speed up project discovery in large workspaces by caching member names ([#22635](https://github.com/astral-sh/uv/pull/22635))
- Reduce uv's binary size and speed up CycloneDX exports ([#22431](https://github.com/astral-sh/uv/pull/22431))
- Reduce `uv-build` binary size, including Linux x86_64 wheels ([#22539](https://github.com/astral-sh/uv/pull/22539), [#22573](https://github.com/astral-sh/uv/pull/22573))
- Reduce allocations when parsing TOML ([#22679](https://github.com/astral-sh/uv/pull/22679))

### Bug fixes

- Respect `--exclude-newer` when a newer wheel for the selected source distribution is already cached ([#22685](https://github.com/astral-sh/uv/pull/22685))
- Prevent URL credentials from leaking through diagnostics or persisted metadata ([#22600](https://github.com/astral-sh/uv/pull/22600), [#22601](https://github.com/astral-sh/uv/pull/22601), [#22617](https://github.com/astral-sh/uv/pull/22617), [#22622](https://github.com/astral-sh/uv/pull/22622), [#22646](https://github.com/astral-sh/uv/pull/22646))
- Keep separate cache entries for signed `--find-links` URLs that differ only in sensitive query values ([#22630](https://github.com/astral-sh/uv/pull/22630))
- Honor PEP 751 environment restrictions when installing or syncing from `pylock.toml` ([#22594](https://github.com/astral-sh/uv/pull/22594))
- Use configured credentials before prompting during `uv publish`, and avoid prompting when no upload occurs ([#22593](https://github.com/astral-sh/uv/pull/22593))
- Match recursive build globs against complete file paths and ignore invalid license symlinks ([#22581](https://github.com/astral-sh/uv/pull/22581), [#22583](https://github.com/astral-sh/uv/pull/22583))
- Return an error for invalid `uv pip compile --python-version` values instead of panicking ([#22727](https://github.com/astral-sh/uv/pull/22727))
- Reject invalid `pylock.toml` output filenames before resolution instead of panicking ([#22725](https://github.com/astral-sh/uv/pull/22725))
- Avoid panicking during repeated resolution conflicts involving fixed-priority packages ([#22589](https://github.com/astral-sh/uv/pull/22589))
- Create version aliases correctly when installing managed Python into a relative directory ([#22547](https://github.com/astral-sh/uv/pull/22547))
- Show incompatibilities from failing `uv pip check` invocations with `-q` ([#22544](https://github.com/astral-sh/uv/pull/22544))
- Treat logically equivalent dependency markers as equal regardless of construction order ([#22680](https://github.com/astral-sh/uv/pull/22680))

### Rust API

- Require the opt-in `unsafe-url` feature to use raw `url::Url` APIs in `uv-pep508` ([#22623](https://github.com/astral-sh/uv/pull/22623))

### Documentation

- Clarify credential lookup, environment synchronization, managed Python, and integration guidance ([#22542](https://github.com/astral-sh/uv/pull/22542))

### Other changes

- Use stable `sh.astral.*` signing identifiers for macOS binaries ([#22648](https://github.com/astral-sh/uv/pull/22648))

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


