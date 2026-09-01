# Changelog

<!-- prettier-ignore-start -->


## 0.12.9

Released on 2026-09-01.

### Python

- Add CPython 3.15.0rc2 ([#21413](https://github.com/astral-sh/uv/pull/21413), [#21415](https://github.com/astral-sh/uv/pull/21415))

### Enhancements

- Add `--no-locked` and `--no-frozen` to disable lock modes enabled by `UV_LOCKED` and `UV_FROZEN` for a single invocation ([#21408](https://github.com/astral-sh/uv/pull/21408))
- Report the exact command-line lock-mode flag in warnings and errors ([#21402](https://github.com/astral-sh/uv/pull/21402))

### Performance

- Speed up cold wheel installs by extracting each streaming ZIP archive in a single blocking task and reusing buffers across files ([#21372](https://github.com/astral-sh/uv/pull/21372))

### Bug fixes

- Update `async_http_range_reader` to 0.11.1 to address a potential memory-safety issue when reading metadata ranges from untrusted wheels ([#21401](https://github.com/astral-sh/uv/pull/21401))
- Remove sensitive headers when redirects cross authentication realms, including same-host redirects that change URL schemes ([#21382](https://github.com/astral-sh/uv/pull/21382))
- Redact secrets in signed URLs from retry diagnostics, including nested request errors ([#21381](https://github.com/astral-sh/uv/pull/21381))
- Give `--locked`, `--frozen`, `--check`, and `--check-exists` precedence over conflicting `UV_LOCKED` and `UV_FROZEN` values ([#21396](https://github.com/astral-sh/uv/pull/21396))
- Prevent concurrent uv processes from redundantly extracting the same local or source-built wheel ([#21400](https://github.com/astral-sh/uv/pull/21400))

## 0.12.8

Released on 2026-08-31.

### Enhancements

- Warn about invalid tool directories and continue upgrading valid tools with `uv tool upgrade --all` ([#21368](https://github.com/astral-sh/uv/pull/21368))

### Preview features

- Deduplicate identical files within and across cached wheels with the `content-addressed-cache` preview feature ([#21327](https://github.com/astral-sh/uv/pull/21327))
- Reduce allocations while extracting content-addressed wheels by reusing the hashing buffer across files ([#21340](https://github.com/astral-sh/uv/pull/21340))
- Speed up content-addressed cache cleanup on macOS by reading hard-link counts in bulk ([#21344](https://github.com/astral-sh/uv/pull/21344))

### Performance

- Prevent concurrent uv processes from downloading and extracting the same remote wheel more than once ([#21379](https://github.com/astral-sh/uv/pull/21379))
- Speed up dependency graph construction from large lockfiles by indexing packages during traversal ([#21373](https://github.com/astral-sh/uv/pull/21373))
- Extend indexed lockfile traversal to exports, dependency trees, audits, and freshness checks ([#21377](https://github.com/astral-sh/uv/pull/21377))
- Speed up warm resolutions by reducing repeated marker interner work ([#21300](https://github.com/astral-sh/uv/pull/21300))

### Bug fixes

- Do not trust hashes from direct URLs discovered only in wheel metadata when installing with `--require-hashes` ([#21348](https://github.com/astral-sh/uv/pull/21348))
- Use a compatible Azure Storage API version for anonymous and authenticated requests, allowing credential retries when public access is disabled ([#21366](https://github.com/astral-sh/uv/pull/21366))
- Redact Azure shared access signature (`sig`) query parameters from displayed URLs ([#21360](https://github.com/astral-sh/uv/pull/21360))
- Treat projects below one-level workspace member globs as standalone instead of aborting workspace discovery ([#21341](https://github.com/astral-sh/uv/pull/21341))

### Other changes

- Update `astral-tokio-tar` to 0.7.0 and use effective sizes when tracking extracted hard links ([#21346](https://github.com/astral-sh/uv/pull/21346))

## 0.12.7

Released on 2026-08-27.

### Python

- Replace managed Python installations when upgrading to a newer build of the same version ([#21323](https://github.com/astral-sh/uv/pull/21323))

### Enhancements

- Support Linux `s390x`, `ppc64le`, and `loongarch64` targets for cross-platform dependency resolution ([#21313](https://github.com/astral-sh/uv/pull/21313))
- Retry downloads with configured credentials when Azure Storage denies anonymous access to an endpoint configured via `UV_AZURE_ENDPOINT_URL` ([#21318](https://github.com/astral-sh/uv/pull/21318))

### Preview features

- Use content-based directory hashes to deduplicate extracted wheels in the cache with the `content-addressed-cache` preview feature ([#19693](https://github.com/astral-sh/uv/pull/19693))

### Bug fixes

- Reject source archives with hash mismatches before persisting their extracted contents to the cache ([#21248](https://github.com/astral-sh/uv/pull/21248))

### Other changes

- remove pyx specific features ([#21182](https://github.com/astral-sh/uv/pull/21182), [#21183](https://github.com/astral-sh/uv/pull/21183), [#21184](https://github.com/astral-sh/uv/pull/21184), [#21185](https://github.com/astral-sh/uv/pull/21185), [#21186](https://github.com/astral-sh/uv/pull/21186))

## 0.12.6

Released on 2026-08-25.

### Python

- Update CPython to use OpenSSL 3.5.8 and libffi 3.4.8 [#21295](https://github.com/astral-sh/uv/pull/21295))
### Enhancements

- Report cache-cleaning space savings from filesystem block allocation and avoid double-counting hard links ([#21261](https://github.com/astral-sh/uv/pull/21261))
- Limit warnings about unbounded `uv_build` requirements to source-distribution builds ([#21078](https://github.com/astral-sh/uv/pull/21078))
- Display byte counts below 1 KiB without a fractional part ([#21237](https://github.com/astral-sh/uv/pull/21237))

### Preview features

- Add `uv workspace metadata --sync --exact` to remove packages outside the selected resolution ([#21117](https://github.com/astral-sh/uv/pull/21117))
- Add the `artifact-hash-filtering` preview feature to make `uv pip compile --generate-hashes` honor `--only-binary` and `--no-binary` ([#21235](https://github.com/astral-sh/uv/pull/21235))
- Respect package-specific `exclude-newer` cutoffs when `uv check` selects its `ty` executable ([#21227](https://github.com/astral-sh/uv/pull/21227))
- Preserve virtual-environment hints from `tar-codec` source-distribution errors when the base interpreter is outside a `bin` directory ([#21146](https://github.com/astral-sh/uv/pull/21146))

### Performance

- Enable profile-guided optimization for Linux x86-64 release binaries ([#21001](https://github.com/astral-sh/uv/pull/21001))
- Enable profile-guided optimization for Windows x86-64 release binaries ([#21003](https://github.com/astral-sh/uv/pull/21003))
- Enable profile-guided optimization for macOS ARM64 release binaries ([#21002](https://github.com/astral-sh/uv/pull/21002))
- Enable profile-guided optimization for Linux ARM64 release binaries ([#21004](https://github.com/astral-sh/uv/pull/21004))
- Speed up syncing projects with many activated conflict items by reusing their encoded representation ([#21148](https://github.com/astral-sh/uv/pull/21148))

### Bug fixes

- Allow explicit `uv build` and non-editable first-party workspace packages when `no-build` is enabled ([#21294](https://github.com/astral-sh/uv/pull/21294))
- Reuse configured index credentials during `uv tool upgrade` when the tool receipt references the same index ([#21275](https://github.com/astral-sh/uv/pull/21275))
- Ensure full 40-character Git commit pins resolve to the requested object instead of a SHA-named branch ([#21224](https://github.com/astral-sh/uv/pull/21224))
- Prevent TLS segfaults in riscv64 musl release binaries ([#21158](https://github.com/astral-sh/uv/pull/21158))
- Preserve dependencies selected by recursive extras when markers mix production and extra conditions ([#21181](https://github.com/astral-sh/uv/pull/21181))
- Preserve version constraints from transitively referenced recursive extras ([#21209](https://github.com/astral-sh/uv/pull/21209))
- Resolve repository-relative Git archive dependencies inside the checkout during the initial `uv sync` ([#21264](https://github.com/astral-sh/uv/pull/21264))
- Return an error instead of panicking when a bearer token cannot be encoded as an HTTP header ([#21282](https://github.com/astral-sh/uv/pull/21282))
- Do not misclassify package URLs ending in `.py` as local script paths ([#21144](https://github.com/astral-sh/uv/pull/21144))
- Use directory creation times consistently across libc implementations for directory `cache-keys` entries ([#21137](https://github.com/astral-sh/uv/pull/21137))
- Promote human-readable sizes to the next unit at rounding boundaries ([#21136](https://github.com/astral-sh/uv/pull/21136))

### Other changes

- Add Python 3.15 release-candidate Docker images ([#21293](https://github.com/astral-sh/uv/pull/21293))
- Raise the minimum supported Rust version to 1.96 and update the repository toolchain to Rust 1.98 ([#21258](https://github.com/astral-sh/uv/pull/21258))

## 0.12.5

Released on 2026-08-14.

### Python

- Add CPython 3.10.21, 3.11.16, and 3.12.14 ([#21138](https://github.com/astral-sh/uv/pull/21138))
- Prefer newer versions and standard variants when selecting between equally prioritized Python interpreters ([#21134](https://github.com/astral-sh/uv/pull/21134))

### Enhancements

- Simplify errors and hints for invalid editable requirements, and redact credentials in requirement URLs ([#21130](https://github.com/astral-sh/uv/pull/21130))

### Preview features

- Allow `--index` and `--default-index` to select configured package indexes by name with the `index-by-name` preview feature ([#17455](https://github.com/astral-sh/uv/pull/17455))
- Include distribution artifact URLs and hashes in CycloneDX SBOM exports by default ([#21131](https://github.com/astral-sh/uv/pull/21131))
- Fall back to logical file sizes when using `cache-physical-space` on filesystems that do not support physical-space accounting ([#21133](https://github.com/astral-sh/uv/pull/21133))

### Bug fixes

- Resolve relative package index paths in PEP 723 scripts against the script directory ([#21097](https://github.com/astral-sh/uv/pull/21097))

## 0.12.4

Released on 2026-08-13.

### Enhancements

- Prefer post-quantum key exchange and enable opt-in TLS diagnostics ([#21054](https://github.com/astral-sh/uv/pull/21054))
- Accept whitespace before versions in noncompliant wildcard comparisons such as `Requires-Python: >= 3.5.*` ([#21012](https://github.com/astral-sh/uv/pull/21012))
- Report a specific error when a PEP 723 closing tag contains trailing whitespace or other content ([#20944](https://github.com/astral-sh/uv/pull/20944))
- Omit source-span carets from diagnostics for empty PEP 508 requirements ([#21094](https://github.com/astral-sh/uv/pull/21094))

### Preview features

- Add `uv check --no-install-project` and respect `UV_NO_INSTALL_PROJECT` to install dependencies without building or installing the project ([#21085](https://github.com/astral-sh/uv/pull/21085))
- Make the ty subprocess invoked by `uv check` honor uv's color and progress settings, including quiet mode ([#21086](https://github.com/astral-sh/uv/pull/21086))

### Performance

- Speed up resolutions with long runs of unavailable package versions by coalescing gaps in the resolver's version ranges ([#20804](https://github.com/astral-sh/uv/pull/20804))
- Speed up Simple API parsing by deserializing PyPI and Pyx file metadata directly ([#21041](https://github.com/astral-sh/uv/pull/21041))

### Bug fixes

- Use windowed `pythonw.exe` launchers for virtual environments created from managed Python minor-version links ([#19235](https://github.com/astral-sh/uv/pull/19235))
- Allow `uv lock` to proceed when `.venv` is an unusable project environment ([#21068](https://github.com/astral-sh/uv/pull/21068))
- Respect `fork-strategy` when ordering forks created from `environments` or existing lockfile `resolution-markers` ([#21000](https://github.com/astral-sh/uv/pull/21000))
- Preserve consecutive wildcard Python minor-version exclusions such as `!=3.11.*, !=3.12.*` in `uv.lock` ([#21045](https://github.com/astral-sh/uv/pull/21045))
- Preserve inline comments on the final item in dependency arrays when `uv add` updates it ([#21008](https://github.com/astral-sh/uv/pull/21008))
- Recover from stale base-interpreter cache metadata when an existing virtual environment exposes a version mismatch ([#21073](https://github.com/astral-sh/uv/pull/21073))
- Prevent interpreter cache reuse across different `PYTHONEXECUTABLE` and `__PYVENV_LAUNCHER__` overrides ([#21075](https://github.com/astral-sh/uv/pull/21075))
- Show standard styling, usage guidance, and line termination for invalid `uv version --bump` values ([#21076](https://github.com/astral-sh/uv/pull/21076))

## 0.12.3

Released on 2026-08-07.

### Python

- Add CPython 3.13.15 ([#20997](https://github.com/astral-sh/uv/pull/20997))

### Preview features

- Add `--output-format` to select automatic, human-readable, or raw-byte output for `uv cache size` ([#20992](https://github.com/astral-sh/uv/pull/20992))
- Preserve JSON output from `uv workspace metadata --quiet` while suppressing diagnostics ([#20991](https://github.com/astral-sh/uv/pull/20991))
- Reduce memory usage for large workspaces by streaming `uv workspace metadata` JSON output ([#20990](https://github.com/astral-sh/uv/pull/20990))

### Performance

- Reduce Linux startup latency by initializing the workspace cache before spawning another thread ([#20989](https://github.com/astral-sh/uv/pull/20989))
- Reuse compiled workspace exclusion patterns during workspace discovery ([#20988](https://github.com/astral-sh/uv/pull/20988))
- Speed up conflict-heavy resolutions by avoiding materialized range complements ([#20982](https://github.com/astral-sh/uv/pull/20982))
- Avoid slow procfs reads during Python interpreter discovery on Linux ([#20987](https://github.com/astral-sh/uv/pull/20987))

### Documentation

- Add PEP 740 attestations to the GitHub Actions publishing example ([#20986](https://github.com/astral-sh/uv/pull/20986))
- Restrict the GitHub Actions publishing example to Python version tags ([#20973](https://github.com/astral-sh/uv/pull/20973))
- Correct `--python-pin` to `--pin-python` in the `uv init --bare` example ([#20876](https://github.com/astral-sh/uv/pull/20876))

## 0.12.2

Released on 2026-08-05.

### Python

- Add CPython 3.15.0rc1 ([#20948](https://github.com/astral-sh/uv/pull/20948))
- Add CPython 3.14.7 ([#20971](https://github.com/astral-sh/uv/pull/20971))

### Enhancements

- Ensure diagnostic hints end with a newline to prevent malformed terminal output ([#20959](https://github.com/astral-sh/uv/pull/20959))

### Preview features

- Audit one or all installed tools with `uv tool audit` ([#20921](https://github.com/astral-sh/uv/pull/20921))
- Report physically reclaimed disk space during cache cleanup with the `cache-physical-space` preview feature ([#20925](https://github.com/astral-sh/uv/pull/20925))

### Configuration

- Add `UV_RUN_RLIMIT_NOFILE` to set the open-file limit for commands launched by `uv run` ([#20926](https://github.com/astral-sh/uv/pull/20926))

### Performance

- Speed up `uv.lock` parsing for wheel entries ([#20881](https://github.com/astral-sh/uv/pull/20881))
- Speed up `uv.lock` parsing for source distribution entries ([#20882](https://github.com/astral-sh/uv/pull/20882))
- Speed up filename extraction from distribution URLs ([#20879](https://github.com/astral-sh/uv/pull/20879))
- Reduce filesystem metadata lookups during bytecode compilation ([#20928](https://github.com/astral-sh/uv/pull/20928))
- Reuse file metadata when building source distributions ([#20927](https://github.com/astral-sh/uv/pull/20927))

### Bug fixes

- Preserve compatibility with older uv versions when recording artifact sizes in cached wheels and source distributions ([#20963](https://github.com/astral-sh/uv/pull/20963))
- Avoid including workspace-root default dependency groups when syncing or exporting a selected workspace member unless explicitly requested ([#20930](https://github.com/astral-sh/uv/pull/20930))

### Documentation

- Separate build and publish jobs in the GitHub Actions publishing guide ([#20946](https://github.com/astral-sh/uv/pull/20946))
- Ensure the GitHub Actions publishing example waits for the build job to finish ([#20957](https://github.com/astral-sh/uv/pull/20957))
- Correct typos in the Docker integration guide ([#20970](https://github.com/astral-sh/uv/pull/20970))

## 0.12.1

Released on 2026-07-31.

### Enhancements

- Add package-specific pre-release policies with `--prerelease-package` ([#20837](https://github.com/astral-sh/uv/pull/20837))
- Support local HTML files as flat indexes ([#20802](https://github.com/astral-sh/uv/pull/20802))
- Add Xonsh virtual environment activation scripts (`activate.xsh`) ([#19740](https://github.com/astral-sh/uv/pull/19740))
- Preserve filesystem paths passed to `uv add --index` when updating `pyproject.toml` ([#20817](https://github.com/astral-sh/uv/pull/20817))

### Preview features

- Add automatic fixes to `uv check` with `--fix` ([#20793](https://github.com/astral-sh/uv/pull/20793))
- Avoid rejecting unchanged metadata-free lockfiles when workspace dependencies share direct sources ([#20847](https://github.com/astral-sh/uv/pull/20847))
- Honor direct URL constraints when validating metadata-free lockfiles ([#20796](https://github.com/astral-sh/uv/pull/20796))
- Ignore malformed PEP 723 scripts discovered during project checks ([#20784](https://github.com/astral-sh/uv/pull/20784))
- Use ty's native script exclusion in `uv check` ([#20742](https://github.com/astral-sh/uv/pull/20742))

### Performance

- Parse canonical uv lockfiles directly, with a fallback for other valid TOML syntax ([#20648](https://github.com/astral-sh/uv/pull/20648))
- Accelerate SHA-256 hashing on non-Windows ARM64 platforms ([#20805](https://github.com/astral-sh/uv/pull/20805))

### Bug fixes

- Flush shell startup file updates before `uv tool update-shell` and `uv python update-shell` exit ([#20842](https://github.com/astral-sh/uv/pull/20842))
- Make workspace-root dependency groups available to commands run from workspace members ([#20840](https://github.com/astral-sh/uv/pull/20840))
- Resolve `--find-links` paths in requirements files relative to the containing file ([#20832](https://github.com/astral-sh/uv/pull/20832))
- Respect configured indexes in `uv tool list --outdated` ([#20770](https://github.com/astral-sh/uv/pull/20770))

### Documentation

- Document Astral GPU indexes in the PyTorch guide ([#20785](https://github.com/astral-sh/uv/pull/20785))
- Use consistent dependency-group argument descriptions throughout the CLI documentation ([#20823](https://github.com/astral-sh/uv/pull/20823))

## 0.12.0

Released on 2026-07-28.

Since we released uv [0.11.0](https://github.com/astral-sh/uv/releases/tag/0.11.0) in March, we've accumulated changes that improve correctness, safety, and compatibility with specifications, but could break some workflows. This release contains those changes; many have been marked as breaking out of an abundance of caution.

**We expect most users to be able to upgrade without making changes.**

There are no breaking changes to the configuration of the [uv build backend](https://docs.astral.sh/uv/concepts/build-backend/). If your `[build-system]` table includes an upper bound on `uv_build`, update it to allow `uv_build` 0.12, e.g., `uv_build>=0.11.32,<0.13`.

### Breaking changes

- **Define build systems by default with `uv init`** ([#19197](https://github.com/astral-sh/uv/pull/19197))

  Projects created with `uv init` now declare a build system and are packaged by default. This was the default project layout all the way back in [v0.3](https://github.com/astral-sh/uv/releases/tag/0.3.0), but we found that the use of the `hatchling` build system was confusing to newcomers and consequently dropped use of a build system by default in [v0.4](https://github.com/astral-sh/uv/releases/tag/0.4.0). Since then, we've created our own build system (`uv_build`) with tight integration with uv and are excited to restore the default to a best-practice project layout.

  Previously, `uv init example` created an unpackaged layout containing `main.py` and a `pyproject.toml` without a build system. The project could declare dependencies but was not itself installed into its virtual environment.

  Now, `uv init example` defines a `[build-system]` using `uv_build`, places application source code in `src/example`, and includes a `[project.scripts]` entry named `example`. Defining a build system allows the project to be imported from tests or other code, installed as a dependency, and run as a command:

  ```console
  $ uv init example
  $ cd example
  $ uv run example
  Hello from example!
  ```

  Existing projects are unaffected. Use [`uv init --no-package example`](https://docs.astral.sh/uv/concepts/projects/init/#creating-a-project-without-a-build-system) to create the previous unpackaged layout without a build system.

  See the [project creation documentation](https://docs.astral.sh/uv/concepts/projects/init/#applications) for more details.

  This stabilizes the `packaged-init` preview feature.
- **Reject unsupported source distribution and wheel archive formats** ([#18927](https://github.com/astral-sh/uv/pull/18927))

  [PEP 625](https://peps.python.org/pep-0625/) requires [source distributions](https://docs.astral.sh/uv/concepts/resolution/#source-distribution) to use `.tar.gz` archives. Previously, uv also accepted legacy formats such as `.tar.bz2` and `.tar.xz`. Those formats are now rejected, including when referenced by an existing lockfile. Legacy `.zip` source distributions remain supported for backwards compatibility.

  Wheels and other ZIP archives can no longer contain entries compressed with bzip2, LZMA, or XZ. Entries must use the stored, DEFLATE, or zstd compression methods.

  Removing support for uncommon compression methods reduces uv's compression dependencies and the attack surface exposed when processing untrusted packages.

  You cannot opt out of this behavior. If you depend on a legacy source distribution that uses an unsupported format, we recommend rebuilding it as a `.tar.gz` archive and regenerating any lockfile containing references to the legacy archive.
- **Reject wheel files that could replace the Python interpreter** ([#20748](https://github.com/astral-sh/uv/pull/20748), [#20749](https://github.com/astral-sh/uv/pull/20749))

  uv already rejected wheel entry points named `python`, but case variants such as `Python` were still accepted. On case-insensitive filesystems, including common macOS and Windows setups, these entry points could overwrite the virtual environment's interpreter.

  Wheels could also place interpreter files in their `.data/scripts` directory or in paths such as `.data/data/bin/python`, bypassing the entry-point check and replacing the interpreter during installation.

  uv now rejects case-insensitive variants of reserved interpreter names and wheel data files that would be installed over an interpreter. This includes names such as `Python`, `python.py`, and `Python.exe`, along with other reserved interpreter names and their versioned variants.

  You cannot opt out of these checks. Rename conflicting entry points or wheel data files and rebuild the affected wheel.
- **Prefer stable releases before falling back to pre-releases** ([#19993](https://github.com/astral-sh/uv/pull/19993))

  A dependency can introduce a [pre-release requirement](https://docs.astral.sh/uv/concepts/resolution/#pre-release-handling) after resolution starts. uv previously required each package's pre-release eligibility to be known before resolution began: the default `if-necessary-or-explicit` mode allowed them for direct requirements that explicitly requested a pre-release, or for packages that only published pre-releases.

  This meant that a pre-release requirement discovered in a dependency's metadata, e.g., `example>=2.0.0b1`, would fail to resolve even when a compatible pre-release existed. To resolve it, you had to add that dependency as a direct requirement or allow pre-releases across your entire dependency graph.

  The default mode is now `if-necessary`. uv tries stable candidates first and falls back to pre-releases when no stable candidate satisfies the active constraints. Like pip, uv now supports [pre-release requirements discovered transitively](https://docs.astral.sh/uv/pip/compatibility/#pre-release-compatibility), but can select different versions than previous uv releases when both stable and pre-release candidates are available.

  You can opt out of automatic pre-release selection with `--prerelease disallow`. Alternatively, `--prerelease allow` considers pre-releases without first preferring stable releases, and `--prerelease explicit` only allows them for direct requirements that mention a pre-release.

  The old `if-necessary-or-explicit` mode distinguished between explicitly requested pre-releases and packages with no stable releases. That distinction is unnecessary now that `if-necessary` handles both cases, including transitive requirements. The old name remains available as an alias but is deprecated and will be removed in a future release.
- **Respect `--require-hashes` directives in `requirements.txt`** ([#19336](https://github.com/astral-sh/uv/pull/19336))

  Previously, `uv pip install` and `uv pip sync` warned about `--require-hashes` inside a `requirements.txt` file but still installed dependencies without checking their hashes. Now, the directive enables hash-checking mode, just as if `--require-hashes` had been passed on the command line.

  For example, this requirements file is no longer accepted because the requirement is neither pinned nor hashed:

  ```text
  --require-hashes
  anyio
  ```

  You cannot opt out while the directive is present. Pin every requirement with `==` and provide its hash, or remove `--require-hashes` if hash checking is not intended.
- **Reject MD5-only hashes in hash-checking mode** ([#20758](https://github.com/astral-sh/uv/pull/20758))

  Previously, `uv pip install --require-hashes` and `uv pip sync --require-hashes` accepted requirements whose only available digest used MD5. MD5 is not collision-resistant, so relying on it undermined installations that require hash verification and differed from pip's behavior.

  Hash-checking mode now requires at least one secure digest for every requirement. For example, the following requirement is rejected unless a secure hash, such as SHA-256, is also supplied:

  ```text
  anyio==4.0.0 --hash=md5:420d85e19168705cdf0223621b18831a
  ```

  A secure hash can be supplied directly on the requirement or in a matching constraints file. Ordinary hash verification without `--require-hashes` continues to support MD5.

  You cannot opt out while hash checking is required. Regenerate affected hashes with SHA-256 or another supported secure hash.
- **Reject invalid `pylock.toml` files and artifacts** ([#20402](https://github.com/astral-sh/uv/pull/20402), [#20440](https://github.com/astral-sh/uv/pull/20440), [#20443](https://github.com/astral-sh/uv/pull/20443))

  uv now validates additional requirements from the [`pylock.toml` specification](https://packaging.python.org/en/latest/specifications/pylock-toml/):

  - The `packages` array must be present. Previously, uv interpreted a missing array as an empty lockfile, so `uv pip sync` could uninstall an environment instead of rejecting malformed input. An explicitly empty `packages = []` array remains valid.
  - Lockfile filenames must be `pylock.toml` or a single-name variant such as `pylock.dev.toml`. Names such as `pylock..toml` and `pylock.foo.bar.toml` are rejected.
  - If a wheel, source distribution, or other artifact declares a `size`, the downloaded or cached artifact must match. Previously, an incorrect size was accepted when the hash was correct. Sizes reported by package indexes remain advisory.

  You cannot opt out of these checks. Regenerate malformed lockfiles, rename invalid filenames, and either correct or remove an incorrect optional `size` value.
- **Honor explicit certificate overrides even when no certificates can be loaded** ([#20741](https://github.com/astral-sh/uv/pull/20741), [#20767](https://github.com/astral-sh/uv/pull/20767))

  Previously, uv ignored [`SSL_CERT_FILE` or `SSL_CERT_DIR`](https://docs.astral.sh/uv/concepts/authentication/certificates/#custom-certificates) values that pointed to missing or inaccessible paths, empty files or directories, or sources without valid certificates. Instead, it fell back to its default trust roots, potentially allowing HTTPS connections that the configured override was intended to reject.

  Now, any non-empty `SSL_CERT_FILE` or `SSL_CERT_DIR` value replaces uv's default certificate roots, even when no valid certificates can be loaded. In that case, HTTPS requests fail because no certificates are trusted. This applies to package downloads and remote scripts, including GitHub Gists.

  Fix or unset the certificate override. Unsetting it restores the default trust store; empty environment-variable values continue to be ignored.
- **Support pip-compatible `--cert` handling in `uv pip`** ([#20418](https://github.com/astral-sh/uv/pull/20418))

  The `uv pip` interface now accepts [`--cert <path>`](https://docs.astral.sh/uv/concepts/authentication/certificates/#custom-certificates), e.g.:

  ```console
  $ uv pip install --cert ./company-ca.pem example
  ```

  As in pip, the provided PEM bundle replaces all other certificate sources for that invocation, including system certificates and `SSL_CERT_FILE` or `SSL_CERT_DIR`. This change has no effect unless you pass `--cert`. Include the necessary certificate authorities in the bundle.

  `--cert` is only supported by `uv pip` commands; other uv commands continue to use their existing certificate configuration.
- **Discover projects relative to the script passed to `uv run`** ([#20225](https://github.com/astral-sh/uv/pull/20225))

  Previously, `uv run project/script.py` discovered its project from the current directory, even when the script belonged to another project. uv now starts project and workspace discovery from the script's directory instead.

  For example, running `uv run other-project/script.py` now uses `other-project` and its dependencies. This fixes scripts that previously failed because their own dependencies were not installed, but can select a different environment than before.

  You can opt out of script-relative discovery by selecting a project explicitly, e.g., `uv run --project . other-project/script.py`.

  This stabilizes the `target-workspace-discovery` preview feature.
- **Require `--force` before clearing a directory that is not a virtual environment** ([#20225](https://github.com/astral-sh/uv/pull/20225))

  `uv venv --clear` previously removed any existing target directory, even if it was not a virtual environment. uv emitted a warning but still deleted the directory and its contents. Now, uv refuses to clear directories that do not contain a virtual environment.

  You can opt out of this safety check by explicitly passing `--force`, e.g., `uv venv --clear --force ./not-a-virtualenv`.

  This stabilizes the `venv-safe-clear` preview feature.
- **Reject `--project` when initializing a project** ([#20225](https://github.com/astral-sh/uv/pull/20225))

  `--project` selects an existing project, so it is not meaningful when initializing a new one. Previously, `uv init --project example` warned and initialized `example` anyway; if a positional path was also provided, `--project` was ignored.

  This usage is now an error. Use `uv init example` to initialize a project at the requested path, or `uv init --directory example` to change the working directory first.

  This stabilizes the `init-project-flag` preview feature.
- **Reject missing or invalid `--project` paths** ([#20225](https://github.com/astral-sh/uv/pull/20225))

  uv previously warned when `--project` referred to a missing directory or a file other than `pyproject.toml`, but then attempted to continue. This could produce confusing errors later or run against an unintended project.

  Now, `uv run --project missing python` fails immediately instead of continuing. You cannot opt out of this behavior. Create the directory first or select an existing project. Passing `--project path/to/pyproject.toml` remains supported and selects the file's parent directory.

  This stabilizes the `project-directory-must-exist` preview feature.
- **Skip distributions with non-normalized filenames when publishing** ([#20225](https://github.com/astral-sh/uv/pull/20225))

  Distribution filenames must use [normalized package names](https://packaging.python.org/en/latest/specifications/name-normalization/) and versions. For example, a wheel for version `1.01.0` should be named `example-1.1.0-py3-none-any.whl`, not `example-1.01.0-py3-none-any.whl`.

  Previously, `uv publish` warned about non-normalized filenames but still attempted to upload them. It now skips the affected wheels and source distributions instead.

  You cannot opt out of this behavior. Rebuild distributions with normalized filenames before publishing.

  This stabilizes the `publish-require-normalized` preview feature.
- **Classify Conda environments named `base` and `root` by their paths** ([#20225](https://github.com/astral-sh/uv/pull/20225))

  Conda environments named `base` or `root` were previously assumed to be the base Conda environment, even when they were ordinary child environments. uv now recognizes child Conda environments named `base` or `root` based on their paths, as it already does for other names.

  You can opt out of automatic interpreter selection by requesting an interpreter explicitly with `--python /path/to/python`.

  This stabilizes the `special-conda-env-names` preview feature.
- **Reject broken `.venv` symlinks during environment discovery** ([#20433](https://github.com/astral-sh/uv/pull/20433))

  Previously, uv could ignore a broken `.venv` symlink and continue searching parent directories for another virtual environment. As a result, commands such as `uv pip install` could unexpectedly modify an unrelated ancestor environment.

  uv now stops at a broken `.venv` symlink and reports its exact path. Errors encountered while reading virtual environment metadata, including permission failures, are also reported immediately instead of being ignored.

  You cannot opt out of this behavior. Repair or remove the broken `.venv` symlink and correct any permissions that prevent uv from inspecting the environment.
- **Reinstall matching installed Python patch versions instead of upgrading implicitly** ([#20659](https://github.com/astral-sh/uv/pull/20659))

  Before [Python upgrades](https://docs.astral.sh/uv/guides/install-python/#upgrading-python-versions) were supported, `uv python install 3.12 --reinstall` doubled as a way to install the latest Python 3.12 patch release. Now that `--upgrade` is available, `--reinstall` reinstalls the matching patch releases that are already present.

  For example, if Python 3.12.6 and 3.12.7 are installed, `uv python install 3.12 --reinstall` reinstalls both versions instead of installing the latest available 3.12 release.

  You can recover the previous upgrade behavior with `uv python install 3.12 --upgrade`. Combine `--upgrade --reinstall` to reinstall only the latest patch.
- **Require `--upgrade-group` to name an existing dependency group** ([#18957](https://github.com/astral-sh/uv/pull/18957))

  Previously, `uv lock --upgrade-group docs` silently succeeded even if no `docs` [dependency group](https://docs.astral.sh/uv/concepts/projects/dependencies/#dependency-groups) existed. uv now validates the requested group against the project, its workspace members, and workspace-level dependency groups.

  You cannot opt out of this behavior. Correct the group name or add it to `[dependency-groups]`. Legacy `tool.uv.dev-dependencies` still satisfies `--upgrade-group dev`.
- **Resolve relative indexes and find-links against `--directory`** ([#20740](https://github.com/astral-sh/uv/pull/20740))

  The `--directory` option changes the directory in which uv operates. Previously, relative index and find-links paths supplied on the command line were still resolved against the original working directory.

  uv now resolves `--index`, `--default-index`, `--index-url`, `--extra-index-url`, and `--find-links` relative to the directory selected by `--directory`. For example:

  ```console
  $ uv add --directory project --index ./packages example
  ```

  This now uses `project/packages` instead of `./packages` in the original working directory. Absolute paths and indexes loaded from configuration files are unaffected.

  To preserve the previous target, pass an absolute path or adjust the relative path, e.g., `--index ../packages`.
- **Preserve absolute paths provided to `uv add`** ([#18402](https://github.com/astral-sh/uv/pull/18402))

  `uv add` previously converted every local dependency into a project-relative path, even when the original request used an absolute path or a literal `file://` URL. It now preserves the form of the request in `pyproject.toml` and `uv.lock`:

  ```console
  $ uv add ../library             # remains relative
  $ uv add /projects/library      # remains absolute
  ```

  Absolute paths make a project less portable. Use a relative path to avoid recording an absolute path. URLs containing expanded variables retain their existing relative-path behavior.
- **Remove older PyPy distributions that are only available as bzip2 archives** ([#20423](https://github.com/astral-sh/uv/pull/20423))

  Older PyPy patch releases that are only distributed as `.tar.bz2` archives are no longer available through `uv python install`. These releases require unsupported bzip2 archives.

  The latest PyPy release for each supported Python minor version is available as a gzip-compressed archive and remains supported. For example, `uv python list 3.10 --all-versions` still includes the latest PyPy 3.10 release, but older bzip2-only patch releases are omitted.

  You cannot opt out of this behavior. Request a newer PyPy patch release instead.
- **Omit excluded-package comments when annotations are disabled** ([#20085](https://github.com/astral-sh/uv/pull/20085))

  `uv pip compile --no-annotate` suppresses comments describing the generated requirements file. Previously, a footer listing packages excluded with `--unsafe-package` was still included, even though annotations were disabled. That footer is now omitted.

  You can recover the footer by removing `--no-annotate`.

### Stabilizations

- **TOML 1.0-compatible source distributions** ([#20225](https://github.com/astral-sh/uv/pull/20225))

  `uv_build` now writes a TOML 1.0-compatible `pyproject.toml` when building source distributions, allowing older Python build frontends to consume projects that use newer TOML syntax. The original project file remains available in the archive as `pyproject.toml.orig`.

  This stabilizes the `toml-backwards-compatibility` preview feature.
- **Automatic open-file limit adjustment on Unix** ([#20225](https://github.com/astral-sh/uv/pull/20225))

  On Linux and macOS, uv now attempts to raise the soft open-file limit at startup toward the hard limit, capped at 1,048,576 descriptors. The new limit also applies to subprocesses and reduces failures caused by running out of file descriptors. If the limit cannot be raised, uv continues running with the existing limit.

  This stabilizes the `adjust-ulimit` preview feature.

### Preview features

- Allow `uv upgrade` to target multiple packages, upgrade all production dependencies, and exclude selected dependencies ([#20338](https://github.com/astral-sh/uv/pull/20338))

### Bug fixes

- Include extras activated by dependency groups when evaluating conflicts ([#20237](https://github.com/astral-sh/uv/pull/20237))

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


