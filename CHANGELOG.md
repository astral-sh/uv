# Changelog

<!-- prettier-ignore-start -->


## 0.9.9

Released on 2025-11-12.

### Deprecations

- Deprecate use of `--project` in `uv init` ([#16674](https://github.com/astral-sh/uv/pull/16674))

### Enhancements

- Add iOS support to Python interpreter discovery ([#16686](https://github.com/astral-sh/uv/pull/16686))
- Reject ambiguously parsed URLs ([#16622](https://github.com/astral-sh/uv/pull/16622))
- Allow explicit values in `uv version --bump` ([#16555](https://github.com/astral-sh/uv/pull/16555))
- Warn on use of managed pre-release Python versions when a stable version is available ([#16619](https://github.com/astral-sh/uv/pull/16619))
- Allow signing trampolines on Windows by using `.rcdata` to store metadata ([#15068](https://github.com/astral-sh/uv/pull/15068))
- Add `--only-emit-workspace` and similar variants to `uv export` ([#16681](https://github.com/astral-sh/uv/pull/16681))

### Preview features

- Add `uv workspace dir` command ([#16678](https://github.com/astral-sh/uv/pull/16678))
- Add `uv workspace metadata` command ([#16516](https://github.com/astral-sh/uv/pull/16516))

### Configuration

- Add `UV_NO_DEFAULT_GROUPS` environment variable ([#16645](https://github.com/astral-sh/uv/pull/16645))

### Bug fixes

- Remove `torch-model-archiver` and `torch-tb-profiler` from PyTorch backend ([#16655](https://github.com/astral-sh/uv/pull/16655))
- Fix Pixi environment detection ([#16585](https://github.com/astral-sh/uv/pull/16585))

### Documentation

- Fix `CMD` path in FastAPI Dockerfile ([#16701](https://github.com/astral-sh/uv/pull/16701))


## 0.9.8

Released on 2025-11-07.

### Enhancements

- Accept multiple packages in `uv export` ([#16603](https://github.com/astral-sh/uv/pull/16603))
- Accept multiple packages in `uv sync` ([#16543](https://github.com/astral-sh/uv/pull/16543))
- Add a `uv cache size` command ([#16032](https://github.com/astral-sh/uv/pull/16032))
- Add prerelease guidance for build-system resolution failures ([#16550](https://github.com/astral-sh/uv/pull/16550))
- Allow Python requests to include `+gil` to require a GIL-enabled interpreter ([#16537](https://github.com/astral-sh/uv/pull/16537))
- Avoid pluralizing 'retry' for single value ([#16535](https://github.com/astral-sh/uv/pull/16535))
- Enable first-class dependency exclusions ([#16528](https://github.com/astral-sh/uv/pull/16528))
- Fix inclusive constraints on available package versions in resolver errors ([#16629](https://github.com/astral-sh/uv/pull/16629))
- Improve `uv init` error for invalid directory names ([#16554](https://github.com/astral-sh/uv/pull/16554))
- Show help on `uv build -h` ([#16632](https://github.com/astral-sh/uv/pull/16632))
- Include the Python variant suffix in "Using Python ..." messages ([#16536](https://github.com/astral-sh/uv/pull/16536))
- Log most recently modified file for cache-keys ([#16338](https://github.com/astral-sh/uv/pull/16338))
- Update Docker builds to use nightly Rust toolchain with musl v1.2.5 ([#16584](https://github.com/astral-sh/uv/pull/16584))
- Add GitHub attestations for uv release artifacts ([#11357](https://github.com/astral-sh/uv/pull/11357))

### Configuration

- Expose `UV_NO_GROUP` as an environment variable ([#16529](https://github.com/astral-sh/uv/pull/16529))
- Add `UV_NO_SOURCES` as an environment variable ([#15883](https://github.com/astral-sh/uv/pull/15883))

### Bug fixes

- Allow `--check` and `--locked` to be used together in `uv lock` ([#16538](https://github.com/astral-sh/uv/pull/16538))
- Allow for unnormalized names in the METADATA file (#16547) ([#16548](https://github.com/astral-sh/uv/pull/16548))
- Fix missing value_type for `default-groups` in schema ([#16575](https://github.com/astral-sh/uv/pull/16575))
- Respect multi-GPU outputs in `nvidia-smi` ([#15460](https://github.com/astral-sh/uv/pull/15460))
- Fix DNS lookup errors in Docker containers ([#8450](https://github.com/astral-sh/uv/issues/8450))

### Documentation

- Fix typo in uv tool list doc ([#16625](https://github.com/astral-sh/uv/pull/16625))
- Note `uv pip list` name normalization in docs ([#13210](https://github.com/astral-sh/uv/pull/13210))

### Other changes

- Update Rust toolchain to 1.91 and MSRV to 1.89 ([#16531](https://github.com/astral-sh/uv/pull/16531))

## 0.9.7

Released on 2025-10-30.

### Enhancements

- Add Windows x86-32 emulation support to interpreter architecture checks ([#13475](https://github.com/astral-sh/uv/pull/13475))
- Improve readability of progress bars ([#16509](https://github.com/astral-sh/uv/pull/16509))

### Bug fixes

- Drop terminal coloring from `uv auth token` output ([#16504](https://github.com/astral-sh/uv/pull/16504))
- Don't use UV_LOCKED to enable `--check` flag ([#16521](https://github.com/astral-sh/uv/pull/16521))

## 0.9.6

Released on 2025-10-29.

This release contains an upgrade to Astral's fork of `async_zip`, which addresses potential sources of ZIP parsing differentials between uv and other Python packaging tooling. See [GHSA-pqhf-p39g-3x64](https://github.com/astral-sh/uv/security/advisories/GHSA-pqhf-p39g-3x64) for additional details.

### Security

* Address ZIP parsing differentials ([GHSA-pqhf-p39g-3x64](https://github.com/astral-sh/uv/security/advisories/GHSA-pqhf-p39g-3x64))

### Python

- Upgrade GraalPy to 25.0.1 ([#16401](https://github.com/astral-sh/uv/pull/16401))

### Enhancements

- Add `--clear` to `uv build` to remove old build artifacts ([#16371](https://github.com/astral-sh/uv/pull/16371))
- Add `--no-create-gitignore` to `uv build` ([#16369](https://github.com/astral-sh/uv/pull/16369))
- Do not error when a virtual environment directory cannot be removed due to a busy error ([#16394](https://github.com/astral-sh/uv/pull/16394))
- Improve hint on `pip install --system` when externally managed ([#16392](https://github.com/astral-sh/uv/pull/16392))
- Running `uv lock --check` with outdated lockfile will print that `--check` was passed, instead of `--locked`  ([#16322](https://github.com/astral-sh/uv/pull/16322))
- Update `uv init` template for Maturin ([#16449](https://github.com/astral-sh/uv/pull/16449))
- Improve ordering of Python sources in logs ([#16463](https://github.com/astral-sh/uv/pull/16463))
- Restore DockerHub release images and annotations ([#16441](https://github.com/astral-sh/uv/pull/16441))

### Bug fixes

- Check for matching Python implementation during `uv python upgrade` ([#16420](https://github.com/astral-sh/uv/pull/16420))
- Deterministically order `--find-links` distributions ([#16446](https://github.com/astral-sh/uv/pull/16446))
- Don't panic in `uv export --frozen` when the lockfile is outdated ([#16407](https://github.com/astral-sh/uv/pull/16407))
- Fix root of `uv tree` when `--package` is used with circular dependencies ([#15908](https://github.com/astral-sh/uv/pull/15908))
- Show package list with `pip freeze --quiet` ([#16491](https://github.com/astral-sh/uv/pull/16491))
- Limit `uv auth login pyx.dev` retries to 60s ([#16498](https://github.com/astral-sh/uv/pull/16498))
- Add an empty group with `uv add --group ... -r ...` ([#16490](https://github.com/astral-sh/uv/pull/16490))

### Documentation

- Update docs for maturin build backend init template ([#16469](https://github.com/astral-sh/uv/pull/16469))
- Update docs to reflect previous changes to signal forwarding semantics ([#16430](https://github.com/astral-sh/uv/pull/16430))
- Add instructions for installing via MacPorts ([#16039](https://github.com/astral-sh/uv/pull/16039))

## 0.9.5

Released on 2025-10-21.

This release contains an upgrade to `astral-tokio-tar`, which addresses a vulnerability in tar extraction on malformed archives with mismatching size information between the ustar header and PAX extensions. While the `astral-tokio-tar` advisory has been graded as "high" due its potential broader impact, the *specific* impact to uv is **low** due to a lack of novel attacker capability. Specifically, uv only processes tar archives from source distributions, which already possess the capability for full arbitrary code execution by design, meaning that an attacker gains no additional capabilities through `astral-tokio-tar`.

Regardless, we take the hypothetical risk of parser differentials very seriously. Out of an abundance of caution, we have assigned this upgrade an advisory: https://github.com/astral-sh/uv/security/advisories/GHSA-w476-p2h3-79g9

### Security

* Upgrade `astral-tokio-tar` to 0.5.6 to address a parsing differential ([#16387](https://github.com/astral-sh/uv/pull/16387))

### Enhancements

- Add required environment marker example to hint ([#16244](https://github.com/astral-sh/uv/pull/16244))
- Fix typo in MissingTopLevel warning ([#16351](https://github.com/astral-sh/uv/pull/16351))
- Improve 403 Forbidden error message to indicate package may not exist ([#16353](https://github.com/astral-sh/uv/pull/16353))
- Add a hint on `uv pip install` failure if the `--system` flag is used to select an externally managed interpreter ([#16318](https://github.com/astral-sh/uv/pull/16318))

### Bug fixes

- Fix backtick escaping for PowerShell ([#16307](https://github.com/astral-sh/uv/pull/16307))

### Documentation

- Document metadata consistency expectation ([#15683](https://github.com/astral-sh/uv/pull/15683))
- Remove outdated aarch64 musl note ([#16385](https://github.com/astral-sh/uv/pull/16385))

## 0.9.4

Released on 2025-10-17.

### Enhancements

- Add CUDA 13.0 support ([#16321](https://github.com/astral-sh/uv/pull/16321))
- Add auto-detection for Intel GPU on Windows ([#16280](https://github.com/astral-sh/uv/pull/16280))
- Implement display of RFC 9457 HTTP error contexts ([#16199](https://github.com/astral-sh/uv/pull/16199))

### Bug fixes

- Avoid obfuscating pyx tokens in `uv auth token` output ([#16345](https://github.com/astral-sh/uv/pull/16345))

## 0.9.3

Released on 2025-10-14.

### Python

- Add CPython 3.15.0a1
- Add CPython 3.13.9

### Enhancements

- Obfuscate secret token values in logs ([#16164](https://github.com/astral-sh/uv/pull/16164))

### Bug fixes

- Fix workspace with relative pathing ([#16296](https://github.com/astral-sh/uv/pull/16296))

## 0.9.2

Released on 2025-10-10.

### Python

- Add CPython 3.9.24.
- Add CPython 3.10.19.
- Add CPython 3.11.14.
- Add CPython 3.12.12.

### Enhancements

- Avoid inferring check URLs for pyx in `uv publish` ([#16234](https://github.com/astral-sh/uv/pull/16234))
- Add `uv tool list --show-python` ([#15814](https://github.com/astral-sh/uv/pull/15814))

### Documentation

- Add missing "added in" to new environment variables in reference ([#16217](https://github.com/astral-sh/uv/pull/16217))

## 0.9.1

Released on 2025-10-09.

### Enhancements

- Log Python choice in `uv init` ([#16182](https://github.com/astral-sh/uv/pull/16182))
- Fix `pylock.toml` config conflict error messages ([#16211](https://github.com/astral-sh/uv/pull/16211))

### Configuration

- Add `UV_UPLOAD_HTTP_TIMEOUT` and respect `UV_HTTP_TIMEOUT` in uploads ([#16040](https://github.com/astral-sh/uv/pull/16040))
- Support `UV_WORKING_DIRECTORY` for setting `--directory` ([#16125](https://github.com/astral-sh/uv/pull/16125))

### Bug fixes

- Allow missing `Scripts` directory ([#16206](https://github.com/astral-sh/uv/pull/16206))
- Fix handling of Python requests with pre-releases in ranges ([#16208](https://github.com/astral-sh/uv/pull/16208))
- Preserve comments on version bump ([#16141](https://github.com/astral-sh/uv/pull/16141))
- Retry all HTTP/2 errors ([#16038](https://github.com/astral-sh/uv/pull/16038))
- Treat deleted Windows registry keys as equivalent to missing ones ([#16194](https://github.com/astral-sh/uv/pull/16194))
- Ignore pre-release Python versions when a patch version is requested ([#16210](https://github.com/astral-sh/uv/pull/16210))

### Documentation

- Document why uv discards upper bounds on `requires-python` ([#15927](https://github.com/astral-sh/uv/pull/15927))
- Document uv version environment variables were added in ([#15196](https://github.com/astral-sh/uv/pull/15196))

## 0.9.0

Released on 2025-10-07.

This breaking release is primarily motivated by the release of Python 3.14, which contains some breaking changes (we recommend reading the ["What's new in Python 3.14"](https://docs.python.org/3/whatsnew/3.14.html) page). uv may use Python 3.14 in cases where it previously used 3.13, e.g., if you have not pinned your Python version and do not have any Python versions installed on your machine. While we think this is uncommon, we prefer to be cautious. We've included some additional small changes that could break workflows.

See our [Python 3.14](https://astral.sh/blog/python-3.14) blog post for some discussion of features we're excited about!

There are no breaking changes to [`uv_build`](https://docs.astral.sh/uv/concepts/build-backend/). If you have an upper bound in your `[build-system]` table, you should update it.

### Breaking changes

- **Python 3.14 is now the default stable version**
  
  The default Python version has changed from 3.13 to 3.14. This applies to Python version installation when no Python version is requested, e.g., `uv python install`. By default, uv will use the system Python version if present, so this may not cause changes to general use of uv. For example, if Python 3.13 is installed already, then `uv venv` will use that version. If no Python versions are installed on a machine and automatic downloads are enabled, uv will now use 3.14 instead of 3.13, e.g., for `uv venv` or `uvx python`. This change will not affect users who are using a `.python-version` file to pin to a specific Python version.
- **Allow use of free-threaded variants in Python 3.14+ without explicit opt-in** ([#16142](https://github.com/astral-sh/uv/pull/16142))
  
  Previously, free-threaded variants of Python were considered experimental and required explicit opt-in (i.e., with `3.14t`) for usage. Now uv will allow use of free-threaded Python 3.14+ interpreters without explicit selection. The GIL-enabled build of Python will still be preferred, e.g., when performing an installation with `uv python install 3.14`. However, e.g., if a free-threaded interpreter comes before a GIL-enabled build on the `PATH`, it will be used. This change does not apply to free-threaded Python 3.13 interpreters, which will continue to require opt-in.
- **Use Python 3.14 stable Docker images** ([#16150](https://github.com/astral-sh/uv/pull/16150))
  
  Previously, the Python 3.14 images had an `-rc` suffix, e.g., `python:3.14-rc-alpine` or
`python:3.14-rc-trixie`. Now, the `-rc` suffix has been removed to match the stable
[upstream images](https://hub.docker.com/_/python). The `-rc` images tags will no longer be
updated. This change should not break existing workflows.
- **Upgrade Alpine Docker image to Alpine 3.22**
  
  Previously, the `uv:alpine` Docker image was based on Alpine 3.21. Now, this image is based on Alpine 3.22. The previous image can be recovered with `uv:alpine3.21` and will continue to be updated until a future release.
- **Upgrade Debian Docker images to Debian 13 "Trixie"**
  
  Previously, the `uv:debian` and `uv:debian-slim` Docker images were based on Debian 12 "Bookworm". Now, these images are based on Debian 13 "Trixie". The previous images can be recovered with `uv:bookworm` and `uv:bookworm-slim` and will continue to be updated until a future release.
- **Fix incorrect output path when a trailing `/` is used in `uv build`** ([#15133](https://github.com/astral-sh/uv/pull/15133))
  
  When using `uv build` in a workspace, the artifacts are intended to be written to a `dist` directory in the workspace root. A bug caused workspace root determination to fail when the input path included a trailing `/` causing the `dist` directory to be placed in the child directory. This bug has been fixed in this release. For example, `uv build child/` is used, the output path will now be in `<workspace root>/dist/` rather than `<workspace root>/child/dist/`.

### Python

- Add CPython 3.14.0
- Add CPython 3.13.8

### Enhancements

- Don't warn when a dependency is constrained by another dependency ([#16149](https://github.com/astral-sh/uv/pull/16149))

### Bug fixes

- Fix `uv python upgrade / install` output when there is a no-op for one request ([#16158](https://github.com/astral-sh/uv/pull/16158))
- Surface pinned-version hint when `uv tool upgrade` can’t move the tool ([#16081](https://github.com/astral-sh/uv/pull/16081))
- Ban pre-release versions in `uv python upgrade` requests ([#16160](https://github.com/astral-sh/uv/pull/16160))
- Fix `uv python upgrade` replacement of installed binaries on pre-release to stable ([#16159](https://github.com/astral-sh/uv/pull/16159))

### Documentation

- Update `uv pip compile` args in `layout.md` ([#16155](https://github.com/astral-sh/uv/pull/16155))

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


