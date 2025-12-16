# Changelog

<!-- prettier-ignore-start -->


## 0.9.18

Released on 2025-12-16.

### Enhancements

- Add value hints to command line arguments to improve shell completion accuracy ([#17080](https://github.com/astral-sh/uv/pull/17080))
- Improve error handling in `uv publish` ([#17096](https://github.com/astral-sh/uv/pull/17096))
- Improve rendering of multiline error messages ([#17132](https://github.com/astral-sh/uv/pull/17132))
- Support redirects in `uv publish` ([#17130](https://github.com/astral-sh/uv/pull/17130))
- Include Docker images with the alpine version, e.g., `python3.x-alpine3.23` ([#17100](https://github.com/astral-sh/uv/pull/17100))

### Configuration

- Accept `--torch-backend` in `[tool.uv]` ([#17116](https://github.com/astral-sh/uv/pull/17116))

### Performance

- Speed up `uv cache size` ([#17015](https://github.com/astral-sh/uv/pull/17015))
- Initialize S3 signer once ([#17092](https://github.com/astral-sh/uv/pull/17092))

### Bug fixes

- Avoid panics due to reads on failed requests ([#17098](https://github.com/astral-sh/uv/pull/17098))
- Enforce latest-version in `@latest` requests ([#17114](https://github.com/astral-sh/uv/pull/17114))
- Explicitly set `EntryType` for file entries in tar ([#17043](https://github.com/astral-sh/uv/pull/17043))
- Ignore `pyproject.toml` index username in lockfile comparison ([#16995](https://github.com/astral-sh/uv/pull/16995))
- Relax error when using `uv add` with `UV_GIT_LFS` set ([#17127](https://github.com/astral-sh/uv/pull/17127))
- Support file locks on ExFAT on macOS ([#17115](https://github.com/astral-sh/uv/pull/17115))
- Change schema for `exclude-newer` into optional string ([#17121](https://github.com/astral-sh/uv/pull/17121))

### Documentation

- Drop arm musl caveat from Docker documentation ([#17111](https://github.com/astral-sh/uv/pull/17111))
- Fix version reference in resolver example ([#17085](https://github.com/astral-sh/uv/pull/17085))
- Better documentation for `exclude-newer*` ([#17079](https://github.com/astral-sh/uv/pull/17079))

## 0.9.17

Released on 2025-12-09.

### Enhancements

- Add `torch-tensorrt` and `torchao` to the PyTorch list ([#17053](https://github.com/astral-sh/uv/pull/17053))
- Add hint for misplaced `--verbose`  in `uv tool run` ([#17020](https://github.com/astral-sh/uv/pull/17020))
- Add support for relative durations in `exclude-newer` (a.k.a., dependency cooldowns) ([#16814](https://github.com/astral-sh/uv/pull/16814))
- Add support for relocatable nushell activation script ([#17036](https://github.com/astral-sh/uv/pull/17036))

### Bug fixes

- Respect dropped (but explicit) indexes in dependency groups ([#17012](https://github.com/astral-sh/uv/pull/17012))

### Documentation

- Improve `source-exclude` reference docs ([#16832](https://github.com/astral-sh/uv/pull/16832))
- Recommend `UV_NO_DEV` in Docker installs ([#17030](https://github.com/astral-sh/uv/pull/17030))
- Update `UV_VERSION` in docs for GitLab CI/CD ([#17040](https://github.com/astral-sh/uv/pull/17040))

## 0.9.16

Released on 2025-12-06.

### Python

- Add CPython 3.14.2
- Add CPython 3.13.11

### Enhancements

- Add a 5m default timeout to acquiring file locks to fail faster on deadlock ([#16342](https://github.com/astral-sh/uv/pull/16342))
- Add a stub `debug` subcommand to `uv pip` announcing its intentional absence ([#16966](https://github.com/astral-sh/uv/pull/16966))
- Add bounds in `uv add --script` ([#16954](https://github.com/astral-sh/uv/pull/16954))
- Add brew specific message for `uv self update` ([#16838](https://github.com/astral-sh/uv/pull/16838))
- Error when built wheel is for the wrong platform ([#16074](https://github.com/astral-sh/uv/pull/16074))
- Filter wheels from PEP 751 files based on `--no-binary` et al in `uv pip compile` ([#16956](https://github.com/astral-sh/uv/pull/16956))
- Support `--target` and `--prefix` in `uv pip list`, `uv pip freeze`, and `uv pip show` ([#16955](https://github.com/astral-sh/uv/pull/16955))
- Tweak language for build backend validation errors ([#16720](https://github.com/astral-sh/uv/pull/16720))
- Use explicit credentials cache instead of global static ([#16768](https://github.com/astral-sh/uv/pull/16768))
- Enable SIMD in HTML parsing ([#17010](https://github.com/astral-sh/uv/pull/17010))

### Preview features

- Fix missing preview warning in `uv workspace metadata` ([#16988](https://github.com/astral-sh/uv/pull/16988))
- Add a `uv auth helper --protocol bazel` command ([#16886](https://github.com/astral-sh/uv/pull/16886))

### Bug fixes

- Fix Pyston wheel compatibility tags ([#16972](https://github.com/astral-sh/uv/pull/16972))
- Allow redundant entries in `tool.uv.build-backend.module-name` but emit warnings ([#16928](https://github.com/astral-sh/uv/pull/16928))
- Fix infinite loop in non-attribute re-treats during HTML parsing ([#17010](https://github.com/astral-sh/uv/pull/17010))

### Documentation

- Clarify `--project` flag help text to indicate project discovery ([#16965](https://github.com/astral-sh/uv/pull/16965))
- Regenerate the crates.io READMEs on release ([#16992](https://github.com/astral-sh/uv/pull/16992))
- Update Docker integration guide to prefer `COPY` over `ADD` for simple cases ([#16883](https://github.com/astral-sh/uv/pull/16883))
- Update PyTorch documentation to include information about supporting CUDA 13.0.x ([#16957](https://github.com/astral-sh/uv/pull/16957))
- Update the versioning policy ([#16710](https://github.com/astral-sh/uv/pull/16710))
- Upgrade PyTorch documentation to latest versions ([#16970](https://github.com/astral-sh/uv/pull/16970))

## 0.9.15

Released on 2025-12-02.

### Python

- Add CPython 3.14.1
- Add CPython 3.13.10

### Enhancements

- Add ROCm 6.4 to `--torch-backend=auto` ([#16919](https://github.com/astral-sh/uv/pull/16919))
- Add a Windows manifest to uv binaries ([#16894](https://github.com/astral-sh/uv/pull/16894))
- Add LFS toggle to Git sources ([#16143](https://github.com/astral-sh/uv/pull/16143))
- Cache source reads during resolution ([#16888](https://github.com/astral-sh/uv/pull/16888))
- Allow reading requirements from scripts without an extension ([#16923](https://github.com/astral-sh/uv/pull/16923))
- Allow reading requirements from scripts with HTTP(S) paths ([#16891](https://github.com/astral-sh/uv/pull/16891))

### Configuration

- Add `UV_HIDE_BUILD_OUTPUT` to omit build logs ([#16885](https://github.com/astral-sh/uv/pull/16885))

### Bug fixes

- Fix `uv-trampoline-builder` builds from crates.io by moving bundled executables ([#16922](https://github.com/astral-sh/uv/pull/16922))
- Respect `NO_COLOR` and always show the command as a header when paging `uv help` output ([#16908](https://github.com/astral-sh/uv/pull/16908))
- Use `0o666` permissions for flock files instead of `0o777` ([#16845](https://github.com/astral-sh/uv/pull/16845))
- Revert "Bump `astral-tl` to v0.7.10 (#16887)" to narrow down a regression causing hangs in metadata retrieval ([#16938](https://github.com/astral-sh/uv/pull/16938))

### Documentation

- Link to the uv version in crates.io member READMEs ([#16939](https://github.com/astral-sh/uv/pull/16939))

## 0.9.14

Released on 2025-12-01.

### Performance

- Bump `astral-tl` to v0.7.10 to enable SIMD for HTML parsing ([#16887](https://github.com/astral-sh/uv/pull/16887))

### Bug fixes

- Allow earlier post releases with exclusive ordering ([#16881](https://github.com/astral-sh/uv/pull/16881))
- Prefer updating existing `.zshenv` over creating a new one in `tool update-shell` ([#16866](https://github.com/astral-sh/uv/pull/16866))
- Respect `-e` flags in `uv add` ([#16882](https://github.com/astral-sh/uv/pull/16882))

### Enhancements

- Attach subcommand to User-Agent string ([#16837](https://github.com/astral-sh/uv/pull/16837))
- Prefer `UV_WORKING_DIR` over `UV_WORKING_DIRECTORY` for consistency ([#16884](https://github.com/astral-sh/uv/pull/16884))

## 0.9.13

Released on 2025-11-26.

### Bug fixes

- Revert "Allow `--with-requirements` to load extensionless inline-metadata scripts" to fix reading of requirements files from streams ([#16861](https://github.com/astral-sh/uv/pull/16861))
- Validate URL wheel tags against `Requires-Python` and required environments ([#16824](https://github.com/astral-sh/uv/pull/16824))

### Documentation

- Drop unpublished crates from the uv crates.io README ([#16847](https://github.com/astral-sh/uv/pull/16847))
- Fix the links to uv in crates.io member READMEs ([#16848](https://github.com/astral-sh/uv/pull/16848))

## 0.9.12

Released on 2025-11-24.

### Enhancements

- Allow `--with-requirements` to load extensionless inline-metadata scripts ([#16744](https://github.com/astral-sh/uv/pull/16744))
- Collect and upload PEP 740 attestations during `uv publish` ([#16731](https://github.com/astral-sh/uv/pull/16731))
- Prevent `uv export` from overwriting `pyproject.toml` ([#16745](https://github.com/astral-sh/uv/pull/16745))

### Documentation

- Add a crates.io README for uv ([#16809](https://github.com/astral-sh/uv/pull/16809))
- Add documentation for intermediate Docker layers in a workspace ([#16787](https://github.com/astral-sh/uv/pull/16787))
- Enumerate workspace members in the uv crate README ([#16811](https://github.com/astral-sh/uv/pull/16811))
- Fix documentation links for crates ([#16801](https://github.com/astral-sh/uv/pull/16801))
- Generate a crates.io README for uv workspace members ([#16812](https://github.com/astral-sh/uv/pull/16812))
- Move the "Export" guide to the projects concept section ([#16835](https://github.com/astral-sh/uv/pull/16835))
- Update the cargo install recommendation to use crates ([#16800](https://github.com/astral-sh/uv/pull/16800))
- Use the word "internal" in crate descriptions ([#16810](https://github.com/astral-sh/uv/pull/16810))

## 0.9.11

Released on 2025-11-20.

### Python

- Add CPython 3.15.0a2

See the [`python-build-standalone` release notes](https://github.com/astral-sh/python-build-standalone/releases/tag/20251120) for details.

### Enhancements

- Add SBOM support to `uv export` ([#16523](https://github.com/astral-sh/uv/pull/16523))
- Publish to `crates.io` ([#16770](https://github.com/astral-sh/uv/pull/16770))

### Preview features

- Add `uv workspace list --paths` ([#16776](https://github.com/astral-sh/uv/pull/16776))
- Fix the preview warning on `uv workspace dir` ([#16775](https://github.com/astral-sh/uv/pull/16775))

### Bug fixes

- Fix `uv init` author serialization via `toml_edit` inline tables ([#16778](https://github.com/astral-sh/uv/pull/16778))
- Fix status messages without TTY ([#16785](https://github.com/astral-sh/uv/pull/16785))
- Preserve end-of-line comment whitespace when editing `pyproject.toml` ([#16734](https://github.com/astral-sh/uv/pull/16734))
- Disable `always-authenticate` when running under Dependabot ([#16773](https://github.com/astral-sh/uv/pull/16773))

### Documentation

- Document the new behavior for free-threaded python versions ([#16781](https://github.com/astral-sh/uv/pull/16781))
- Improve note about build system in publish guide ([#16788](https://github.com/astral-sh/uv/pull/16788))
- Move do not upload publish note out of the guide into concepts ([#16789](https://github.com/astral-sh/uv/pull/16789))

## 0.9.10

Released on 2025-11-17.

### Enhancements

- Add support for `SSL_CERT_DIR` ([#16473](https://github.com/astral-sh/uv/pull/16473))
- Enforce UTF‑8-encoded license files during `uv build` ([#16699](https://github.com/astral-sh/uv/pull/16699))
- Error when a `project.license-files` glob matches nothing ([#16697](https://github.com/astral-sh/uv/pull/16697))
- `pip install --target` (and `sync`) install Python if necessary ([#16694](https://github.com/astral-sh/uv/pull/16694))
- Account for `python_downloads_json_url` in pre-release Python version warnings ([#16737](https://github.com/astral-sh/uv/pull/16737))
- Support HTTP/HTTPS URLs in `uv python --python-downloads-json-url` ([#16542](https://github.com/astral-sh/uv/pull/16542))

### Preview features

- Add support for `--upgrade` in `uv python install` ([#16676](https://github.com/astral-sh/uv/pull/16676))
- Fix handling of `python install --default` for pre-release Python versions ([#16706](https://github.com/astral-sh/uv/pull/16706))
- Add `uv workspace list` to list workspace members ([#16691](https://github.com/astral-sh/uv/pull/16691))

### Bug fixes

- Don't check file URLs for ambiguously parsed credentials ([#16759](https://github.com/astral-sh/uv/pull/16759))

### Documentation

- Add a "storage" reference document ([#15954](https://github.com/astral-sh/uv/pull/15954))

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


