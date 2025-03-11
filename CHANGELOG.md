# Changelog

<!-- prettier-ignore-start -->


## 0.6.6

### Python

- Add support for dynamic musl Python distributions on x86-64 Linux ([#12121](https://github.com/astral-sh/uv/pull/12121))
- Allow the experimental JIT to be enabled at runtime on Python 3.13 and 3.14 on Linux
- Upgrade the build toolchain to LLVM 20, improving performance

See the [`python-build-standalone` release notes](https://github.com/astral-sh/python-build-standalone/releases/tag/20250311) for more details.

### Enhancements

- Add `--marker` flag to `uv add` ([#12012](https://github.com/astral-sh/uv/pull/12012))
- Allow overriding module name for uv build backend ([#11884](https://github.com/astral-sh/uv/pull/11884))
- Sync latest Python releases ([#12120](https://github.com/astral-sh/uv/pull/12120))
- Use 'Upload' instead of 'Download' in publish reporter ([#12029](https://github.com/astral-sh/uv/pull/12029))
- Add `[index].authenticate` allowing authentication to be required on an index ([#11896](https://github.com/astral-sh/uv/pull/11896))
- Add support for Windows legacy scripts in `uv tool run` ([#12079](https://github.com/astral-sh/uv/pull/12079))
- Propagate conflicting dependency groups when using `include-group` ([#12005](https://github.com/astral-sh/uv/pull/12005))
- Show ambiguous requirements when `uv add` failed ([#12106](https://github.com/astral-sh/uv/pull/12106))

### Performance

- Cache workspace discovery ([#12096](https://github.com/astral-sh/uv/pull/12096))
- Insert dependencies into fork state prior to fetching metadata ([#12057](https://github.com/astral-sh/uv/pull/12057))
- Remove some allocations from `uv-auth` ([#12077](https://github.com/astral-sh/uv/pull/12077))

### Bug fixes

- Avoid considering `PATH` updated when the `export` is commented in the shellrc ([#12043](https://github.com/astral-sh/uv/pull/12043))
- Fix `uv publish` retry on network failures ([#12041](https://github.com/astral-sh/uv/pull/12041))
- Use a sized stream in `uv publish` to comply with WSGI PyPI server constraints ([#12111](https://github.com/astral-sh/uv/pull/12111))

### Preview features

- Fix `uv_build` invocation ([#12058](https://github.com/astral-sh/uv/pull/12058))

### Documentation

- Quote versions string in `python-versions.md` ([#12112](https://github.com/astral-sh/uv/pull/12112))
- Fix tool concept page headings ([#12053](https://github.com/astral-sh/uv/pull/12053))
- Update the `[index].authenticate` docs ([#12102](https://github.com/astral-sh/uv/pull/12102))
- Update versioning policy ([#11666](https://github.com/astral-sh/uv/pull/11666))

## 0.6.5

### Enhancements

- Allow `--constraints` and `--overrides` in `uvx` ([#10207](https://github.com/astral-sh/uv/pull/10207))
- Allow overrides in `satisfies` check for `uv tool run` ([#11994](https://github.com/astral-sh/uv/pull/11994))
- Allow users to set `package = true` on `tool.uv.sources` ([#12014](https://github.com/astral-sh/uv/pull/12014))
- Add support for Windows legacy scripts via `uv run` ([#11888](https://github.com/astral-sh/uv/pull/11888))
- Return error when running uvx with a `.py` script ([#11623](https://github.com/astral-sh/uv/pull/11623))
- Warn user on use of `uvx run` ([#11992](https://github.com/astral-sh/uv/pull/11992))

### Configuration

- Add `NO_BUILD` and `NO_BUILD_PACKAGE` environment variables ([#11968](https://github.com/astral-sh/uv/pull/11968))

### Performance

- Allow overrides in all satisfies checks ([#11995](https://github.com/astral-sh/uv/pull/11995))
- Respect markers on constraints when validating current environment ([#11976](https://github.com/astral-sh/uv/pull/11976))

### Bug fixes

- Compare major-minor specifiers when filtering interpreters ([#11952](https://github.com/astral-sh/uv/pull/11952))
- Fix system site packages detection default ([#11956](https://github.com/astral-sh/uv/pull/11956))
- Invalidate lockfile when empty dependency groups are added or removed ([#12010](https://github.com/astral-sh/uv/pull/12010))
- Remove prepended sys.path ([#11954](https://github.com/astral-sh/uv/pull/11954))
- Fix PyPy Python version label ([#11965](https://github.com/astral-sh/uv/pull/11965))
- Fix error message suggesting `--user` instead of `--username` ([#11947](https://github.com/astral-sh/uv/pull/11947))

### Preview

- Move the uv build backend into a separate, minimal `uv_build` package ([#11446](https://github.com/astral-sh/uv/pull/11446))

## 0.6.4

### Enhancements

- Upgrade pypy3.10 to v7.3.19 ([#11814](https://github.com/astral-sh/uv/pull/11814))
- Allow configuring log verbosity from the CLI (i.e., `-vvv`) ([#11758](https://github.com/astral-sh/uv/pull/11758))
- Warn when duplicate index names found in single file ([#11824](https://github.com/astral-sh/uv/pull/11824))

### Bug fixes

- Always store registry index on resolution packages ([#11815](https://github.com/astral-sh/uv/pull/11815))
- Avoid error on relative paths in `uv tool uninstall` ([#11889](https://github.com/astral-sh/uv/pull/11889))
- Avoid silently dropping errors in directory enumeration ([#11890](https://github.com/astral-sh/uv/pull/11890))
- Disable interactive git terminal prompts during fetches ([#11744](https://github.com/astral-sh/uv/pull/11744))
- Discover Windows registry (PEP 514) Python versions across 32/64-bit ([#11801](https://github.com/astral-sh/uv/pull/11801))
- Don't panic on Ctrl-C in confirm prompt ([#11706](https://github.com/astral-sh/uv/pull/11706))
- Fix non-directory in workspace on Windows ([#11833](https://github.com/astral-sh/uv/pull/11833))
- Make interpreter caching robust to OS upgrades ([#11875](https://github.com/astral-sh/uv/pull/11875))
- Respect `include-system-site-packages` in layered environments ([#11873](https://github.com/astral-sh/uv/pull/11873))
- Suggest `uv tool update-shell` in PowerShell ([#11846](https://github.com/astral-sh/uv/pull/11846))
- Update code page to `65001` before setting environment variables in virtual environments ([#11831](https://github.com/astral-sh/uv/pull/11831))
- Use hash instead of full wheel name in wheels bucket ([#11738](https://github.com/astral-sh/uv/pull/11738))
- Fix version string truncation while generating cache_key ([#11830](https://github.com/astral-sh/uv/pull/11830))
- Explicitly handle ctrl-c in confirmation prompt instead of using a signal handler ([#11897](https://github.com/astral-sh/uv/pull/11897))

### Performance

- Avoid cloning to string when creating cache path ([#11772](https://github.com/astral-sh/uv/pull/11772))
- Avoid redundant clones in version containment check ([#11767](https://github.com/astral-sh/uv/pull/11767))
- Avoid string allocation when enumerating tool names ([#11910](https://github.com/astral-sh/uv/pull/11910))
- Avoid using owned `String` for package name constructors ([#11768](https://github.com/astral-sh/uv/pull/11768))
- Avoid using owned `String` in deserializers ([#11764](https://github.com/astral-sh/uv/pull/11764))
- Migrate to `zlib-rs` (again) ([#11894](https://github.com/astral-sh/uv/pull/11894))
- Remove unnecessary clones when adding package names ([#11771](https://github.com/astral-sh/uv/pull/11771))
- Skip unquote allocation for non-quoted strings ([#11813](https://github.com/astral-sh/uv/pull/11813))
- Use `SmallString` for filenames and URLs ([#11765](https://github.com/astral-sh/uv/pull/11765))
- Use a Boxed slice for version specifiers ([#11766](https://github.com/astral-sh/uv/pull/11766))
- Use matches over contains for extra value parsing ([#11770](https://github.com/astral-sh/uv/pull/11770))

### Documentation

- Avoid fallback to PyPI in mixed CPU/CUDA example ([#11115](https://github.com/astral-sh/uv/pull/11115))
- Docs: Clarify that setting cache-keys overrides defaults ([#11895](https://github.com/astral-sh/uv/pull/11895))
- Document our MSRV policy ([#11898](https://github.com/astral-sh/uv/pull/11898))
- Fix reference to macOS cache path ([#11845](https://github.com/astral-sh/uv/pull/11845))
- Fix typo in `no_default_groups` documentation and changelog ([#11928](https://github.com/astral-sh/uv/pull/11928))
- Update the "Locking and syncing" page ([#11647](https://github.com/astral-sh/uv/pull/11647))
- Update alternative indexes documentation to use new interface ([#10826](https://github.com/astral-sh/uv/pull/10826))

## 0.6.3

### Enhancements

- Allow quotes around command-line options in `requirement.txt files` ([#11644](https://github.com/astral-sh/uv/pull/11644))
- Initialize PEP 723 script in `uv lock --script` ([#11717](https://github.com/astral-sh/uv/pull/11717))

### Configuration

- Accept multiple `.env` files in `UV_ENV_FILE` ([#11665](https://github.com/astral-sh/uv/pull/11665))

### Performance

- Reduce overhead in converting resolutions ([#11660](https://github.com/astral-sh/uv/pull/11660))
- Use `SmallString` on `Hashes` ([#11756](https://github.com/astral-sh/uv/pull/11756))
- Use a `Box` for `Yanked` on `File` ([#11755](https://github.com/astral-sh/uv/pull/11755))
- Use a `SmallString` for the `Yanked` enum ([#11715](https://github.com/astral-sh/uv/pull/11715))
- Use boxed slices for hash vector ([#11714](https://github.com/astral-sh/uv/pull/11714))
- Use install concurrency for bytecode compilation too ([#11615](https://github.com/astral-sh/uv/pull/11615))

### Bug fixes

- Avoid installing duplicate dependencies across conflicting groups ([#11653](https://github.com/astral-sh/uv/pull/11653))
- Check subdirectory existence after cache heal ([#11719](https://github.com/astral-sh/uv/pull/11719))
- Include uppercase platforms for Windows wheels ([#11681](https://github.com/astral-sh/uv/pull/11681))
- Respect existing PEP 723 script settings in `uv add` ([#11716](https://github.com/astral-sh/uv/pull/11716))
- Reuse refined interpreter to create tool environment ([#11680](https://github.com/astral-sh/uv/pull/11680))
- Skip removed directories during bytecode compilation ([#11633](https://github.com/astral-sh/uv/pull/11633))
- Support conflict markers in `uv export` ([#11643](https://github.com/astral-sh/uv/pull/11643))
- Treat lockfile as outdated if (empty) extras are added ([#11702](https://github.com/astral-sh/uv/pull/11702))
- Display path separators as backslashes on Windows ([#11667](https://github.com/astral-sh/uv/pull/11667))
- Display the built file name instead of the canonicalized name in `uv build` ([#11593](https://github.com/astral-sh/uv/pull/11593))
- Fix message when there are no buildable packages ([#11722](https://github.com/astral-sh/uv/pull/11722))
- Re-allow HTTP schemes for Git dependencies ([#11687](https://github.com/astral-sh/uv/pull/11687))

### Documentation

- Add anchor links to arguments and options in the CLI reference ([#11754](https://github.com/astral-sh/uv/pull/11754))
- Add link to environment marker specification ([#11748](https://github.com/astral-sh/uv/pull/11748))
- Fix missing a closing bracket in the `cache-keys` setting ([#11669](https://github.com/astral-sh/uv/pull/11669))
- Remove the last edited date from documentation pages ([#11753](https://github.com/astral-sh/uv/pull/11753))
- Fix readme typo ([#11742](https://github.com/astral-sh/uv/pull/11742))

## 0.6.2

### Enhancements

- Add support for constraining build dependencies with `tool.uv.build-constraint-dependencies` ([#11585](https://github.com/astral-sh/uv/pull/11585))
- Sort dependency group keys when adding new group ([#11591](https://github.com/astral-sh/uv/pull/11591))

### Performance

- Use an `Arc` for index URLs ([#11586](https://github.com/astral-sh/uv/pull/11586))

### Bug fixes

- Allow use of x86-64 Python on ARM Windows ([#11625](https://github.com/astral-sh/uv/pull/11625))
- Fix an issue where conflict markers could instigate a very large lock file ([#11293](https://github.com/astral-sh/uv/pull/11293))
- Fix duplicate packages with multiple conflicting extras declared ([#11513](https://github.com/astral-sh/uv/pull/11513))
- Respect color settings for log messages ([#11604](https://github.com/astral-sh/uv/pull/11604))
- Eagerly reject unsupported Git schemes ([#11514](https://github.com/astral-sh/uv/pull/11514))

### Documentation

- Add documentation for specifying Python versions in tool commands ([#11598](https://github.com/astral-sh/uv/pull/11598))

## 0.6.1

### Enhancements

- Allow users to mark platforms as "required" for wheel coverage ([#10067](https://github.com/astral-sh/uv/pull/10067))
- Warn for builds in non-build and workspace root pyproject.toml ([#11394](https://github.com/astral-sh/uv/pull/11394))

### Bug fixes

- Add `--all` to `uvx --reinstall` message ([#11535](https://github.com/astral-sh/uv/pull/11535))
- Fallback to `GET` on HTTP 400 when attempting to use range requests for wheel download ([#11539](https://github.com/astral-sh/uv/pull/11539))
- Prefer local variants in preference selection ([#11546](https://github.com/astral-sh/uv/pull/11546))
- Respect verbatim executable name in `uvx` ([#11524](https://github.com/astral-sh/uv/pull/11524))

### Documentation

- Add documentation for required environments ([#11542](https://github.com/astral-sh/uv/pull/11542))
- Note that `main.py` used to be `hello.py` ([#11519](https://github.com/astral-sh/uv/pull/11519))

## 0.6.0

There have been 31 releases and 1135 pull requests since [0.5.0](https://github.com/astral-sh/uv/releases/tag/0.5.0), our last release with breaking changes. As before, we've accumulated various changes that improve correctness and user experience, but could break some workflows. This release contains those changes; many have been marked as breaking out of an abundance of caution. We expect most users to be able to upgrade without making changes.

### Breaking changes

- **Create `main.py` instead of `hello.py` in `uv init`** ([#10369](https://github.com/astral-sh/uv/pull/10369))
  
  Previously, `uv init` created a `hello.py` sample file. Now, `uv init` will create `main.py` instead — which aligns with expectations from user feedback. The `--bare` option can be used to avoid creating the file altogether.
- **Respect `UV_PYTHON` in `uv python install`** ([#11487](https://github.com/astral-sh/uv/pull/11487))
  
  Previously, `uv python install` did not read this environment variable; now it does. We believe this matches user expectations, however, this will take priority over `.python-version` files which could be considered breaking.
- **Set `UV` to the uv executable path** ([#11326](https://github.com/astral-sh/uv/pull/11326))
  
  When uv spawns a subprocess, it will now have the `UV` environment variable set to the `uv` binary path. This change is breaking if you are setting the `UV` environment variable yourself, as we will overwrite its value.
  
  Additionally, this change requires marking the uv Rust entrypoint (`uv::main`) as `unsafe` to avoid unsoundness — this is only relevant if you are invoking uv using Rust. See the [Rust documentation](https://doc.rust-lang.org/std/env/fn.set_var.html#safety) for details about the safety of updating a process' environment.
- **Error on non-existent extras, e.g., in `uv sync`** ([#11426](https://github.com/astral-sh/uv/pull/11426))
  
  Previously, uv would silently ignore non-existent extras requested on the command-line (e.g., via `uv sync --extra foo`). This is *generally* correct behavior when resolving requests for package extras, because an extra may be present on one compatible version of a package but not another. However, this flexibility doesn't need to apply to the local project and it's less surprising to error here.
- **Error on missing dependency groups when `--frozen` is provided** ([#11499](https://github.com/astral-sh/uv/pull/11499))
  
  Previously, uv would not validate that the requested dependency groups were present in the lockfile when the `--frozen` flag was used. Now, an error will be raised if a requested dependency group is not present.
- **Change `-p` to a `--python` alias in `uv pip compile`** ([#11486](https://github.com/astral-sh/uv/pull/11486))
  
  In `uv pip compile`, `-p` was an alias for `--python-version` while everywhere else in uv's interface it is an alias for `--python`. Additionally, `uv pip compile` did not respect the `UV_PYTHON` environment variable. Now, the semantics of this flag have been updated for parity with the rest of the CLI.
  
  However, `--python-version` is unique: if we cannot find an interpreter with the given version, we will not fail. Instead, we'll use an alternative interpreter and override its version tags with the requested version during package resolution. This behavior is retained here for backwards compatibility, `--python <version>` / `-p <version>` will not fail if the version cannot be found. However, if a specific interpreter is requested, e.g., with `--python <path>` or `--python pypy`, and cannot be found — uv will exit with an error.
  
  The breaking changes here are that `UV_PYTHON` is respected and `--python <version>` will no longer fail if the version cannot be found.
- **Bump `alpine` default tag to 3.21 for derived Docker images** ([#11157](https://github.com/astral-sh/uv/pull/11157))
  
  Alpine 3.21 was released in Dec 2024 and is used in the official Alpine-based Python images. Our `uv:python3.x-alpine` images have been using 3.21 since uv v0.5.8. However, now the `uv:alpine` image will use 3.21 instead of 3.20 and `uv:alpine3.20` will no longer be updated.
- **Use files instead of junctions on Windows** ([#11269](https://github.com/astral-sh/uv/pull/11269))
  
  Previously, we used junctions for atomic replacement of cache entries on Windows. Now, we use a file with a pointer to the cache entry instead. This resolves various edge-case behaviors with junctions. These files are only intended to be consumed by uv and the cache version has been bumped. We do not think this change will affect workflows.

### Stabilizations

- **`uv publish` is no longer in preview** ([#11032](https://github.com/astral-sh/uv/pull/11032))
  
  This does not come with any behavior changes. You will no longer see an experimental warning when using `uv publish`. See the linked pull request for a report on the stabilization.

### Enhancements

- Support `--active` for PEP 723 script environments ([#11433](https://github.com/astral-sh/uv/pull/11433))
- Add `revision` to the lockfile to allow backwards-compatible metadata changes ([#11500](https://github.com/astral-sh/uv/pull/11500))

### Bug fixes

- Avoid reading metadata from `.egg-info` files ([#11395](https://github.com/astral-sh/uv/pull/11395))
- Include archive bucket version in archive pointers ([#11306](https://github.com/astral-sh/uv/pull/11306))
- Omit lockfile version when additional fields are dynamic ([#11468](https://github.com/astral-sh/uv/pull/11468))
- Respect executable name in `uvx --from tool@latest` ([#11465](https://github.com/astral-sh/uv/pull/11465))

### Documentation

- The `CHANGELOG.md` is now split into separate files for each "major" version to fix rendering ([#11510](https://github.com/astral-sh/uv/pull/11510))

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


