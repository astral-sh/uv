# Changelog

<!-- prettier-ignore-start -->


## 0.11.8

Released on 2026-04-27.

### Enhancements

- Add `--python-downloads-json-url` to `python pin` ([#19092](https://github.com/astral-sh/uv/pull/19092))
- Fetch uv from Astral mirror during self-update ([#18682](https://github.com/astral-sh/uv/pull/18682))
- Support `pip uninstall -y` ([#19082](https://github.com/astral-sh/uv/pull/19082))
- Add `UV_PYTHON_NO_REGISTRY` ([#19035](https://github.com/astral-sh/uv/pull/19035))
- Allow `exclude-newer` to be missing from the lockfile when `exclude-newer-span` is present ([#19024](https://github.com/astral-sh/uv/pull/19024))
- Only show the version number in `uv self version --short` ([#19019](https://github.com/astral-sh/uv/pull/19019))
- Silence warnings on empty `SSL_CERT_DIR` directory ([#19018](https://github.com/astral-sh/uv/pull/19018))
- Use a sentinel timestamp for relative `exclude-newer` and `exclude-newer-package` values in lockfiles ([#19022](https://github.com/astral-sh/uv/pull/19022), [#19101](https://github.com/astral-sh/uv/pull/19101))
 
### Configuration

- Add an environment variable for `UV_NO_PROJECT` ([#19052](https://github.com/astral-sh/uv/pull/19052))
- Expose `UV_PYTHON_SEARCH_PATH` for Python discovery `PATH` overrides ([#19034](https://github.com/astral-sh/uv/pull/19034))

### Bug fixes

- Add `rust-toolchain.toml` to uv-build sdist ([#19131](https://github.com/astral-sh/uv/pull/19131))
- Ensure uv invocations of git do not inherit repository location environment variables ([#19088](https://github.com/astral-sh/uv/pull/19088))
- Redact pre-signed upload URLs in verbose output ([#19146](https://github.com/astral-sh/uv/pull/19146))
- Handle transitive URL dependencies in PEP 517 build requirements ([#19076](https://github.com/astral-sh/uv/pull/19076), [#19086](https://github.com/astral-sh/uv/pull/19086))
- Support `uv lock` on a `pyproject.toml` that only contains dependency-groups ([#19087](https://github.com/astral-sh/uv/pull/19087))
- Disable transparent Python upgrades in projects when a patch version is requested via `.python-version` ([#19102](https://github.com/astral-sh/uv/pull/19102))
- Fix Python variant tagging in the Windows registry ([#19012](https://github.com/astral-sh/uv/pull/19012))
- Use a single codepath for extracting a .tar.zst wheel, disallowing external symlinks ([#19144](https://github.com/astral-sh/uv/pull/19144))

### Documentation

- Bump astral-sh/setup-uv version in docs ([#19030](https://github.com/astral-sh/uv/pull/19030))
- Update PyTorch documentation for PyTorch 2.11 ([#19095](https://github.com/astral-sh/uv/pull/19095))
- Remove deprecated license classifiers from uv-build and add Python 3.14 classifier ([#19130](https://github.com/astral-sh/uv/pull/19130))

## 0.11.7

Released on 2026-04-15.

### Python

- Upgrade CPython build to 20260414 including an OpenSSL security upgrade ([#19004](https://github.com/astral-sh/uv/pull/19004))

### Enhancements

- Elevate configuration errors to `required-version` mismatches ([#18977](https://github.com/astral-sh/uv/pull/18977))
- Further improve TLS certificate validation messages ([#18933](https://github.com/astral-sh/uv/pull/18933))
- Improve `--exclude-newer` hints  ([#18952](https://github.com/astral-sh/uv/pull/18952))

### Preview features

- Fix `--script` handling in `uv audit` ([#18970](https://github.com/astral-sh/uv/pull/18970))
- Fix traversal of extras in `uv audit` ([#18970](https://github.com/astral-sh/uv/pull/18970))

### Bug fixes

- De-quote `workspace metadata` in linehaul data ([#18966](https://github.com/astral-sh/uv/pull/18966))
- Avoid installing tool workspace member dependencies as editable ([#18891](https://github.com/astral-sh/uv/pull/18891))
- Emit JSON report for `uv sync --check` failures ([#18976](https://github.com/astral-sh/uv/pull/18976))
- Filter and warn on invalid TLS certificates ([#18951](https://github.com/astral-sh/uv/pull/18951))
- Fix equality comparisons for version specifiers with `~=` operators ([#18960](https://github.com/astral-sh/uv/pull/18960))
- Fix stale Python upgrade preview feature check in project environment construction ([#18961](https://github.com/astral-sh/uv/pull/18961))
- Improve Windows path normalization ([#18945](https://github.com/astral-sh/uv/pull/18945))

## 0.11.6

Released on 2026-04-09.

This release resolves a low severity security advisory in which wheels with malformed RECORD entries could delete arbitrary files on uninstall. See [GHSA-pjjw-68hj-v9mw](https://github.com/astral-sh/uv/security/advisories/GHSA-pjjw-68hj-v9mw) for details.

### Bug fixes

- Do not remove files outside the venv on uninstall ([#18942](https://github.com/astral-sh/uv/pull/18942))
- Validate and heal wheel `RECORD` during installation ([#18943](https://github.com/astral-sh/uv/pull/18943))
- Avoid `uv cache clean` errors due to Win32 path normalization ([#18856](https://github.com/astral-sh/uv/pull/18856))

## 0.11.5

Released on 2026-04-08.

### Python

- Add CPython 3.13.13, 3.14.4, and 3.15.0a8 ([#18908](https://github.com/astral-sh/uv/pull/18908))

### Enhancements

- Fix `build_system.requires` error message ([#18911](https://github.com/astral-sh/uv/pull/18911))
- Remove trailing path separators in path normalization ([#18915](https://github.com/astral-sh/uv/pull/18915))
- Improve error messages for unsupported or invalid TLS certificates ([#18924](https://github.com/astral-sh/uv/pull/18924))

### Preview features

- Add `exclude-newer` to `[[tool.uv.index]]` ([#18839](https://github.com/astral-sh/uv/pull/18839))
- `uv audit`: add context/warnings for ignored vulnerabilities ([#18905](https://github.com/astral-sh/uv/pull/18905))

### Bug fixes

- Normalize persisted fork markers before lock equality checks ([#18612](https://github.com/astral-sh/uv/pull/18612))
- Clear junction properly when uninstalling Python versions on Windows ([#18815](https://github.com/astral-sh/uv/pull/18815))
- Report error cleanly instead of panicking on TLS certificate error ([#18904](https://github.com/astral-sh/uv/pull/18904))

### Documentation

- Remove the legacy `PIP_COMPATIBILITY.md` redirect file ([#18928](https://github.com/astral-sh/uv/pull/18928))
- Fix `uv init example-bare --bare` examples ([#18822](https://github.com/astral-sh/uv/pull/18822), [#18925](https://github.com/astral-sh/uv/pull/18925))

## 0.11.4

Released on 2026-04-07.

### Enhancements

- Add support for `--upgrade-group` ([#18266](https://github.com/astral-sh/uv/pull/18266))
- Merge repeated archive URL hashes by version ID ([#18841](https://github.com/astral-sh/uv/pull/18841))
- Require all direct URL hash algorithms to match ([#18842](https://github.com/astral-sh/uv/pull/18842))

### Bug fixes

- Avoid panics in environment finding via cycle detection ([#18828](https://github.com/astral-sh/uv/pull/18828))
- Enforce direct URL hashes for `pyproject.toml` dependencies ([#18786](https://github.com/astral-sh/uv/pull/18786))
- Error on `--locked` and `--frozen` when script lockfile is missing ([#18832](https://github.com/astral-sh/uv/pull/18832))
- Fix `uv export` extra resolution for workspace member and conflicting extras ([#18888](https://github.com/astral-sh/uv/pull/18888))
- Include conflicts defined in virtual workspace root ([#18886](https://github.com/astral-sh/uv/pull/18886))
- Recompute relative `exclude-newer` values during `uv tree --outdated` ([#18899](https://github.com/astral-sh/uv/pull/18899))
- Respect `--exclude-newer` in `uv tool list --outdated` ([#18861](https://github.com/astral-sh/uv/pull/18861))
- Sort by comparator to break specifier ties ([#18850](https://github.com/astral-sh/uv/pull/18850))
- Store relative timestamps in tool receipts ([#18901](https://github.com/astral-sh/uv/pull/18901))
- Track newly-activated extras when determining conflicts ([#18852](https://github.com/astral-sh/uv/pull/18852))
- Patch `Cargo.lock` in `uv-build` source distributions ([#18831](https://github.com/astral-sh/uv/pull/18831))

### Documentation

- Clarify that `--exclude-newer` compares artifact upload times ([#18830](https://github.com/astral-sh/uv/pull/18830))

## 0.11.3

Released on 2026-04-01.

### Enhancements

- Add progress bar for hashing phase in uv publish ([#18752](https://github.com/astral-sh/uv/pull/18752))
- Add support for ROCm 7.2 ([#18730](https://github.com/astral-sh/uv/pull/18730))
- Emit abi3t tags for every abi3 version ([#18777](https://github.com/astral-sh/uv/pull/18777))
- Expand `uv workspace metadata` with dependency information from the lock ([#18356](https://github.com/astral-sh/uv/pull/18356))
- Implement support for PEP 803 ([#18767](https://github.com/astral-sh/uv/pull/18767))
- Pretty-print platform in built wheel errors ([#18738](https://github.com/astral-sh/uv/pull/18738))
- Publish installers to `/installers/uv/latest` on the mirror ([#18725](https://github.com/astral-sh/uv/pull/18725))
- Show free-threaded Python in built-wheel errors ([#18740](https://github.com/astral-sh/uv/pull/18740))

### Preview features

- Add `--ignore` and `--ignore-until-fixed` to `uv audit` ([#18737](https://github.com/astral-sh/uv/pull/18737))

### Bug fixes

- Bump simple API cache ([#18797](https://github.com/astral-sh/uv/pull/18797))
- Don't drop `blake2b` hashes ([#18794](https://github.com/astral-sh/uv/pull/18794))
- Handle broken range request implementations ([#18780](https://github.com/astral-sh/uv/pull/18780))
- Remove `powerpc64-unknown-linux-gnu` from release build targets ([#18800](https://github.com/astral-sh/uv/pull/18800))
- Respect dependency metadata overrides in `uv pip check` ([#18742](https://github.com/astral-sh/uv/pull/18742))
- Support debug CPython ABI tags in environment compatibility ([#18739](https://github.com/astral-sh/uv/pull/18739))

### Documentation

- Document `false` opt-out for `exclude-newer-package` ([#18768](https://github.com/astral-sh/uv/pull/18768), [#18803](https://github.com/astral-sh/uv/pull/18803))

## 0.11.2

Released on 2026-03-26.

### Enhancements

- Add a dedicated Windows PE editing error ([#18710](https://github.com/astral-sh/uv/pull/18710))
- Make `uv self update` fetch the manifest from the mirror first ([#18679](https://github.com/astral-sh/uv/pull/18679))
- Use uv reqwest client for self update ([#17982](https://github.com/astral-sh/uv/pull/17982))
- Show `uv self update` success and failure messages with `--quiet` ([#18645](https://github.com/astral-sh/uv/pull/18645))

### Preview features

- Evaluate extras and groups when determining auditable packages ([#18511](https://github.com/astral-sh/uv/pull/18511))

### Bug fixes

- Skip redundant project configuration parsing for `uv run` ([#17890](https://github.com/astral-sh/uv/pull/17890))

## 0.11.1

Released on 2026-03-24.

### Bug fixes

- Add missing hash verification for `riscv64gc-unknown-linux-musl` ([#18686](https://github.com/astral-sh/uv/pull/18686))
- Fallback to direct download when direct URL streaming is unsupported ([#18688](https://github.com/astral-sh/uv/pull/18688))
- Revert treating 'Dynamic' values as case-insensitive ([#18692](https://github.com/astral-sh/uv/pull/18692))
- Remove torchdata from list of packages to source from the PyTorch index ([#18703](https://github.com/astral-sh/uv/pull/18703))
- Special-case `==` Python version request ranges ([#9697](https://github.com/astral-sh/uv/pull/9697))

### Documentation

- Cover `--python <dir>` in "Using arbitrary Python environments" ([#6457](https://github.com/astral-sh/uv/pull/6457))
- Fix version annotations for `PS_MODULE_PATH` and `UV_WORKING_DIR` ([#18691](https://github.com/astral-sh/uv/pull/18691))

## 0.11.0

Released on 2026-03-23.

### Breaking changes

This release includes changes to the networking stack used by uv. While we think that breakage will be rare, it is possible that these changes will result in the rejection of certificates previously trusted by uv so we have marked the change as breaking out of an abundance of caution.

The changes are largely driven by the upgrade of reqwest, which powers uv's HTTP clients, to [v0.13](https://seanmonstar.com/blog/reqwest-v013-rustls-default/) which included some breaking changes to TLS certificate verification.

The following changes are included:

- [`rustls-platform-verifier`](https://github.com/rustls/rustls-platform-verifier) is used instead of [`rustls-native-certs`](https://github.com/rustls/rustls-native-certs) and [`webpki`](https://github.com/rustls/webpki) for certificate verification
  
  **This change should have no effect unless you are using the `native-tls` option to enable reading system certificates.**
  
  `rustls-platform-verifier` delegates to the system for certificate validation (e.g., `Security.framework` on macOS) instead of eagerly loading certificates from the system and verifying them via `webpki`. The effects of this change will vary based on the operating system. In general, uv's certificate validation should now be more consistent with browsers and other native applications. However, this is the most likely cause of breaking changes in this release. Some previously failing certificate chains may succeed, and some previously accepted certificate chains may fail. In either case, we expect the validation to be more correct and welcome reports of regressions.
  
  In particular, because more responsibility for validating the certificate is transferred to your system's security library, some features like [CA constraints](https://support.apple.com/en-us/103255) or [revocation of certificates](https://en.wikipedia.org/wiki/Certificate_revocation) via OCSP and CRLs may now be used.
  
  This change should improve performance when using system certificate on macOS, as uv no longer needs to load all certificates from the keychain at startup.
- [`aws-lc`](https://github.com/aws/aws-lc) is used instead of `ring` for a cryptography backend
  
  There should not be breaking changes from this change. We expect this to expand support for certificate signature algorithms.
- `--native-tls` is deprecated in favor of a new `--system-certs` flag
  
  The `--native-tls` flag is still usable and has identical behavior to `--system-certs.`
  
  This change was made to reduce confusion about the TLS implementation uv uses. uv always uses `rustls` not `native-tls`.
- Building uv on x86-64 and i686 Windows requires NASM
  
  NASM is required by `aws-lc`. If not found on the system, a prebuilt blob provided by `aws-lc-sys` will be used.
  
  If you are not building uv from source, this change has no effect.
  
  See the [CONTRIBUTING](https://github.com/astral-sh/uv/blob/b6854d77bfd0cb78157fecaf8b30126c6f16bc11/CONTRIBUTING.md#setup) guide for details.
- Empty `SSL_CERT_FILE` values are ignored (for consistency with `SSL_CERT_DIR`)

See [#18550](https://github.com/astral-sh/uv/pull/18550) for details.

### Python

- Enable frame pointers for improved profiling on Linux x86-64 and aarch64

See the [python-build-standalone release notes](https://github.com/astral-sh/python-build-standalone/releases/20260320) for details.

### Enhancements

- Treat 'Dynamic' values as case-insensitive ([#18669](https://github.com/astral-sh/uv/pull/18669))
- Use a dedicated error for invalid cache control headers ([#18657](https://github.com/astral-sh/uv/pull/18657))
- Enable checksum verification in the generated installer script ([#18625](https://github.com/astral-sh/uv/pull/18625))

### Preview features

- Add `--service-format` and `--service-url` to `uv audit` ([#18571](https://github.com/astral-sh/uv/pull/18571))

### Performance

- Avoid holding flat index lock across indexes ([#18659](https://github.com/astral-sh/uv/pull/18659))

### Bug fixes

- Find the dynamic linker on the file system when sniffing binaries fails ([#18457](https://github.com/astral-sh/uv/pull/18457))
- Fix export of conflicting workspace members with dependencies ([#18666](https://github.com/astral-sh/uv/pull/18666))
- Respect installed settings in `uv tool list --outdated` ([#18586](https://github.com/astral-sh/uv/pull/18586))
- Treat paths originating as PEP 508 URLs which contain expanded variables as relative ([#18680](https://github.com/astral-sh/uv/pull/18680))
- Fix `uv export` for workspace member packages with conflicts ([#18635](https://github.com/astral-sh/uv/pull/18635))
- Continue to alternative authentication providers when the pyx store has no token ([#18425](https://github.com/astral-sh/uv/pull/18425))
- Use redacted URLs for log messages in cached client ([#18599](https://github.com/astral-sh/uv/pull/18599))

### Documentation

- Add details on Linux versions to the platform policy ([#18574](https://github.com/astral-sh/uv/pull/18574))
- Clarify `FLASH_ATTENTION_SKIP_CUDA_BUILD` guidance for `flash-attn` installs ([#18473](https://github.com/astral-sh/uv/pull/18473))
- Split the dependency bots page into two separate pages ([#18597](https://github.com/astral-sh/uv/pull/18597))
- Split the alternative indexes page into separate pages ([#18607](https://github.com/astral-sh/uv/pull/18607))

## 0.10.12

Released on 2026-03-19.

### Python

- Add pypy 3.11.15 ([#18468](https://github.com/astral-sh/uv/pull/18468))
- Add support for using Python 3.6 interpreters ([#18454](https://github.com/astral-sh/uv/pull/18454))

### Enhancements

- Include uv's target triple in version report ([#18520](https://github.com/astral-sh/uv/pull/18520))
- Allow comma separated values in `--no-emit-package` ([#18565](https://github.com/astral-sh/uv/pull/18565))

### Preview features

- Show `uv audit` in the CLI help ([#18540](https://github.com/astral-sh/uv/pull/18540))

### Bug fixes

- Improve reporting of managed interpreter symlinks in `uv python list` ([#18459](https://github.com/astral-sh/uv/pull/18459))
- Preserve end-of-line comments on previous entries when removing dependencies ([#18557](https://github.com/astral-sh/uv/pull/18557))
- Treat abi3 wheel Python version as a lower bound ([#18536](https://github.com/astral-sh/uv/pull/18536))
- Detect hard-float support on aarch64 kernels running armv7 userspace ([#18530](https://github.com/astral-sh/uv/pull/18530))

### Documentation

- Add Python 3.15 to supported versions ([#18552](https://github.com/astral-sh/uv/pull/18552))
- Adjust the PyPy note ([#18548](https://github.com/astral-sh/uv/pull/18548))
- Move Pyodide to Tier 2 in the Python support policy ([#18561](https://github.com/astral-sh/uv/pull/18561))
- Move Rust and Python version support out of the Platform support policy ([#18535](https://github.com/astral-sh/uv/pull/18535))
- Update Docker guide with changes from `uv-docker-example` ([#18558](https://github.com/astral-sh/uv/pull/18558))
- Update the Python version policy ([#18559](https://github.com/astral-sh/uv/pull/18559))

## 0.10.11

Released on 2026-03-16.

### Enhancements

- Fetch Ruff release metadata from an Astral mirror ([#18358](https://github.com/astral-sh/uv/pull/18358))
- Use PEP 639 license metadata for uv itself ([#16477](https://github.com/astral-sh/uv/pull/16477))

### Performance

- Improve distribution id performance ([#18486](https://github.com/astral-sh/uv/pull/18486))

### Bug fixes

- Allow `--project` to refer to a `pyproject.toml` directly and reduce to a warning on other files ([#18513](https://github.com/astral-sh/uv/pull/18513))
- Disable `SYSTEM_VERSION_COMPAT` when querying interpreters on macOS ([#18452](https://github.com/astral-sh/uv/pull/18452))
- Enforce available distributions for supported environments ([#18451](https://github.com/astral-sh/uv/pull/18451))
- Fix `uv sync --active` recreating active environments when `UV_PYTHON_INSTALL_DIR` is relative ([#18398](https://github.com/astral-sh/uv/pull/18398))

### Documentation

- Add missing `-o requirements.txt` in `uv pip compile` example ([#12308](https://github.com/astral-sh/uv/pull/12308))
- Link to organization security policy ([#18449](https://github.com/astral-sh/uv/pull/18449))
- Link to the AI policy in the contributing guide ([#18448](https://github.com/astral-sh/uv/pull/18448))
## 0.10.10

Released on 2026-03-13.

### Python

- Add CPython 3.15.0a7 ([#18403](https://github.com/astral-sh/uv/pull/18403))

### Enhancements

- Add `--outdated` flag to `uv tool list` ([#18318](https://github.com/astral-sh/uv/pull/18318))
- Add riscv64 musl target to build-release-binaries workflow ([#18228](https://github.com/astral-sh/uv/pull/18228))
- Fetch Ruff from an Astral mirror ([#18286](https://github.com/astral-sh/uv/pull/18286))
- Improve error handling for platform detection in Python downloads ([#18453](https://github.com/astral-sh/uv/pull/18453))
- Warn if `--project` directory does not exist ([#17714](https://github.com/astral-sh/uv/pull/17714))
- Warn when workspace member scripts are skipped due to missing build system ([#18389](https://github.com/astral-sh/uv/pull/18389))
- Update build backend versions used in `uv init` ([#18417](https://github.com/astral-sh/uv/pull/18417))
- Log explicit config file path in verbose output ([#18353](https://github.com/astral-sh/uv/pull/18353))
- Make `uv cache clear` an alias of `uv cache clean` ([#18420](https://github.com/astral-sh/uv/pull/18420))
- Reject invalid classifiers, warn on license classifiers in `uv_build` ([#18419](https://github.com/astral-sh/uv/pull/18419))

### Preview features

- Add links to `uv audit` output ([#18392](https://github.com/astral-sh/uv/pull/18392))
- Output/report formatting for `uv audit` ([#18193](https://github.com/astral-sh/uv/pull/18193))
- Switch to batched OSV queries for `uv audit` ([#18394](https://github.com/astral-sh/uv/pull/18394))

### Bug fixes

- Avoid sharing version metadata across indexes ([#18373](https://github.com/astral-sh/uv/pull/18373))
- Bump zlib-rs to 0.6.2 to fix panic on decompression of large wheels on Windows ([#18362](https://github.com/astral-sh/uv/pull/18362))
- Filter out unsupported environment wheels ([#18445](https://github.com/astral-sh/uv/pull/18445))
- Preserve absolute/relative paths in lockfiles ([#18176](https://github.com/astral-sh/uv/pull/18176))
- Recreate Python environments under `uv tool install --force` ([#18399](https://github.com/astral-sh/uv/pull/18399))
- Respect timestamp and other cache keys in cached environments ([#18396](https://github.com/astral-sh/uv/pull/18396))
- Simplify selected extra markers in `uv export` ([#18433](https://github.com/astral-sh/uv/pull/18433))
- Send pyx mint-token requests with a proper `Content-Type` ([#18334](https://github.com/astral-sh/uv/pull/18334))
- Fix Windows operating system and version reporting ([#18383](https://github.com/astral-sh/uv/pull/18383))

### Documentation

- Update the platform support policy with a tier 3 section including freebsd and 32-bit windows ([#18345](https://github.com/astral-sh/uv/pull/18345))

## 0.10.9

Released on 2026-03-06.

### Enhancements

- Add `fbgemm-gpu`, `fbgemm-gpu-genai`, `torchrec`, and `torchtune` to the PyTorch list ([#18338](https://github.com/astral-sh/uv/pull/18338))
- Add torchcodec to PyTorch List ([#18336](https://github.com/astral-sh/uv/pull/18336))
- Log the duration we took before erroring ([#18231](https://github.com/astral-sh/uv/pull/18231))
- Warn when using `uv_build` settings without `uv_build` ([#15750](https://github.com/astral-sh/uv/pull/15750))
- Add fallback to `/usr/lib/os-release` on Linux system lookup failure ([#18349](https://github.com/astral-sh/uv/pull/18349))
- Use `cargo auditable` to include SBOM in uv builds ([#18276](https://github.com/astral-sh/uv/pull/18276))

### Configuration

- Add an environment variable for `UV_VENV_RELOCATABLE` ([#18331](https://github.com/astral-sh/uv/pull/18331))

### Performance

- Avoid toml `Document` overhead ([#18306](https://github.com/astral-sh/uv/pull/18306))
- Use a single global workspace cache ([#18307](https://github.com/astral-sh/uv/pull/18307))

### Bug fixes

- Continue on trampoline job assignment failures ([#18291](https://github.com/astral-sh/uv/pull/18291))
- Handle the hard link limit gracefully instead of failing ([#17699](https://github.com/astral-sh/uv/pull/17699))
- Respect build constraints for workspace members ([#18350](https://github.com/astral-sh/uv/pull/18350))
- Revalidate editables and other dependencies in scripts ([#18328](https://github.com/astral-sh/uv/pull/18328))
- Support Python 3.13+ on Android ([#18301](https://github.com/astral-sh/uv/pull/18301))
- Support `cp3-none-any` ([#17064](https://github.com/astral-sh/uv/pull/17064))
- Skip tool environments with broken links to Python on Windows ([#17176](https://github.com/astral-sh/uv/pull/17176))

### Documentation

- Add documentation for common marker values ([#18327](https://github.com/astral-sh/uv/pull/18327))
- Improve documentation on virtual dependencies ([#18346](https://github.com/astral-sh/uv/pull/18346))

## 0.10.8

Released on 2026-03-03.

### Python

- Add CPython 3.10.20
- Add CPython 3.11.15
- Add CPython 3.12.13

### Enhancements

- Add Docker images based on Docker Hardened Images ([#18247](https://github.com/astral-sh/uv/pull/18247))
- Add resolver hint when `--exclude-newer` filters out all versions of a package ([#18217](https://github.com/astral-sh/uv/pull/18217))
- Configure a real retry minimum delay of 1s ([#18201](https://github.com/astral-sh/uv/pull/18201))
- Expand `uv_build` direct build compatibility ([#17902](https://github.com/astral-sh/uv/pull/17902))
- Fetch CPython from an Astral mirror by default ([#18207](https://github.com/astral-sh/uv/pull/18207))
- Download uv releases from an Astral mirror in installers by default ([#18191](https://github.com/astral-sh/uv/pull/18191))
- Add SBOM attestations to Docker images ([#18252](https://github.com/astral-sh/uv/pull/18252))
- Improve hint for installing meson-python when missing as build backend ([#15826](https://github.com/astral-sh/uv/pull/15826))

### Configuration

- Add `UV_INIT_BARE` environment variable for `uv init` ([#18210](https://github.com/astral-sh/uv/pull/18210))

### Bug fixes

- Prevent `uv tool upgrade` from installing excluded dependencies ([#18022](https://github.com/astral-sh/uv/pull/18022))
- Promote authentication policy when saving tool receipts ([#18246](https://github.com/astral-sh/uv/pull/18246))
- Respect exclusions in scripts ([#18269](https://github.com/astral-sh/uv/pull/18269))
- Retain default-branch Git SHAs in `pylock.toml` files ([#18227](https://github.com/astral-sh/uv/pull/18227))
- Skip installed Python check for URL dependencies ([#18211](https://github.com/astral-sh/uv/pull/18211))
- Respect constraints during `--upgrade` ([#18226](https://github.com/astral-sh/uv/pull/18226))
- Fix `uv tree` orphaned roots and premature deduplication ([#17212](https://github.com/astral-sh/uv/pull/17212))

### Documentation

- Mention cooldown and tweak inline script metadata in dependency bots documentation ([#18230](https://github.com/astral-sh/uv/pull/18230))
- Move cache prune in GitLab to `after_script` ([#18206](https://github.com/astral-sh/uv/pull/18206))

## 0.10.7

Released on 2026-02-27.

### Bug fixes

- Fix handling of junctions in Windows Containers on Windows ([#18192](https://github.com/astral-sh/uv/pull/18192))

### Enhancements

- Activate logging for middleware retries ([#18200](https://github.com/astral-sh/uv/pull/18200))
- Upload uv releases to a mirror ([#18159](https://github.com/astral-sh/uv/pull/18159))

## 0.10.6

Released on 2026-02-24.

### Bug fixes

- Apply lockfile marker normalization for fork markers ([#18116](https://github.com/astral-sh/uv/pull/18116))
- Fix Python version selection for scripts with a `requires-python` conflicting with `.python-version` ([#18097](https://github.com/astral-sh/uv/pull/18097))
- Preserve file permissions when using reflinks on Linux ([#18187](https://github.com/astral-sh/uv/pull/18187))

### Documentation

- Remove verbose documentation from optional dependencies help text ([#18180](https://github.com/astral-sh/uv/pull/18180))

## 0.10.5

Released on 2026-02-23.

### Enhancements

- Add hint when named index is found in a parent config file ([#18087](https://github.com/astral-sh/uv/pull/18087))
- Add warning for `uv lock --frozen` ([#17859](https://github.com/astral-sh/uv/pull/17859))
- Attempt to use reflinks by default on Linux ([#18117](https://github.com/astral-sh/uv/pull/18117))
- Fallback to hardlinks after reflink failure before copying ([#18104](https://github.com/astral-sh/uv/pull/18104))
- Filter `pylock.toml` wheels by tags and `requires-python` ([#18081](https://github.com/astral-sh/uv/pull/18081))
- Validate wheel filenames are normalized during `uv publish` ([#17783](https://github.com/astral-sh/uv/pull/17783))
- Fix message when `exclude-newer` invalidates the lock file ([#18100](https://github.com/astral-sh/uv/pull/18100))
- Change the missing files log level to debug ([#18075](https://github.com/astral-sh/uv/pull/18075))

### Performance

- Improve performance of repeated conflicts with an extra ([#18094](https://github.com/astral-sh/uv/pull/18094))

### Bug fixes

- Fix `--no-emit-workspace` with `--all-packages` on single-member workspaces ([#18098](https://github.com/astral-sh/uv/pull/18098))
- Fix `UV_NO_DEFAULT_GROUPS` rejecting truthy values like `1` ([#18057](https://github.com/astral-sh/uv/pull/18057))
- Fix iOS detection ([#17973](https://github.com/astral-sh/uv/pull/17973))
- Propagate project-level conflicts to package extras ([#18096](https://github.com/astral-sh/uv/pull/18096))
- Use a global build concurrency semaphore ([#18054](https://github.com/astral-sh/uv/pull/18054))

### Documentation

- Update documentation heading for environment variable files ([#18122](https://github.com/astral-sh/uv/pull/18122))
- Fix comment about `uv export` formats ([#17900](https://github.com/astral-sh/uv/pull/17900))
- Make it clear that Windows is supported in user- and system- level configuration docs ([#18106](https://github.com/astral-sh/uv/pull/18106))

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


