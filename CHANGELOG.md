# Changelog

<!-- prettier-ignore-start -->


## 0.11.30

Released on 2026-07-20.

### Preview features

- Support `--active` in `uv workspace metadata` ([#20500](https://github.com/astral-sh/uv/pull/20500))

### Performance

- Cache resolver Python requirement markers ([#20461](https://github.com/astral-sh/uv/pull/20461))
- Compact cached distribution metadata ([#20463](https://github.com/astral-sh/uv/pull/20463))
- Decode cached payloads on the cache-read pool ([#20464](https://github.com/astral-sh/uv/pull/20464))
- Decode stale cache entries in one blocking task ([#20486](https://github.com/astral-sh/uv/pull/20486))
- Further compact cached distribution metadata ([#20483](https://github.com/astral-sh/uv/pull/20483))
- Isolate cache reads on a blocking pool ([#20427](https://github.com/astral-sh/uv/pull/20427))
- Migrate lockfile serialization to `toml_writer` ([#20450](https://github.com/astral-sh/uv/pull/20450))
- Reuse resolver fork markers while recording preferences ([#20462](https://github.com/astral-sh/uv/pull/20462))
- Skip fully cutoff-excluded resolver candidates ([#20460](https://github.com/astral-sh/uv/pull/20460))

### Bug fixes

- Reuse centralized environments through workspace symlinks ([#20436](https://github.com/astral-sh/uv/pull/20436))
- Stop escaping `extends-environment` in pyvenv.cfg ([#20466](https://github.com/astral-sh/uv/pull/20466))

### Other changes

- Add a nightly fast-build test profile ([#20456](https://github.com/astral-sh/uv/pull/20456))
- Add contributing section to docs ([#20511](https://github.com/astral-sh/uv/pull/20511))
- Allow Packse scenarios without generated tests ([#20469](https://github.com/astral-sh/uv/pull/20469))
- Enable nightly Rust features for Windows tests ([#20452](https://github.com/astral-sh/uv/pull/20452))
- Enable nightly Rust features for development builds ([#20458](https://github.com/astral-sh/uv/pull/20458))
- Exclude skipped tar-wheel entries from RECORD healing ([#20429](https://github.com/astral-sh/uv/pull/20429))
- Fix stale release job reference ([#20442](https://github.com/astral-sh/uv/pull/20442))
- Support range requests in the Packse server ([#20473](https://github.com/astral-sh/uv/pull/20473))
- Sync latest Python releases ([#20519](https://github.com/astral-sh/uv/pull/20519))
- Use centralized crates.io publishing policies ([#20449](https://github.com/astral-sh/uv/pull/20449))

## 0.11.29

Released on 2026-07-15.

### Python

- Use gzip-compressed artifacts for PyPy downloads ([#20265](https://github.com/astral-sh/uv/pull/20265))

### Enhancements

- Add JSON output to `uv tree` ([#19978](https://github.com/astral-sh/uv/pull/19978))
- Add CUDA 13.2 as a supported PyTorch backend ([#20267](https://github.com/astral-sh/uv/pull/20267))
- Prefer local artifacts over URLs when installing from `pylock.toml` ([#20393](https://github.com/astral-sh/uv/pull/20393))
- Clarify diagnostics for unsatisfiable direct requirement ranges ([#20227](https://github.com/astral-sh/uv/pull/20227))
- Include the selected project name in missing-extra errors ([#20358](https://github.com/astral-sh/uv/pull/20358))

### Preview features

- Preserve extras and dependency-group conflict context when selecting locked project tools ([#20078](https://github.com/astral-sh/uv/pull/20078))
- Split OSV audit queries that exceed the service's 1,000-package limit ([#20398](https://github.com/astral-sh/uv/pull/20398))
- Apply OSV fixed-version information only to the matching package and ecosystem ([#20399](https://github.com/astral-sh/uv/pull/20399))
- Skip the virtualenv distutils monkeypatch on Python 3.10 and later ([#20222](https://github.com/astral-sh/uv/pull/20222))
- Report invalid `uv audit --service-url` values instead of panicking ([#20374](https://github.com/astral-sh/uv/pull/20374))
- Include preview settings in the published SchemaStore schema ([#20304](https://github.com/astral-sh/uv/pull/20304))

### Performance

- Reduce resolver work by widening selected versions across ranges without other known candidates ([#20115](https://github.com/astral-sh/uv/pull/20115))
- Defer client and build setup for no-op `uv sync` operations ([#20364](https://github.com/astral-sh/uv/pull/20364))
- Reuse workspace discovery during frozen syncs ([#20363](https://github.com/astral-sh/uv/pull/20363))
- Reuse workspace discovery after resolving settings ([#20356](https://github.com/astral-sh/uv/pull/20356))
- Reuse workspace discovery in `uv tree`, `uv export`, `uv format`, and `uv audit` ([#20359](https://github.com/astral-sh/uv/pull/20359))
- Avoid cache and interpreter setup when reading a project version ([#20360](https://github.com/astral-sh/uv/pull/20360))

### Bug fixes

- Reject duplicate active package entries in `pylock.toml` ([#20391](https://github.com/astral-sh/uv/pull/20391))
- Preserve direct-archive hashes in `uv pip freeze` output ([#20395](https://github.com/astral-sh/uv/pull/20395))
- Explain conflicting root requirements instead of displaying an empty version range ([#20228](https://github.com/astral-sh/uv/pull/20228))
- Prevent build-backend data paths from escaping the project or bypassing wheel exclusions ([#20397](https://github.com/astral-sh/uv/pull/20397))
- Reject PEP 517 backend paths outside the source tree, including paths that escape through symlinks ([#20387](https://github.com/astral-sh/uv/pull/20387))
- Redact credentials from failed Git fetch commands ([#20401](https://github.com/astral-sh/uv/pull/20401))
- Fix exclusive post-release range ordering to match PEP 440 ([#20268](https://github.com/astral-sh/uv/pull/20268))
- Canonicalize equivalent PEP 440 ranges during dependency resolution ([#20182](https://github.com/astral-sh/uv/pull/20182))
- Honor Python version pins when initializing scripts ([#20404](https://github.com/astral-sh/uv/pull/20404))
- Respect package-scoped source filtering for scripts ([#20389](https://github.com/astral-sh/uv/pull/20389))
- Report existing environment incompatibilities when `uv pip install --strict` has nothing to install ([#20388](https://github.com/astral-sh/uv/pull/20388))
- Continue scanning `platlib` when `purelib` is missing ([#20405](https://github.com/astral-sh/uv/pull/20405))
- Handle versionless `.egg-info` files as legacy package metadata ([#20403](https://github.com/astral-sh/uv/pull/20403))
- Make repeated locking idempotent for impossible cross-variable platform markers ([#20369](https://github.com/astral-sh/uv/pull/20369))
- Report invalid cloud credential endpoint URLs instead of panicking ([#20372](https://github.com/astral-sh/uv/pull/20372))
- Report invalid `pylock.toml` artifact URLs instead of panicking ([#20373](https://github.com/astral-sh/uv/pull/20373))
- Report non-UTF-8 virtual environment paths instead of panicking while generating activation scripts ([#20375](https://github.com/astral-sh/uv/pull/20375))
- Return an unsupported-operation error from unimplemented build-backend requirement hooks ([#20376](https://github.com/astral-sh/uv/pull/20376))

### Documentation

- Clarify `--no-build` behavior for editable requirements ([#20234](https://github.com/astral-sh/uv/pull/20234))
- Document uv's threat model ([#20236](https://github.com/astral-sh/uv/pull/20236))
- Reduce the number of badges in the README ([#11257](https://github.com/astral-sh/uv/pull/11257))

## 0.11.28

Released on 2026-07-07.

### Security

This release updates our ZIP library, [astral-async-zip](https://github.com/astral-sh/rs-async-zip), to v0.0.20, which includes 15 changes that harden our ZIP handling against [parser differentials](https://www.brainonfire.net/blog/2022/04/11/what-is-parser-mismatch/). uv may reject ZIP archives with malformed or ambiguous content that were previously accepted.

See the [upstream commits](https://github.com/astral-sh/rs-async-zip/compare/v0.0.18...v0.0.20) for a full list of changes.

### Python

- Upgrade GraalPy to 25.1.3 ([#20069](https://github.com/astral-sh/uv/pull/20069))

### Enhancements

- Improve trace logs for unexpected error chains ([#20220](https://github.com/astral-sh/uv/pull/20220))
- Move lockfile update guidance to a hint ([#20219](https://github.com/astral-sh/uv/pull/20219))
- Preserve indentation for multiline error causes ([#20156](https://github.com/astral-sh/uv/pull/20156))
- Render user errors with their cause chains ([#20217](https://github.com/astral-sh/uv/pull/20217))
- Route final command errors through the printer to respect `-q` and `-qq` ([#20163](https://github.com/astral-sh/uv/pull/20163))
- Use standard rendering for `uv build` errors ([#20159](https://github.com/astral-sh/uv/pull/20159))
- Use standard rendering for tool requirement errors ([#20160](https://github.com/astral-sh/uv/pull/20160))

### Performance

- Only compile bytecode for installed distributions in `uv pip install` ([#19914](https://github.com/astral-sh/uv/pull/19914))
- Avoid allocating URL-safe Git revisions ([#20194](https://github.com/astral-sh/uv/pull/20194))
- Avoid allocating canonical Python request strings ([#20193](https://github.com/astral-sh/uv/pull/20193))
- Avoid allocating custom Astral mirror URLs ([#20204](https://github.com/astral-sh/uv/pull/20204))
- Avoid allocating expanded compatibility tags ([#20190](https://github.com/astral-sh/uv/pull/20190))
- Avoid allocating shell strings that need no escaping ([#20196](https://github.com/astral-sh/uv/pull/20196))
- Avoid allocating static ABI descriptions ([#20201](https://github.com/astral-sh/uv/pull/20201))
- Avoid allocating static Windows executable names ([#20200](https://github.com/astral-sh/uv/pull/20200))
- Avoid allocating static dependency table names ([#20199](https://github.com/astral-sh/uv/pull/20199))
- Avoid allocating static platform triple components ([#20195](https://github.com/astral-sh/uv/pull/20195))
- Avoid allocating static resolver report labels ([#20198](https://github.com/astral-sh/uv/pull/20198))
- Avoid allocating static unavailable-version messages ([#20197](https://github.com/astral-sh/uv/pull/20197))
- Avoid allocating unchanged Python download architectures ([#20202](https://github.com/astral-sh/uv/pull/20202))
- Avoid allocating unchanged paths during case normalization ([#20203](https://github.com/astral-sh/uv/pull/20203))
- Avoid allocations when expanding group conflicts ([#20211](https://github.com/astral-sh/uv/pull/20211))
- Avoid allocations when formatting requirements ([#20206](https://github.com/astral-sh/uv/pull/20206))
- Avoid cloning credential lookup services ([#20210](https://github.com/astral-sh/uv/pull/20210))
- Avoid cloning dry-run distributions ([#20209](https://github.com/astral-sh/uv/pull/20209))
- Avoid cloning owned dependency metadata ([#20212](https://github.com/astral-sh/uv/pull/20212))
- Avoid redundant direct URL clones ([#20207](https://github.com/astral-sh/uv/pull/20207))
- Create metadata version errors lazily ([#20205](https://github.com/astral-sh/uv/pull/20205))
- Optimize expanded tag compatibility checks ([#20171](https://github.com/astral-sh/uv/pull/20171))
- Optimize parsing of single-digit three-part versions ([#20118](https://github.com/astral-sh/uv/pull/20118))

### Bug fixes

- Avoid overflow when computing HTTP cache age ([#20178](https://github.com/astral-sh/uv/pull/20178))
- Respect `--upgrade` when `upgrade-package` is configured ([#19955](https://github.com/astral-sh/uv/pull/19955))
- Support `uv tree` in dependency-group-only projects ([#20167](https://github.com/astral-sh/uv/pull/20167))
- Treat cache entries as stale at exact expiration ([#20183](https://github.com/astral-sh/uv/pull/20183))

## 0.11.27

Released on 2026-07-06.

### Enhancements

- Continue on ignored errors when fetching wheel metadata ([#12255](https://github.com/astral-sh/uv/pull/12255))
- Use caching for `--python-downloads-json-url` ([#16749](https://github.com/astral-sh/uv/pull/16749))

### Preview features

- Discover extensionless shebang scripts in `uv workspace list --scripts` ([#20099](https://github.com/astral-sh/uv/pull/20099))

### Performance

- Avoid full site-packages scans for direct reinstalls ([#20119](https://github.com/astral-sh/uv/pull/20119))
- Avoid redundant pyproject parsing ([#20076](https://github.com/astral-sh/uv/pull/20076))
- Cache default dependency markers when reading locks ([#20125](https://github.com/astral-sh/uv/pull/20125))
- Enable SIMD-accelerated TOML parsing ([#20079](https://github.com/astral-sh/uv/pull/20079))
- Intern `requires-python` specifiers in Simple API parsing ([#20104](https://github.com/astral-sh/uv/pull/20104))
- Read cache entries into exact-sized buffers ([#20120](https://github.com/astral-sh/uv/pull/20120))
- Reduce VersionSpecifiers parsing allocations ([#20105](https://github.com/astral-sh/uv/pull/20105))
- Reduce site-packages scan allocation overhead ([#20087](https://github.com/astral-sh/uv/pull/20087))
- Reuse package names when parsing wheel filenames ([#20110](https://github.com/astral-sh/uv/pull/20110))
- Sort Simple API files after grouping ([#20112](https://github.com/astral-sh/uv/pull/20112))

### Bug fixes

- Always emit `packages` table for pylock.toml ([#20145](https://github.com/astral-sh/uv/pull/20145))
- Avoid blank line for empty `uv pip tree` ([#20062](https://github.com/astral-sh/uv/pull/20062))
- Encode hashes in file paths ([#19807](https://github.com/astral-sh/uv/pull/19807))
- Error on a registry uv.lock package without a version instead of panicking ([#19855](https://github.com/astral-sh/uv/pull/19855))
- Preserve conditional extra markers in exports ([#20148](https://github.com/astral-sh/uv/pull/20148))
- Skip the ambiguous authority check for file transport VCS URLs ([#20086](https://github.com/astral-sh/uv/pull/20086))
- Sync index format when `uv add --index` updates an existing index URL ([#19818](https://github.com/astral-sh/uv/pull/19818))

### Other changes

- Re-add `pub` APIs used in Pixi ([#20074](https://github.com/astral-sh/uv/pull/20074))
- Update Rust toolchain to 1.96.1 ([#20103](https://github.com/astral-sh/uv/pull/20103))

## 0.11.26

Released on 2026-06-30.

### Performance

- Adapt uv to IDs-only PubGrub dependencies ([#20048](https://github.com/astral-sh/uv/pull/20048))
- Avoid allocations in `ForkMap::contains` ([#20023](https://github.com/astral-sh/uv/pull/20023))
- Reuse resolver work across PubGrub iterations ([#20020](https://github.com/astral-sh/uv/pull/20020))
- Speed up candidate selection for disjoint ranges ([#20026](https://github.com/astral-sh/uv/pull/20026))

### Bug fixes

- Warn when the build cache is inside the source directory ([#20056](https://github.com/astral-sh/uv/pull/20056))

## 0.11.25

Released on 2026-06-26.

### Security

This release updates our tar library, [astral-tokio-tar](https://github.com/astral-sh/tokio-tar), to v0.6.3, which includes over 20 changes that harden our tar handling against [parser differentials](https://www.brainonfire.net/blog/2022/04/11/what-is-parser-mismatch/). uv may reject source distributions with malformed or ambiguous content that were previously accepted.

See the [upstream commits](https://github.com/astral-sh/tokio-tar/compare/v0.6.2...v0.6.3) for a full list of changes.

### Enhancements

- Add a full "lockfile" to tool receipts ([#18937](https://github.com/astral-sh/uv/pull/18937))
- Allow scoped overrides to add dependencies ([#19974](https://github.com/astral-sh/uv/pull/19974))
- Avoid writing redundant lockfile markers with `tool.uv.environments` ([#19933](https://github.com/astral-sh/uv/pull/19933))
- Factor supported environments out of lockfile markers ([#19969](https://github.com/astral-sh/uv/pull/19969))
- Recommend our own build backend in the build frontend ([#19994](https://github.com/astral-sh/uv/pull/19994))
- Reject wheels with multiple .dist-info directories ([#19986](https://github.com/astral-sh/uv/pull/19986))
- Simplify dependency markers under parent reachability ([#19971](https://github.com/astral-sh/uv/pull/19971))
- Support scoped dependency exclusions ([#19977](https://github.com/astral-sh/uv/pull/19977))
- Support scoped dependency overrides ([#19970](https://github.com/astral-sh/uv/pull/19970))
- Explain why files are skipped in registry index parsing ([#19983](https://github.com/astral-sh/uv/pull/19983))

### Preview features

- Add `uv workspace list --scripts` ([#20009](https://github.com/astral-sh/uv/pull/20009))
- Support centralised environments in `uv venv` ([#19912](https://github.com/astral-sh/uv/pull/19912))
- Use locked ty versions in `uv check` ([#19884](https://github.com/astral-sh/uv/pull/19884))
- Add centralized storage of project environments ([#18214](https://github.com/astral-sh/uv/pull/18214))
- Verify lockfile hashes before reusing a cached ty in `uv check` ([#19995](https://github.com/astral-sh/uv/pull/19995))
- Use locked dependency selection for `uv check --script` ([#19989](https://github.com/astral-sh/uv/pull/19989))

### Bug fixes

- Preserve standalone markers in workspace metadata ([#20011](https://github.com/astral-sh/uv/pull/20011))
- Reject `uv build` if the cache dir is enclosed ([#19991](https://github.com/astral-sh/uv/pull/19991))

## 0.11.24

Released on 2026-06-23.

### Python

- Add CPython 3.15.0b3 ([#19964](https://github.com/astral-sh/uv/pull/19964))

### Preview features

- Make project environments relocatable under preview ([#19965](https://github.com/astral-sh/uv/pull/19965))

### Performance

- Use a compact index for lazy version maps ([#19959](https://github.com/astral-sh/uv/pull/19959))

### Bug fixes

- Allow disabling `exclude-newer` ([#19934](https://github.com/astral-sh/uv/pull/19934))
- Avoid archive id collisions ([#19949](https://github.com/astral-sh/uv/pull/19949))
- Reapply "Fix transparent Python upgrades in project environments" ([#19928](https://github.com/astral-sh/uv/pull/19928))
- Clean up partial tool entrypoint installs ([#19966](https://github.com/astral-sh/uv/pull/19966))
- Fix relocatable `activate.fish` and broaden Fish version support ([#19856](https://github.com/astral-sh/uv/pull/19856))

## 0.11.23

Released on 2026-06-19.

### Bug fixes

- Revert "Fix transparent Python upgrades in project environments" to mitigate unintended breakage in `pre-commit-uv` ([#19925](https://github.com/astral-sh/uv/pull/19925))
- Restore old behavior where workspace members "hidden" by an intermediate `pyproject.toml` would be treated as standalone projects ([#19926](https://github.com/astral-sh/uv/pull/19926))

## 0.11.22

Released on 2026-06-18.

### Enhancements

- Publish wheels before sdists in `uv publish` ([#19831](https://github.com/astral-sh/uv/pull/19831))
- Add `TY` and `RUFF` env vars for providing paths for binaries used by `uv format` and `uv check` ([#19821](https://github.com/astral-sh/uv/pull/19821))

### Preview features

- Allow configuring preview features in `uv.toml` and `pyproject.toml` ([#18437](https://github.com/astral-sh/uv/pull/18437))
- Update the lockfile during `uv check --no-sync` ([#19909](https://github.com/astral-sh/uv/pull/19909))
- Add `--script` to `uv check` and `uv metadata` ([#19860](https://github.com/astral-sh/uv/pull/19860))
- Report workspace-exclusive dependency groups in `workspace metadata` ([#19862](https://github.com/astral-sh/uv/pull/19862))
- Support SARIF as a `uv audit` output ([#19872](https://github.com/astral-sh/uv/pull/19872))

### Performance

- Use a more deadlock-resistant concurrent hashmap in the resolver ([#19532](https://github.com/astral-sh/uv/pull/19532))

### Bug fixes

- Update string marker ordering semantics to match [upstream clarified rules](https://github.com/pypa/packaging.python.org/pull/1988) ([#19808](https://github.com/astral-sh/uv/pull/19808))
- Reject extras that have the same normalized name ([#19871](https://github.com/astral-sh/uv/pull/19871))
- Reject dependency group `include-group` entries that have additional fields ([#19866](https://github.com/astral-sh/uv/pull/19866))
- Reject invalid UTF-8 URL credentials ([#19814](https://github.com/astral-sh/uv/pull/19814))
- Validate that PEP 517 `backend-path`s exist when building sdists ([#19834](https://github.com/astral-sh/uv/pull/19834))
- Validate that `pylock.toml` files do not have an unsupported a `lock-version` ([#19869](https://github.com/astral-sh/uv/pull/19869))
- Validate that the environment satisfies the `packages.requires-python` of a `pylock.toml` ([#19868](https://github.com/astral-sh/uv/pull/19868))
- Allow `uv` to be recursively invoked by PEP 517 build hooks ([#19879](https://github.com/astral-sh/uv/pull/19879))
- Allow empty `credentials.toml` files ([#19815](https://github.com/astral-sh/uv/pull/19815))
- Fix transparent Python upgrades in project environments ([#19890](https://github.com/astral-sh/uv/pull/19890))
- Handle non-file editable URLs in `uv pip list` ([#19867](https://github.com/astral-sh/uv/pull/19867))
- Fix incorrect output from `uv tree --invert` ([#19910](https://github.com/astral-sh/uv/pull/19910))
- Fix environment locking of `uv venv` in a project ([#19837](https://github.com/astral-sh/uv/pull/19837))
- Fix handling of workspace-exclusive dependency groups in `uv tree` ([#19905](https://github.com/astral-sh/uv/pull/19905))

### Documentation

- Archive the 0.10.x changelog ([#19813](https://github.com/astral-sh/uv/pull/19813))

### Other changes

- Mark more tests as requiring network for vendors that need to run tests offline ([#19819](https://github.com/astral-sh/uv/pull/19819))

## 0.11.21

Released on 2026-06-11.

### Python

- Add CPython 3.13.14 and 3.14.6 ([#19787](https://github.com/astral-sh/uv/pull/19787))

### Preview features

- Add `environment.root` to `uv workspace metadata --sync` ([#19760](https://github.com/astral-sh/uv/pull/19760))
- Allow `uv upgrade` to update a single dependency constraint ([#19738](https://github.com/astral-sh/uv/pull/19738))
- Compute and pass `uv workspace metadata` payload in `ty check` ([#19763](https://github.com/astral-sh/uv/pull/19763))
- Make packaged applications the default for `uv init` ([#17841](https://github.com/astral-sh/uv/pull/17841))

### Performance

- Add parallel discovery of Python versions for `uv python list` ([#18684](https://github.com/astral-sh/uv/pull/18684))
- Avoid normalizing source distribution names twice ([#19784](https://github.com/astral-sh/uv/pull/19784))

### Bug fixes

- Improve cache robustness and pruning behavior
  - Allow CI cache pruning without an sdist bucket ([#19802](https://github.com/astral-sh/uv/pull/19802))
  - Avoid overflow when reading malformed cache entries ([#19799](https://github.com/astral-sh/uv/pull/19799))
  - Preserve cached Python downloads during cache pruning ([#19795](https://github.com/astral-sh/uv/pull/19795))
  - Reject running inside the cache ([#19659](https://github.com/astral-sh/uv/pull/19659))
- Fix Python discovery and version request edge cases
  - Avoid panics for Unicode Python version requests ([#19797](https://github.com/astral-sh/uv/pull/19797))
  - Fix handling of non-critical errors in `uv python list` with path requests ([#19774](https://github.com/astral-sh/uv/pull/19774))
  - Fix stop-discovery-at regression ([#19769](https://github.com/astral-sh/uv/pull/19769))
- Harden parsing and validation for package metadata, requirements, markers, URLs, and conflict sets
  - Allow trailing commas in version specifiers ([#19806](https://github.com/astral-sh/uv/pull/19806))
  - Avoid panics for invalid UTF-8 URL credentials ([#19800](https://github.com/astral-sh/uv/pull/19800))
  - Avoid panics for malformed source distribution filenames ([#19776](https://github.com/astral-sh/uv/pull/19776))
  - Avoid panics for trailing extra separators ([#19779](https://github.com/astral-sh/uv/pull/19779))
  - Avoid stack overflow for recursive requirements path aliases ([#19777](https://github.com/astral-sh/uv/pull/19777))
  - Ignore reversed string compatible-release markers ([#19782](https://github.com/astral-sh/uv/pull/19782))
  - Reject duplicate entries in conflict sets ([#19801](https://github.com/astral-sh/uv/pull/19801))
  - Reject malformed hash options in requirements files ([#19783](https://github.com/astral-sh/uv/pull/19783))
  - Reject source distribution filenames without a separator ([#19803](https://github.com/astral-sh/uv/pull/19803))
  - Use UTF-8 lengths for requirement errors ([#19781](https://github.com/astral-sh/uv/pull/19781))
  - Use UTF-8 lengths for trailing marker errors ([#19796](https://github.com/astral-sh/uv/pull/19796))
  - Use byte offsets when peeking over requirements ([#19780](https://github.com/astral-sh/uv/pull/19780))
  - Validate GraalPy ABI suffixes ([#19805](https://github.com/astral-sh/uv/pull/19805))
- Improve wheel entry-point error handling and virtual environment activation quoting
  - Propagate errors when reading wheel entry points ([#19794](https://github.com/astral-sh/uv/pull/19794))
  - Quote virtual environment activation paths with shell metacharacters ([#19798](https://github.com/astral-sh/uv/pull/19798))

## 0.11.20

Released on 2026-06-10.

### Enhancements

- Add `--emit-index-url` and `--emit-find-links` to `uv export` ([#18370](https://github.com/astral-sh/uv/pull/18370))
- Add `--find-links` support for `uv pip list` ([#16103](https://github.com/astral-sh/uv/pull/16103))
- Group executable install errors during `uv python install` ([#19691](https://github.com/astral-sh/uv/pull/19691))
- Use ICF in macOS release builds to reduce binary sizes ([#19615](https://github.com/astral-sh/uv/pull/19615))

### Preview features

- Add initial hidden `uv upgrade` command ([#19678](https://github.com/astral-sh/uv/pull/19678))
- Reject Git revisions in `uv upgrade` ([#19742](https://github.com/astral-sh/uv/pull/19742))

### Configuration

- Recognize `UV_NO_INSTALL_PROJECT`, `UV_NO_INSTALL_WORKSPACE`, `UV_NO_INSTALL_LOCAL` ([#19323](https://github.com/astral-sh/uv/pull/19323))

### Performance

- Speed up discovery of large workspaces ([#18311](https://github.com/astral-sh/uv/pull/18311))

### Bug fixes

- Allow unknown preview flags with a warning again ([#19669](https://github.com/astral-sh/uv/pull/19669))
- Apply dependency exclusions to direct requirements ([#19699](https://github.com/astral-sh/uv/pull/19699))
- Avoid following external symlinks during cache clean ([#19682](https://github.com/astral-sh/uv/pull/19682))
- Avoid following symlinks during cache prune ([#19543](https://github.com/astral-sh/uv/pull/19543))
- Fix Git cache keys for worktrees and packed refs ([#19706](https://github.com/astral-sh/uv/pull/19706))
- Make resolver error handling iterative to avoid stack overflows ([#19695](https://github.com/astral-sh/uv/pull/19695))
- Pass `VIRTUAL_ENV` through `cygpath` inside `fish` on Windows ([#19703](https://github.com/astral-sh/uv/pull/19703))
- Rebuild explicit local directory tool installs ([#19591](https://github.com/astral-sh/uv/pull/19591))
- Validate egg top-level entries as identifiers ([#19679](https://github.com/astral-sh/uv/pull/19679))

### Documentation

- Document `--find-links` caching behavior ([#19585](https://github.com/astral-sh/uv/pull/19585))
- Add a small section for malware checks ([#19680](https://github.com/astral-sh/uv/pull/19680))

## 0.11.19

Released on 2026-06-03.

### Python

- Add CPython 3.15.0b2 ([#19531](https://github.com/astral-sh/uv/pull/19531))

### Enhancements

- Always compute SHA256 for remote distributions ([#19662](https://github.com/astral-sh/uv/pull/19662))
- Add PyEmscripten platform (PEP 783) ([#19629](https://github.com/astral-sh/uv/pull/19629))
- Add Pyodide 2025 target triple ([#19653](https://github.com/astral-sh/uv/pull/19653))

### Preview features

- Make preview features for commands have names that aren't ambiguous with the command ([#19645](https://github.com/astral-sh/uv/pull/19645))
- Respect `--isolated` in `uv check` ([#19666](https://github.com/astral-sh/uv/pull/19666))

### Bug fixes

- Continue tool uninstall after dangling receipts ([#19623](https://github.com/astral-sh/uv/pull/19623))
- Skip Unix-specific installation steps when cross-installing Windows Python distributions ([#19424](https://github.com/astral-sh/uv/pull/19424))

## 0.11.18

Released on 2026-06-01.

### Performance

- Fix performance regression in unzip of local wheels ([#19637](https://github.com/astral-sh/uv/pull/19637))

### Preview

- Add `uv check` to run `ty` from uv ([#19605](https://github.com/astral-sh/uv/pull/19605))

### Bug fixes

- Update activation scripts with upstream fixes ([#19628](https://github.com/astral-sh/uv/pull/19628))

### Other changes

- Bump MSRV to 1.94 ([#19600](https://github.com/astral-sh/uv/pull/19600))

## 0.11.17

Released on 2026-05-28.

### Enhancements

- Add a diagnostic for `uv add` with standard library modules ([#19572](https://github.com/astral-sh/uv/pull/19572))
- Expose `uv workspace` and its `list` subcommand in help output ([#19533](https://github.com/astral-sh/uv/pull/19533))
- Improve the "403 forbidden" hint to suggest `ignore-error-codes` when applicable ([#19521](https://github.com/astral-sh/uv/pull/19521))
- Skip direct URL lock freshness checks while offline ([#19596](https://github.com/astral-sh/uv/pull/19596))
- Add `import-names` and `import-namespaces` support to `uv-build` ([PEP 794](https://peps.python.org/pep-0794/)) ([#19380](https://github.com/astral-sh/uv/pull/19380))
- Add a `--no-editable-package` flag to various commands ([#19584](https://github.com/astral-sh/uv/pull/19584))
- Infer Python version requests from source trees in `uv tool` invocations ([#19577](https://github.com/astral-sh/uv/pull/19577))

### Preview features

- Add module owners to `uv workspace metadata` ([#19122](https://github.com/astral-sh/uv/pull/19122))
- Do not allow `uv venv --clear` to remove non-virtual environments ([#19595](https://github.com/astral-sh/uv/pull/19595))

### Bug fixes

- Improve the performance of large entries in `tool.uv.conflicts` ([#19538](https://github.com/astral-sh/uv/pull/19538))
- Avoid modifying the parent process' env with `--env-file` in `uv run` ([#19567](https://github.com/astral-sh/uv/pull/19567))
- Fix script environment creation for scripts with long filenames ([#19539](https://github.com/astral-sh/uv/pull/19539))
- Fix transitive Git archive dependencies in lockfiles ([#19589](https://github.com/astral-sh/uv/pull/19589))
- Preserve Git repository URLs in direct URL metadata ([#19590](https://github.com/astral-sh/uv/pull/19590))
- Support redirects in `--check-url` ([#19594](https://github.com/astral-sh/uv/pull/19594))
- Accept case-insensitive HTML tags in `--find-links` parsing ([#19537](https://github.com/astral-sh/uv/pull/19537))
- Reject duplicate script metadata blocks ([#19544](https://github.com/astral-sh/uv/pull/19544))
- Ban names like "python3" as script entry points ([#19535](https://github.com/astral-sh/uv/pull/19535), [#19536](https://github.com/astral-sh/uv/pull/19536))
- Validate Git LFS artifacts for Git archives ([#19592](https://github.com/astral-sh/uv/pull/19592))
- Use a relative path when creating symlinks in cache to improve relocatability ([#19033](https://github.com/astral-sh/uv/pull/19033))

### Documentation

- Fix malformed positional anchors in the CLI reference ([#19575](https://github.com/astral-sh/uv/pull/19575))

## 0.11.16

Released on 2026-05-21.

### Enhancements

- Add support for direct archive dependencies in Git ([#10072](https://github.com/astral-sh/uv/pull/10072))
- Adjust hint rendering ([#18090](https://github.com/astral-sh/uv/pull/18090))

### Preview features

- uv audit: specialize malformed OSV error ([#19515](https://github.com/astral-sh/uv/pull/19515))
- Reject locked malware installations ([#18936](https://github.com/astral-sh/uv/pull/18936))

### Configuration

- Allow disabling reading the system config with `UV_NO_SYSTEM_CONFIG` ([#19476](https://github.com/astral-sh/uv/pull/19476))

### Bug fixes

- Allow environment variables that take a list to be empty ([#19503](https://github.com/astral-sh/uv/pull/19503))
- Ensure that incompatible wheel hints do not leak secrets ([#19504](https://github.com/astral-sh/uv/pull/19504))
- Reject unsafe entry points in `uv-build` ([#19495](https://github.com/astral-sh/uv/pull/19495))
- Restrict delimiters in entry point parsing ([#19471](https://github.com/astral-sh/uv/pull/19471))
- uv-netrc: fix multi-word no-space comment lines causing parse errors ([#19494](https://github.com/astral-sh/uv/pull/19494))

### Documentation

- Document and test relative exclude-newer support for uv pip ([#19475](https://github.com/astral-sh/uv/pull/19475))

## 0.11.15

Released on 2026-05-18.

### Security

- Fix a TAR parser differential, see [GHSA-3cv2-h65g-fgmm](https://github.com/astral-sh/tokio-tar/security/advisories/GHSA-3cv2-h65g-fgmm) ([#19463](https://github.com/astral-sh/uv/pull/19463))
- Enforce that entry points cannot escape in the scripts directory, see [GHSA-4gg8-gxpx-9rph](https://github.com/astral-sh/uv/security/advisories/GHSA-4gg8-gxpx-9rph) ([#19464](https://github.com/astral-sh/uv/pull/19464))

### Enhancements

- Add TOML v1.1 -> v1.0 backwards compatibility for source distributions ([#18741](https://github.com/astral-sh/uv/pull/18741))
- Add support for Azure request signing ([#19421](https://github.com/astral-sh/uv/pull/19421))
- Apply stricter validation to all wheel filename segments ([#19364](https://github.com/astral-sh/uv/pull/19364))
- Reject empty strings as an invalid package name ([#19435](https://github.com/astral-sh/uv/pull/19435))
- Use structured errors for signing authentication failures ([#19422](https://github.com/astral-sh/uv/pull/19422))

### Preview

- uv audit: Add JSON output ([#19305](https://github.com/astral-sh/uv/pull/19305))

### Configuration

- Respect `required-environments` in `uv pip compile` ([#19378](https://github.com/astral-sh/uv/pull/19378))

### Performance

- Avoid parsing JSON manifest when local Python is available ([#19398](https://github.com/astral-sh/uv/pull/19398))
- Avoid walking nested directories in linker conflict registration ([#19382](https://github.com/astral-sh/uv/pull/19382))
- Optimize async wheel ZIP writing ([#19383](https://github.com/astral-sh/uv/pull/19383))
- Fix dead "already trimmed" fast-path in `Version::only_release_trimmed` ([#19425](https://github.com/astral-sh/uv/pull/19425))

### Bug fixes

- Apply workspace-member `[tool.uv.sources]` credentials under `uv sync --frozen` ([#19423](https://github.com/astral-sh/uv/pull/19423))
- Skip empty directories in uv build outputs ([#19437](https://github.com/astral-sh/uv/pull/19437))
- Fix Git submodule handling when using relative paths ([#12156](https://github.com/astral-sh/uv/pull/12156))
- Fix line number reporting in netrc parsing ([#19452](https://github.com/astral-sh/uv/pull/19452))

### Documentation

- Move Bazel auth helper setup into integration guide ([#19392](https://github.com/astral-sh/uv/pull/19392))

## 0.11.14

Released on 2026-05-12.

### Enhancements

- Add Astral mirror URL override ([#19206](https://github.com/astral-sh/uv/pull/19206))
- Ignore `top_level.txt` entries in uninstall that are not valid Python identifiers ([#19340](https://github.com/astral-sh/uv/pull/19340))

### Bug fixes

- Avoid applying `.env` files in parent process ([#19343](https://github.com/astral-sh/uv/pull/19343))
- Filter ANSI codes in logging output ([#19311](https://github.com/astral-sh/uv/pull/19311))
- Fix `uv tree` showing extra-conditional deps for packages required without extras ([#19332](https://github.com/astral-sh/uv/pull/19332))
- Respect build options (e.g., `--no-build`) during lock validation ([#19366](https://github.com/astral-sh/uv/pull/19366))

## 0.11.13

Released on 2026-05-10.

### Bug fixes

- Include data files in editable builds ([#19312](https://github.com/astral-sh/uv/pull/19312))
- Respect `--require-hashes` when installing from `pylock.toml` files ([#19334](https://github.com/astral-sh/uv/pull/19334))

### Python
### Python

- Add CPython 3.14.5

## 0.11.12

Released on 2026-05-08.

### Python

- Add CPython 3.15.0b1

### Enhancements

- Add `--no-editable` support to `uv pip install` ([#19306](https://github.com/astral-sh/uv/pull/19306))
- Require git refs in URLs to be percent-encoded ([#19320](https://github.com/astral-sh/uv/pull/19320))

### Bug fixes

- Respect `--no-dev` over `UV_DEV=1` ([#19313](https://github.com/astral-sh/uv/pull/19313))
- Don't suggest non-existent `--no-frozen` flag (#19290) ([#19294](https://github.com/astral-sh/uv/pull/19294))

### Documentation

- Fix bug from inconsistent workflow name in GHA-PyPI guide example ([#19309](https://github.com/astral-sh/uv/pull/19309))

## 0.11.11

Released on 2026-05-06.

### Bug fixes

- Accept legacy ID format from pre-0.11.9 cache entries ([#19301](https://github.com/astral-sh/uv/pull/19301))

## 0.11.10

Released on 2026-05-05.

### Bug fixes

- Allow pre-release Python requests with non-zero patch versions ([#19286](https://github.com/astral-sh/uv/pull/19286))

## 0.11.9

Released on 2026-05-04.

This release includes a special release candidate for the next Python 3.14 patch release. Python 3.14 included a new garbage collection implementation, which reduced pause times but caused significant unexpected memory pressure in production environments. In 3.14.5 and 3.15, the previous garbage collection implementation will be restored.

We would greatly appreciate if you tested the 3.14.5rc1 version included in this release. The stable version is expected to be released soon and any feedback on potential issues would be helpful to the Python development team.

For more context, see the [announcement](https://discuss.python.org/t/reverting-the-incremental-gc-in-python-3-14-and-3-15/107014), [issue](https://github.com/python/cpython/issues/148726), and [pull request](https://github.com/python/cpython/pull/148720).

Issues with the new release can be reported in the uv or CPython issue trackers.

### Python

- Upgrade PyPy to v7.3.22
- Add CPython 3.14.5rc1
- On macOS, CPython statically links `libpython` to match Linux

### Enhancements

- Omit compatible release desugaring for pre-release hints ([#19267](https://github.com/astral-sh/uv/pull/19267))
- Fix file locks on Android ([#18323](https://github.com/astral-sh/uv/pull/18323))

### Preview

- `uv audit` add reporting for adverse project statuses ([#19128](https://github.com/astral-sh/uv/pull/19128))

### Bug fixes

- Discover versioned Python executables when `requires-python` pins a version ([#18700](https://github.com/astral-sh/uv/pull/18700))
- Fix URL prefix matching to require path boundaries ([#19154](https://github.com/astral-sh/uv/pull/19154))
- Fix transitive Git path dependencies in lockfiles ([#19269](https://github.com/astral-sh/uv/pull/19269))
- Handle incorrect unlock error in `LockedFile::drop` on Wine ([#19229](https://github.com/astral-sh/uv/pull/19229))
- Prevent uninstalling site-packages for empty `top_level.txt` in `.egg-info` ([#19114](https://github.com/astral-sh/uv/pull/19114))
- Use symlinks instead of junctions on Wine ([#19213](https://github.com/astral-sh/uv/pull/19213))
- Fix floating-point environment handling on ARMv7 ([#19157](https://github.com/astral-sh/uv/pull/19157))
- Redact credentials from remote requirements URL in offline errors ([#19216](https://github.com/astral-sh/uv/pull/19216))
- Windows tramplolines no longer set `PYTHONHOME` and only set `__PYVENV_LAUNCHER__` for virtual environments ([#19199](https://github.com/astral-sh/uv/pull/19199))

### Documentation

- Mark `--native-tls` and `UV_NATIVE_TLS` as deprecated ([#18705](https://github.com/astral-sh/uv/pull/18705))
- Re-add `pytorch-triton-rocm` to PyTorch ROCm docs ([#19241](https://github.com/astral-sh/uv/pull/19241))
- Tweak changelog entries for 0.11.8 ([#19188](https://github.com/astral-sh/uv/pull/19188))
- Add 'Exporting lockfiles' to the Concepts->Projects index ([#19209](https://github.com/astral-sh/uv/pull/19209))
- Clarify that `uv init` creates git files / folders in the projects guide ([#19183](https://github.com/astral-sh/uv/pull/19183))

## 0.11.8

Released on 2026-04-27.

### Enhancements

- Add `--python-downloads-json-url` to `python pin` ([#19092](https://github.com/astral-sh/uv/pull/19092))
- Fetch uv from Astral mirror during self-update ([#18682](https://github.com/astral-sh/uv/pull/18682))
- Support `pip uninstall -y` ([#19082](https://github.com/astral-sh/uv/pull/19082))
- Allow `exclude-newer` to be missing from the lockfile when `exclude-newer-span` is present ([#19024](https://github.com/astral-sh/uv/pull/19024))
- Only show the version number in `uv self version --short` ([#19019](https://github.com/astral-sh/uv/pull/19019))
- Silence warnings on empty `SSL_CERT_DIR` directory ([#19018](https://github.com/astral-sh/uv/pull/19018))
- Use a sentinel timestamp for relative `exclude-newer` and `exclude-newer-package` values in lockfiles ([#19022](https://github.com/astral-sh/uv/pull/19022), [#19101](https://github.com/astral-sh/uv/pull/19101))

### Configuration

- Add `UV_PYTHON_NO_REGISTRY` ([#19035](https://github.com/astral-sh/uv/pull/19035))
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
- Ban external symlinks in `.tar.zst` wheels ([#19144](https://github.com/astral-sh/uv/pull/19144))

### Distributions

- Remove deprecated license classifiers from uv-build and add Python 3.14 classifier ([#19130](https://github.com/astral-sh/uv/pull/19130))

### Documentation

- Bump astral-sh/setup-uv version in docs ([#19030](https://github.com/astral-sh/uv/pull/19030))
- Update PyTorch documentation for PyTorch 2.11 ([#19095](https://github.com/astral-sh/uv/pull/19095))

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


