# Changelog

<!-- prettier-ignore-start -->


## 0.8.24

Released on 2025-10-06.

### Enhancements

- Emit a message on `cache clean` and `prune` when lock is held ([#16138](https://github.com/astral-sh/uv/pull/16138))
- Add `--force` flag for `uv cache prune` ([#16137](https://github.com/astral-sh/uv/pull/16137))

### Documentation

- Fix example of bumping beta version without patch bump ([#16132](https://github.com/astral-sh/uv/pull/16132))

## 0.8.23

Released on 2025-10-03.

### Enhancements

- Build `s390x` on stable Rust compiler version ([#16082](https://github.com/astral-sh/uv/pull/16082))
- Add `UV_SKIP_WHEEL_FILENAME_CHECK` to allow installing invalid wheels ([#16046](https://github.com/astral-sh/uv/pull/16046))

### Bug fixes

- Avoid rejecting already-installed URL distributions with `--no-sources` ([#16094](https://github.com/astral-sh/uv/pull/16094))
- Confirm that the directory name is a valid Python install key during managed check ([#16080](https://github.com/astral-sh/uv/pull/16080))
- Ignore origin when comparing installed tools ([#16055](https://github.com/astral-sh/uv/pull/16055))
- Make cache control lookups robust to username ([#16088](https://github.com/astral-sh/uv/pull/16088))
- Re-order lock validation checks by severity ([#16045](https://github.com/astral-sh/uv/pull/16045))
- Remove tracking of inferred dependency conflicts ([#15909](https://github.com/astral-sh/uv/pull/15909))
- Respect `--no-color` on the CLI ([#16044](https://github.com/astral-sh/uv/pull/16044))
- Deduplicate marker-specific dependencies in `uv pip tree` output ([#16078](https://github.com/astral-sh/uv/pull/16078))

### Documentation

- Document transparent x86_64 emulation on aarch64 ([#16041](https://github.com/astral-sh/uv/pull/16041))
- Document why we ban URLs from index dependencies ([#15929](https://github.com/astral-sh/uv/pull/15929))
- Fix rendering of `_CONDA_ROOT` in reference ([#16114](https://github.com/astral-sh/uv/pull/16114))
- Windows arm64 and Linux RISC-V64 are Tier 2 supported ([#16027](https://github.com/astral-sh/uv/pull/16027))

## 0.8.22

Released on 2025-09-23.

### Python

- Upgrade Pyodide to 0.28.3 ([#15999](https://github.com/astral-sh/uv/pull/15999))

### Security

- Upgrade `astral-tokio-tar` to 0.5.5 which [hardens tar archive extraction](https://github.com/astral-sh/tokio-tar/security/advisories/GHSA-3wgq-wrwc-vqmv) ([#16004](https://github.com/astral-sh/uv/pull/16004))

## 0.8.21

Released on 2025-09-23.

### Enhancements

- Refresh lockfile when `--refresh` is provided ([#15994](https://github.com/astral-sh/uv/pull/15994))

### Preview features

- Add support for S3 request signing ([#15925](https://github.com/astral-sh/uv/pull/15925))

## 0.8.20

Released on 2025-09-22.

### Enhancements

- Add `--force` flag for `uv cache clean` ([#15992](https://github.com/astral-sh/uv/pull/15992))
- Improve resolution errors with proxied packages ([#15200](https://github.com/astral-sh/uv/pull/15200))

### Preview features

- Allow upgrading pre-release versions of the same minor Python version ([#15959](https://github.com/astral-sh/uv/pull/15959))

### Bug fixes

- Hide `freethreaded+debug` Python downloads in `uv python list` ([#15985](https://github.com/astral-sh/uv/pull/15985))
- Retain the cache lock and temporary caches during `uv run` and `uvx` ([#15990](https://github.com/astral-sh/uv/pull/15990))

### Documentation

- Add `package` level conflicts to the conflicting dependencies docs ([#15963](https://github.com/astral-sh/uv/pull/15963))
- Document pyodide support ([#15962](https://github.com/astral-sh/uv/pull/15962))
- Document support for free-threaded and debug Python versions ([#15961](https://github.com/astral-sh/uv/pull/15961))
- Expand the contribution docs on issue selection ([#15966](https://github.com/astral-sh/uv/pull/15966))
- Tweak title for viewing version in project guide ([#15964](https://github.com/astral-sh/uv/pull/15964))

## 0.8.19

Released on 2025-09-19.

### Python

- Add CPython 3.14.0rc3
- Upgrade OpenSSL to 3.5.3

See the [python-build-standalone release notes](https://github.com/astral-sh/python-build-standalone/releases/tag/20250918) for more details.

### Bug fixes

- Make `uv cache clean` parallel process safe ([#15888](https://github.com/astral-sh/uv/pull/15888))
- Fix implied `platform_machine` marker for `win_arm64` platform tag ([#15921](https://github.com/astral-sh/uv/pull/15921))

## 0.8.18

Released on 2025-09-17.

### Enhancements

- Add PyG packages to torch backend ([#15911](https://github.com/astral-sh/uv/pull/15911))
- Add handling for unnamed conda environments in base environment detection ([#15681](https://github.com/astral-sh/uv/pull/15681))
- Allow selection of debug build interpreters ([#11520](https://github.com/astral-sh/uv/pull/11520))
- Improve `uv init` defaults for native build backend cache keys ([#15705](https://github.com/astral-sh/uv/pull/15705))
- Error when `pyproject.toml` target does not exist for dependency groups ([#15831](https://github.com/astral-sh/uv/pull/15831))
- Infer check URL from publish URL when known ([#15886](https://github.com/astral-sh/uv/pull/15886))
- Support Gitlab CI/CD as a trusted publisher ([#15583](https://github.com/astral-sh/uv/pull/15583))
- Add GraalPy 25.0.0 with support for Python 3.12 ([#15900](https://github.com/astral-sh/uv/pull/15900))
- Add `--no-clear` to `uv venv` to disable removal prompts ([#15795](https://github.com/astral-sh/uv/pull/15795))
- Add conflict detection between `--only-group` and `--extra` flags ([#15788](https://github.com/astral-sh/uv/pull/15788))
- Allow `[project]` to be missing from a `pyproject.toml` ([#14113](https://github.com/astral-sh/uv/pull/14113))
- Always treat conda environments named `base` and `root` as base environments ([#15682](https://github.com/astral-sh/uv/pull/15682))
- Improve log message when direct build for `uv_build` is skipped ([#15898](https://github.com/astral-sh/uv/pull/15898))
- Log when the cache is disabled ([#15828](https://github.com/astral-sh/uv/pull/15828))
- Show pyx organization name after authenticating ([#15823](https://github.com/astral-sh/uv/pull/15823))
- Use `_CONDA_ROOT` to detect Conda base environments ([#15680](https://github.com/astral-sh/uv/pull/15680))
- Include blake2b hash in `uv publish` upload form ([#15794](https://github.com/astral-sh/uv/pull/15794))
- Fix misleading debug message when removing environments in `uv sync` ([#15881](https://github.com/astral-sh/uv/pull/15881))

### Deprecations

- Deprecate `tool.uv.dev-dependencies` ([#15469](https://github.com/astral-sh/uv/pull/15469))
- Revert "feat(ci): build loongarch64 binaries in CI (#15387)" ([#15820](https://github.com/astral-sh/uv/pull/15820))

### Preview features

- Propagate preview flag to client for `native-auth` feature ([#15872](https://github.com/astral-sh/uv/pull/15872))
- Store native credentials for realms with the https scheme stripped ([#15879](https://github.com/astral-sh/uv/pull/15879))
- Use the root index URL when retrieving credentials from the native store ([#15873](https://github.com/astral-sh/uv/pull/15873))

### Bug fixes

- Fix `uv sync --no-sources` not switching from editable to registry installations ([#15234](https://github.com/astral-sh/uv/pull/15234))
- Avoid display of an empty string when a path is the working directory ([#15897](https://github.com/astral-sh/uv/pull/15897))
- Allow cached environment reuse with `@latest` ([#15827](https://github.com/astral-sh/uv/pull/15827))
- Allow escaping spaces in --env-file handling ([#15815](https://github.com/astral-sh/uv/pull/15815))
- Avoid ANSI codes in debug! messages ([#15843](https://github.com/astral-sh/uv/pull/15843))
- Improve BSD tag construction ([#15829](https://github.com/astral-sh/uv/pull/15829))
- Include SHA when listing lockfile changes ([#15817](https://github.com/astral-sh/uv/pull/15817))
- Invert the logic for determining if a path is a base conda environment ([#15679](https://github.com/astral-sh/uv/pull/15679))
- Load credentials for explicit members when lowering ([#15844](https://github.com/astral-sh/uv/pull/15844))
- Re-add `triton` as a torch backend package ([#15910](https://github.com/astral-sh/uv/pull/15910))
- Respect `UV_INSECURE_NO_ZIP_VALIDATION=1` in duplicate header errors ([#15912](https://github.com/astral-sh/uv/pull/15912))

### Documentation

- Add GitHub Actions to PyPI trusted publishing example ([#15753](https://github.com/astral-sh/uv/pull/15753))
- Add Coiled integration documentation ([#14430](https://github.com/astral-sh/uv/pull/14430))
- Add verbose output to the getting help section ([#15915](https://github.com/astral-sh/uv/pull/15915))
- Document `NO_PROXY` support ([#15816](https://github.com/astral-sh/uv/pull/15816))
- Document cache-keys for native build backends ([#15811](https://github.com/astral-sh/uv/pull/15811))
- Add documentation for dependency group `requires-python` ([#14282](https://github.com/astral-sh/uv/pull/14282))

## 0.8.17

Released on 2025-09-10.

### Enhancements

- Improve error message for HTTP validation in auth services ([#15768](https://github.com/astral-sh/uv/pull/15768))
- Respect `PYX_API_URL` when suggesting `uv auth login` on 401 ([#15774](https://github.com/astral-sh/uv/pull/15774))
- Add pyx as a supported PyTorch index URL ([#15769](https://github.com/astral-sh/uv/pull/15769))

### Bug fixes

- Avoid initiating login flow for invalid API keys ([#15773](https://github.com/astral-sh/uv/pull/15773))
- Do not search for a password for requests with a token attached already ([#15772](https://github.com/astral-sh/uv/pull/15772))
- Filter pre-release Python versions in `uv init --script` ([#15747](https://github.com/astral-sh/uv/pull/15747))

## 0.8.16

### Enhancements

- Allow `--editable` to override `editable = false` annotations ([#15712](https://github.com/astral-sh/uv/pull/15712))
- Allow `editable = false` for workspace sources ([#15708](https://github.com/astral-sh/uv/pull/15708))
- Show a dedicated error for virtual environments in source trees on build ([#15748](https://github.com/astral-sh/uv/pull/15748))
- Support Android platform tags ([#15646](https://github.com/astral-sh/uv/pull/15646))
- Support iOS platform tags ([#15640](https://github.com/astral-sh/uv/pull/15640))
- Support scripts with inline metadata in `--with-requirements` and `--requirements` ([#12763](https://github.com/astral-sh/uv/pull/12763))

### Preview features

- Support `--no-project` in `uv format` ([#15572](https://github.com/astral-sh/uv/pull/15572))
- Allow `uv format` in unmanaged projects ([#15553](https://github.com/astral-sh/uv/pull/15553))

### Bug fixes

- Avoid erroring when `match-runtime` target is optional ([#15671](https://github.com/astral-sh/uv/pull/15671))
- Ban empty usernames and passwords in `uv auth` ([#15743](https://github.com/astral-sh/uv/pull/15743))
- Error early for parent path in build backend ([#15733](https://github.com/astral-sh/uv/pull/15733))
- Retry on IO errors during HTTP/2 streaming ([#15675](https://github.com/astral-sh/uv/pull/15675))
- Support recursive requirements and constraints inclusion ([#15657](https://github.com/astral-sh/uv/pull/15657))
- Use token store credentials for `uv publish` ([#15759](https://github.com/astral-sh/uv/pull/15759))
- Fix virtual environment activation script compatibility with latest nushell ([#15272](https://github.com/astral-sh/uv/pull/15272))
- Skip Python interpreters that cannot be queried with permission errors ([#15685](https://github.com/astral-sh/uv/pull/15685))

### Documentation

- Clarify that `uv auth` commands take a URL ([#15664](https://github.com/astral-sh/uv/pull/15664))
- Improve the CLI help for options that accept requirements files ([#15706](https://github.com/astral-sh/uv/pull/15706))
- Adds example for caching for managed Python downloads in Docker builds ([#15689](https://github.com/astral-sh/uv/pull/15689))

## 0.8.15

### Python

- Upgrade SQLite 3.50.4 in CPython builds for [CVE-2025-6965](https://github.com/advisories/GHSA-2m69-gcr7-jv3q) (see also [python/cpython#137134](https://github.com/python/cpython/issues/137134))

### Enhancements

- Add `uv auth` commands for credential management ([#15570](https://github.com/astral-sh/uv/pull/15570))
- Add pyx support to `uv auth` commands ([#15636](https://github.com/astral-sh/uv/pull/15636))
- Add `uv tree --show-sizes` to show package sizes ([#15531](https://github.com/astral-sh/uv/pull/15531))
- Add `--python-platform riscv64-unknown-linux` ([#15630](https://github.com/astral-sh/uv/pull/15630))
- Add `--python-platform` to `uv run` and `uv tool` ([#15515](https://github.com/astral-sh/uv/pull/15515))
- Add `uv publish --dry-run` ([#15638](https://github.com/astral-sh/uv/pull/15638))
- Add zstandard support for wheels ([#15645](https://github.com/astral-sh/uv/pull/15645))
- Allow registries to pre-provide core metadata ([#15644](https://github.com/astral-sh/uv/pull/15644))
- Retry streaming Python and binary download errors ([#15567](https://github.com/astral-sh/uv/pull/15567))

### Bug fixes

- Fix settings rendering for `extra-build-dependencies` ([#15622](https://github.com/astral-sh/uv/pull/15622))
- Skip non-existent directories in bytecode compilation ([#15608](https://github.com/astral-sh/uv/pull/15608))

### Error messages

- Add error trace to invalid package format ([#15626](https://github.com/astral-sh/uv/pull/15626))

## 0.8.14

### Python

- Add managed CPython distributions for aarch64 musl

### Enhancements

- Add `--python-platform` to `uv pip check` ([#15486](https://github.com/astral-sh/uv/pull/15486))
- Add an environment variable for `UV_ISOLATED` ([#15428](https://github.com/astral-sh/uv/pull/15428))
- Add logging to the uv build backend ([#15533](https://github.com/astral-sh/uv/pull/15533))
- Allow more trailing null bytes in zip files ([#15452](https://github.com/astral-sh/uv/pull/15452))
- Allow pinning managed Python versions to specific build versions ([#15314](https://github.com/astral-sh/uv/pull/15314))
- Cache PyTorch wheels by default ([#15481](https://github.com/astral-sh/uv/pull/15481))
- Reject already-installed wheels that don't match the target platform ([#15484](https://github.com/astral-sh/uv/pull/15484))
- Add `--no-install-local` option to `uv sync`, `uv add` and `uv export`  ([#15328](https://github.com/astral-sh/uv/pull/15328))
- Include cycle error message in `uv pip` CLI ([#15453](https://github.com/astral-sh/uv/pull/15453))

### Preview features

- Fix format of `{version}` on `uv format` failure ([#15527](https://github.com/astral-sh/uv/pull/15527))
- Lock during installs in `uv format` to prevent races ([#15551](https://github.com/astral-sh/uv/pull/15551))
- Respect `--project` in `uv format` ([#15438](https://github.com/astral-sh/uv/pull/15438))
- Run `uv format` in the project root ([#15440](https://github.com/astral-sh/uv/pull/15440))

### Configuration

- Add file-to-CLI overrides for build isolation configuration ([#15437](https://github.com/astral-sh/uv/pull/15437))
- Add file-to-CLI overrides for reinstall configuration ([#15426](https://github.com/astral-sh/uv/pull/15426))

### Performance

- Cache `WHEEL` and `METADATA` reads in installed distributions ([#15489](https://github.com/astral-sh/uv/pull/15489))

### Bug fixes

- Avoid erroring when creating `venv` in current working directory ([#15537](https://github.com/astral-sh/uv/pull/15537))
- Avoid introducing unnecessary system dependency on CUDA ([#15449](https://github.com/astral-sh/uv/pull/15449))
- Clear discovered site packages when creating virtual environment ([#15522](https://github.com/astral-sh/uv/pull/15522))
- Read index credentials from the environment during `uv publish` checks ([#15545](https://github.com/astral-sh/uv/pull/15545))
- Refuse to remove non-virtual environments in `uv venv` ([#15538](https://github.com/astral-sh/uv/pull/15538))
- Stop setting `CLICOLOR_FORCE=1` when calling build backends ([#15472](https://github.com/astral-sh/uv/pull/15472))
- Support file or directory removal for Windows symlinks ([#15543](https://github.com/astral-sh/uv/pull/15543))

### Documentation

- Fix GitHub guide highlight lines ([#15443](https://github.com/astral-sh/uv/pull/15443))
- Move Resolver to new Internals section in the Reference ([#15465](https://github.com/astral-sh/uv/pull/15465))
- Split the "Authentication" page into sections ([#15575](https://github.com/astral-sh/uv/pull/15575))
- Update uninstall docs to mention `uvw.exe` needs to be removed ([#15536](https://github.com/astral-sh/uv/pull/15536))

## 0.8.13

### Enhancements

- Add `--no-install-*` arguments to `uv add` ([#15375](https://github.com/astral-sh/uv/pull/15375))
- Initialize Git prior to reading author in `uv init` ([#15377](https://github.com/astral-sh/uv/pull/15377))
- Add CUDA 129 to available torch backends ([#15416](https://github.com/astral-sh/uv/pull/15416))
- Update Pyodide to 0.28.2 ([#15385](https://github.com/astral-sh/uv/pull/15385))

### Preview features

- Add an experimental `uv format` command ([#15017](https://github.com/astral-sh/uv/pull/15017))
- Allow version specifiers in `extra-build-dependencies` if match-runtime is explicitly `false` ([#15420](https://github.com/astral-sh/uv/pull/15420))

### Bug fixes

- Add `triton` to `torch-backend` manifest ([#15405](https://github.com/astral-sh/uv/pull/15405))
- Avoid panicking when resolver returns stale distributions ([#15389](https://github.com/astral-sh/uv/pull/15389))
- Fix `uv_build` wheel hashes ([#15400](https://github.com/astral-sh/uv/pull/15400))
- Treat `--upgrade-package` on the command-line as overriding `upgrade = false` in configuration ([#15395](https://github.com/astral-sh/uv/pull/15395))
- Restore DockerHub publishing ([#15381](https://github.com/astral-sh/uv/pull/15381))

## 0.8.12

### Python

- Add 3.13.7
- Improve performance of zstd in Python 3.14

See the [python-build-standalone release notes](https://github.com/astral-sh/python-build-standalone/releases/tag/20250818) for details.

### Enhancements

- Add an `aarch64-pc-windows-msvc` target for `python-platform` ([#15347](https://github.com/astral-sh/uv/pull/15347))
- Add fallback parent process detection to `uv tool update-shell` ([#15356](https://github.com/astral-sh/uv/pull/15356))
- Install non-build-isolation packages in a second phase ([#15306](https://github.com/astral-sh/uv/pull/15306))
- Add hint when virtual environments are included in source distributions ([#15202](https://github.com/astral-sh/uv/pull/15202))
- Add Docker images derived from `buildpack-deps:trixie`, `debian:trixie-slim`, `alpine:3.22` ([#15351](https://github.com/astral-sh/uv/pull/15351))

### Bug fixes

- Reject already-installed wheels built with outdated settings ([#15289](https://github.com/astral-sh/uv/pull/15289))
- Skip interpreters that are not found on query ([#15315](https://github.com/astral-sh/uv/pull/15315))
- Handle dotted package names in script path resolution ([#15300](https://github.com/astral-sh/uv/pull/15300))
- Reject `match-runtime = true` for dynamic packages ([#15292](https://github.com/astral-sh/uv/pull/15292))

### Documentation

- Document improvements to build-isolation setups ([#15326](https://github.com/astral-sh/uv/pull/15326))
- Fix reference documentation recommendation to use `uv cache clean` instead of `clear` ([#15313](https://github.com/astral-sh/uv/pull/15313))

## 0.8.11

### Python

- Add Python 3.14.0rc2
- Update Pyodide to 0.28.1

### Enhancements

- Add Debian 13 trixie to published Docker images ([#15269](https://github.com/astral-sh/uv/pull/15269))
- Add `extra-build-dependencies` hint for any missing module on build failure ([#15252](https://github.com/astral-sh/uv/pull/15252))
- Make 'v' prefix cyan in overlap warnings ([#15259](https://github.com/astral-sh/uv/pull/15259))

### Bug fixes

- Fix missing uv version in extended Docker image tags ([#15263](https://github.com/astral-sh/uv/pull/15263))
- Persist cache info when re-installing cached wheels ([#15274](https://github.com/astral-sh/uv/pull/15274))

### Rust API

- Allow passing custom `reqwest` clients to `RegistryClient` ([#15281](https://github.com/astral-sh/uv/pull/15281))

## 0.8.10

### Python

- Add support for installing Pyodide versions ([#14518](https://github.com/astral-sh/uv/pull/14518))

### Enhancements

- Allow Python requests with missing segments, e.g., just `aarch64` ([#14399](https://github.com/astral-sh/uv/pull/14399))

### Preview

- Move warnings for conflicting modules into preview ([#15253](https://github.com/astral-sh/uv/pull/15253))

## 0.8.9

### Enhancements

- Add `--reinstall` flag to `uv python upgrade` ([#15194](https://github.com/astral-sh/uv/pull/15194))

### Bug fixes

- Include build settings in cache key for registry source distribution lookups ([#15225](https://github.com/astral-sh/uv/pull/15225))
- Avoid creating bin links on `uv python upgrade` if they don't already exist ([#15192](https://github.com/astral-sh/uv/pull/15192))
- Respect system proxies on macOS and Windows ([#15221](https://github.com/astral-sh/uv/pull/15221))

### Documentation

- Add the 3.14 classifier ([#15187](https://github.com/astral-sh/uv/pull/15187))

## 0.8.8

### Bug fixes

- Fix `find_uv_bin` compatibility with Python <3.10 ([#15177](https://github.com/astral-sh/uv/pull/15177))

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


