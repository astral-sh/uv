# Changelog

## 0.2.36

### Bug fixes

- Use consistent canonicalization for URLs ([#5980](https://github.com/astral-sh/uv/pull/5980))
- Improve warning message when parsing `pyproject.toml` fails ([#6009](https://github.com/astral-sh/uv/pull/6009))
- Improve handling of overlapping markers in universal resolver ([#5887](https://github.com/astral-sh/uv/pull/5887))

## 0.2.35

### CLI

- Deprecate `--system` and `--no-system` in `uv venv` ([#5925](https://github.com/astral-sh/uv/pull/5925))
- Make `--upgrade` imply `--refresh` ([#5943](https://github.com/astral-sh/uv/pull/5943))
- Warn when there are missing bounds on transitive dependencies with `--resolution-strategy lowest` ([#5953](https://github.com/astral-sh/uv/pull/5953))

### Configuration

- Add support for `no-build-isolation-package` ([#5894](https://github.com/astral-sh/uv/pull/5894))

### Performance

- Enable LTO optimizations in release builds to reduce binary size ([#5904](https://github.com/astral-sh/uv/pull/5904))
- Prefetch metadata in `--no-deps` mode ([#5918](https://github.com/astral-sh/uv/pull/5918))

### Bug fixes

- Display portable paths in POSIX virtual environment activation commands ([#5956](https://github.com/astral-sh/uv/pull/5956))
- Respect subdirectories when locating Git workspaces ([#5944](https://github.com/astral-sh/uv/pull/5944))

### Documentation

- Improve the `uv venv` CLI documentation ([#5963](https://github.com/astral-sh/uv/pull/5963))

## 0.2.34

### Enhancements

- Always strip in release mode ([#5745](https://github.com/astral-sh/uv/pull/5745))
- Assume `git+` prefix when URLs end in `.git` ([#5868](https://github.com/astral-sh/uv/pull/5868))
- Support build constraints ([#5639](https://github.com/astral-sh/uv/pull/5639))

### CLI

- Create help sections for build, install, resolve, and index ([#5693](https://github.com/astral-sh/uv/pull/5693))
- Improve CLI documentation for global options ([#5834](https://github.com/astral-sh/uv/pull/5834))
- Improve `--python` CLI documentation ([#5869](https://github.com/astral-sh/uv/pull/5869))
- Improve display order of top-level commands ([#5830](https://github.com/astral-sh/uv/pull/5830))

### Bug fixes

- Allow downloading wheels for metadata with `--no-binary` ([#5707](https://github.com/astral-sh/uv/pull/5707))
- Reject `pyproject.toml` in `--config-file` ([#5842](https://github.com/astral-sh/uv/pull/5842))
- Remove double-proxy nodes in error reporting ([#5738](https://github.com/astral-sh/uv/pull/5738))
- Respect pre-release preferences from input files ([#5736](https://github.com/astral-sh/uv/pull/5736))
- Support overlapping local and non-local requirements in forks ([#5812](https://github.com/astral-sh/uv/pull/5812))

## 0.2.33

### Enhancements

- Add support for `ksh` to relocatable virtual environments ([#5640](https://github.com/astral-sh/uv/pull/5640))

### CLI

- Add help sections for global options ([#5665](https://github.com/astral-sh/uv/pull/5665))
- Move `--python` and `--python-version` into the "Python options" help ([#5691](https://github.com/astral-sh/uv/pull/5691))
- Show help specific options (i.e. `--no-pager`) in `uv help` ([#5516](https://github.com/astral-sh/uv/pull/5516))
- Update top-level command descriptions ([#5706](https://github.com/astral-sh/uv/pull/5706))

### Bug fixes

- Remove lingering executables after failed installs ([#5666](https://github.com/astral-sh/uv/pull/5666))
- Switch from heuristic freshness lifetime to hard-coded value ([#5654](https://github.com/astral-sh/uv/pull/5654))

### Documentation

- Don't use equals signs for CLI options with values ([#5704](https://github.com/astral-sh/uv/pull/5704))

## 0.2.32

### Enhancements

- Deprecate the `--isolated` flag in favor of `--no-config` ([#5466](https://github.com/astral-sh/uv/pull/5466))
- Re-enable `requires-python` narrowing in forks ([#5583](https://github.com/astral-sh/uv/pull/5583))

### Performance

- Skip copying to empty entries in seekable zip ([#5571](https://github.com/astral-sh/uv/pull/5571))
- Use a consistent buffer size for downloads ([#5569](https://github.com/astral-sh/uv/pull/5569))
- Use a consistent buffer size when writing out zip files ([#5570](https://github.com/astral-sh/uv/pull/5570))

### Bug fixes

- Avoid setting executable permissions on files we might not own ([#5582](https://github.com/astral-sh/uv/pull/5582))
- Statically link liblzma ([#5577](https://github.com/astral-sh/uv/pull/5577))

## 0.2.31

### Enhancements

- Add `--relocatable` flag to `uv venv` ([#5515](https://github.com/astral-sh/uv/pull/5515))
- Support `xz`-compressed packages ([#5513](https://github.com/astral-sh/uv/pull/5513))
- Warn, but don't error, when encountering tilde `.dist-info` directories ([#5520](https://github.com/astral-sh/uv/pull/5520))

### Bug fixes

- Make `pip list --editable` conflict with `--exclude-editable` ([#5506](https://github.com/astral-sh/uv/pull/5506))
- Add some missing reinstall-refresh calls ([#5497](https://github.com/astral-sh/uv/pull/5497))
- Avoid warning users for missing self-extra lower bounds ([#5518](https://github.com/astral-sh/uv/pull/5518))
- Generate hashes for `--find-links` entries ([#5544](https://github.com/astral-sh/uv/pull/5544))
- Retain editable designation for cached wheel installs ([#5545](https://github.com/astral-sh/uv/pull/5545))
- Use 666 rather than 644 for default permissions ([#5498](https://github.com/astral-sh/uv/pull/5498))
- Retry on incomplete body ([#5555](https://github.com/astral-sh/uv/pull/5555))
- Ban `--no-cache` with `--link-mode=symlink` ([#5519](https://github.com/astral-sh/uv/pull/5519))

## 0.2.30

### Enhancements

- Infer missing `.exe` in Windows Python discovery ([#5456](https://github.com/astral-sh/uv/pull/5456))
- Make `--reinstall` imply `--refresh` ([#5425](https://github.com/astral-sh/uv/pull/5425))

### CLI

- Add `--no-config` to replace `--isolated` ([#5463](https://github.com/astral-sh/uv/pull/5463))
- Cache metadata for source tree dependencies ([#5423](https://github.com/astral-sh/uv/pull/5423))

### Bug fixes

- Avoid canonicalizing executables on Windows ([#5446](https://github.com/astral-sh/uv/pull/5446))
- Set standard permissions for temporary files ([#5457](https://github.com/astral-sh/uv/pull/5457))

## 0.2.29

### Enhancements

- Add `--ci` mode to `uv cache prune` ([#5391](https://github.com/astral-sh/uv/pull/5391))
- Display Python installation key for discovered interpreters ([#5365](https://github.com/astral-sh/uv/pull/5365))

### Bug fixes

- Allow symlinks to files in scripts directory ([#5380](https://github.com/astral-sh/uv/pull/5380))
- Always accept already-installed pre-releases ([#5419](https://github.com/astral-sh/uv/pull/5419))
- Validate successful metadata fetch for direct dependencies ([#5392](https://github.com/astral-sh/uv/pull/5392))

### Documentation

- Add warning to `--link-mode=symlink` documentation ([#5387](https://github.com/astral-sh/uv/pull/5387))

## 0.2.28

### Enhancements

- Output stable ordering to `requirements.txt` in universal mode ([#5334](https://github.com/astral-sh/uv/pull/5334))
- Allow symlinks with `--find-links` ([#5323](https://github.com/astral-sh/uv/pull/5323))
- Add support for variations of `pythonw.exe` ([#5259](https://github.com/astral-sh/uv/pull/5259))

### CLI

- Stylize `Requires-Python` consistently in CLI output ([#5304](https://github.com/astral-sh/uv/pull/5304))
- Add `--show-version-specifiers` to `tree` ([#5240](https://github.com/astral-sh/uv/pull/5240))

### Performance

- Avoid always rebuilding dynamic metadata ([#5206](https://github.com/astral-sh/uv/pull/5206))
- Avoid URL parsing when deserializing wheels ([#5235](https://github.com/astral-sh/uv/pull/5235))

### Bug fixes

- Avoid cache prune failure due to removed interpreter ([#5286](https://github.com/astral-sh/uv/pull/5286))
- Avoid including empty extras in resolution ([#5306](https://github.com/astral-sh/uv/pull/5306))
- If multiple indices contain the same version, use the first index ([#5288](https://github.com/astral-sh/uv/pull/5288))
- Include URLs on graph edges ([#5312](https://github.com/astral-sh/uv/pull/5312))
- Match wheel tags against `Requires-Python` major-minor ([#5289](https://github.com/astral-sh/uv/pull/5289))
- Remove Simple API cache files for alternative indexes in `cache clean` ([#5353](https://github.com/astral-sh/uv/pull/5353))
- Remove extraneous `are` from wheel tag error messages ([#5303](https://github.com/astral-sh/uv/pull/5303))
- Allow conflicting pre-release strategies when forking ([#5150](https://github.com/astral-sh/uv/pull/5150))
- Use tag error rather than requires-python error for ABI filtering ([#5296](https://github.com/astral-sh/uv/pull/5296))

## 0.2.27

### Enhancements

- Add GraalPy support ([#5141](https://github.com/astral-sh/uv/pull/5141))
- Add a `--verify-hashes` hash-checking mode ([#4007](https://github.com/astral-sh/uv/pull/4007))
- Discover all `python3.x` executables in the `PATH` ([#5148](https://github.com/astral-sh/uv/pull/5148))
- Support  `--link-mode=symlink` ([#5208](https://github.com/astral-sh/uv/pull/5208))
- Warn about unconstrained direct deps in lowest resolution ([#5142](https://github.com/astral-sh/uv/pull/5142))
- Log origin of version selection ([#5186](https://github.com/astral-sh/uv/pull/5186))
- Key hash policy on version, rather than package ([#5169](https://github.com/astral-sh/uv/pull/5169))

### CLI

- Make missing project table a tracing warning ([#5194](https://github.com/astral-sh/uv/pull/5194))
- Remove trailing period from user-facing messages ([#5218](https://github.com/astral-sh/uv/pull/5218))

### Bug fixes

- Make entrypoint writes atomic to avoid overwriting symlinks ([#5165](https://github.com/astral-sh/uv/pull/5165))
- Use `which`-retrieved path directly when spawning pager ([#5198](https://github.com/astral-sh/uv/pull/5198))
- Don't apply irrelevant constraints when validating site-packages ([#5231](https://github.com/astral-sh/uv/pull/5231))
- Respect local versions for all user requirements ([#5232](https://github.com/astral-sh/uv/pull/5232))

## 0.2.26

### CLI

- Add `--no-progress` global option to hide all progress animations ([#5098](https://github.com/astral-sh/uv/pull/5098))

### Performance

- Cache downloaded wheel when range requests aren't supported ([#5089](https://github.com/astral-sh/uv/pull/5089))

### Bug fixes

- Download wheel to disk when streaming unzip failed with HTTP streaming error ([#5094](https://github.com/astral-sh/uv/pull/5094))
- Filter out invalid wheels based on `requires-python` ([#5084](https://github.com/astral-sh/uv/pull/5084))
- Filter out none ABI wheels with mismatched Python versions ([#5087](https://github.com/astral-sh/uv/pull/5087))
- Lock Git cache on resolve ([#5051](https://github.com/astral-sh/uv/pull/5051))
- Change order of `pip compile` command checks to handle exact argument first ([#5111](https://github.com/astral-sh/uv/pull/5111))

### Documentation

- Document that `--universal` implies `--no-strip-markers` ([#5121](https://github.com/astral-sh/uv/pull/5121))

## 0.2.25

### Enhancements

- Include PyPy-specific executables when creating virtual environments with `uv venv` ([#5047](https://github.com/astral-sh/uv/pull/5047))
- Add a custom error message for `--no-build-isolation` `torch` dependencies ([#5041](https://github.com/astral-sh/uv/pull/5041))
- Improve missing `wheel` error message with `--no-build-isolation` ([#4964](https://github.com/astral-sh/uv/pull/4964))

### CLI

- Add `--no-pager` option in `help` command ([#5007](https://github.com/astral-sh/uv/pull/5007))
- Unhide `--isolated` global argument ([#5005](https://github.com/astral-sh/uv/pull/5005))
- Warn when unused `pyproject.toml` configuration is detected ([#5025](https://github.com/astral-sh/uv/pull/5025))

### Bug fixes

- Fall back to streaming wheel when `Content-Length` header is absent ([#5000](https://github.com/astral-sh/uv/pull/5000))
- Fix substring marker expression disjointness checks ([#4998](https://github.com/astral-sh/uv/pull/4998))
- Lock directories to synchronize wheel-install copies ([#4978](https://github.com/astral-sh/uv/pull/4978))
- Normalize out complementary == or != markers ([#5050](https://github.com/astral-sh/uv/pull/5050))
- Retry on permission errors when persisting extracted source distributions to the cache ([#5076](https://github.com/astral-sh/uv/pull/5076))
- Set absolute URLs prior to uploading to PyPI ([#5038](https://github.com/astral-sh/uv/pull/5038))
- Exclude `--upgrade-package` from the `pip compile` header ([#5032](https://github.com/astral-sh/uv/pull/5032))
- Exclude `--upgrade-package` when option and value are passed as a single argument ([#5033](https://github.com/astral-sh/uv/pull/5033))
- Add split to cover marker universe when existing splits are incomplete ([#5074](https://github.com/astral-sh/uv/pull/5074))
- Use correct `pyproject.toml` path in warnings ([#5069](https://github.com/astral-sh/uv/pull/5069))

### Documentation

- Fix `CONTRIBUTING.md` instructions to install multiple Python versions ([#5015](https://github.com/astral-sh/uv/pull/5015))
- Use versioned badges when uploading to PyPI ([#5039](https://github.com/astral-sh/uv/pull/5039))

## 0.2.24

### Enhancements

- Add support for 'any' Python requests ([#4948](https://github.com/astral-sh/uv/pull/4948))
- Allow constraints to be provided in `--upgrade-package` ([#4952](https://github.com/astral-sh/uv/pull/4952))
- Add `manylinux_2_31` to supported `--python-platform` ([#4965](https://github.com/astral-sh/uv/pull/4965))
- Improve marker simplification ([#4639](https://github.com/astral-sh/uv/pull/4639))

### CLI

- Display short help menu when `--help` is used ([#4772](https://github.com/astral-sh/uv/pull/4772))
- Allow `uv help` global options during `uv help` ([#4906](https://github.com/astral-sh/uv/pull/4906))
- Use paging for `uv help` display when available ([#4909](https://github.com/astral-sh/uv/pull/4909))

### Performance

- Switch to single threaded async runtime ([#4934](https://github.com/astral-sh/uv/pull/4934))

### Bug fixes

- Avoid AND-ing multi-term specifiers in marker normalization ([#4911](https://github.com/astral-sh/uv/pull/4911))
- Avoid inferring package name for GitHub Archives ([#4928](https://github.com/astral-sh/uv/pull/4928))
- Retry on connection reset network errors ([#4960](https://github.com/astral-sh/uv/pull/4960))
- Apply extra to overrides and constraints ([#4829](https://github.com/astral-sh/uv/pull/4829))

### Rust API

- Allow `uv` crate to be used as a library ([#4642](https://github.com/astral-sh/uv/pull/4642))

## 0.2.23

### Enhancements

- Update Windows trampoline binaries ([#4864](https://github.com/astral-sh/uv/pull/4864))
- Show user-facing warning when falling back to copy installs ([#4880](https://github.com/astral-sh/uv/pull/4880))

### Bug fixes

- Initialize all `--prefix` subdirectories ([#4895](https://github.com/astral-sh/uv/pull/4895))
- Respect `requires-python` when prefetching ([#4900](https://github.com/astral-sh/uv/pull/4900))
- Partially revert `Requires-Python` version narrowing ([#4902](https://github.com/astral-sh/uv/pull/4902))

## 0.2.22

### CLI

- Add `--exclude-newer` to installer arguments ([#4785](https://github.com/astral-sh/uv/pull/4785))
- Bold durations in CLI messages ([#4818](https://github.com/astral-sh/uv/pull/4818))
- Drop crate description from the `uv` help menu ([#4773](https://github.com/astral-sh/uv/pull/4773))
- Update "about" in help menu ([#4782](https://github.com/astral-sh/uv/pull/4782))

### Configuration

- Add `UV_OVERRIDE` environment variable for `--override` ([#4836](https://github.com/astral-sh/uv/pull/4836))

### Bug fixes

- Always use release-only comparisons for `requires-python` ([#4794](https://github.com/astral-sh/uv/pull/4794))
- Avoid hangs before exiting CLI ([#4793](https://github.com/astral-sh/uv/pull/4793))
- Preserve verbatim URLs for `--find-links` ([#4838](https://github.com/astral-sh/uv/pull/4838))

## 0.2.21

- Fix issue where standalone installer failed to due missing `uvx.exe` binary on Windows ([#4756](https://github.com/astral-sh/uv/pull/4756))

### CLI

- Differentiate `freeze` and `list` help text ([#4751](https://github.com/astral-sh/uv/pull/4751))

## 0.2.20

- Fix issue where the standalone installer failed due to a missing `uvx` binary ([#4743](https://github.com/astral-sh/uv/pull/4743))

## 0.2.19

### Enhancements

- Indicate when we retried requests during network errors ([#4725](https://github.com/astral-sh/uv/pull/4725))

### CLI

- Add `--disable-pip-version-check` to compatibility arguments ([#4672](https://github.com/astral-sh/uv/pull/4672))
- Allow `uv pip sync` to clear an environment with opt-in ([#4517](https://github.com/astral-sh/uv/pull/4517))
- Add `--invert` to `uv pip tree` ([#4621](https://github.com/astral-sh/uv/pull/4621))
- Omit `(*)` in `uv pip tree` for empty packages ([#4673](https://github.com/astral-sh/uv/pull/4673))
- Add `--package` to `uv pip tree` ([#4655](https://github.com/astral-sh/uv/pull/4655))

### Bug fixes

- Fix bug where git cache did not validate commits correctly ([#4698](https://github.com/astral-sh/uv/pull/4698))
- Narrow `requires-python` requirement in resolver forks ([#4707](https://github.com/astral-sh/uv/pull/4707))
- Fix bug when pruning the last package in `uv pip tree` ([#4652](https://github.com/astral-sh/uv/pull/4652))

## 0.2.18

### CLI

- Make `--universal` and `--python-platform` mutually exclusive ([#4598](https://github.com/astral-sh/uv/pull/4598))
- Add `--depth` and `--prune` support to `pip tree` ([#4440](https://github.com/astral-sh/uv/pull/4440))

### Bug fixes

- Handle cycles when propagating markers ([#4595](https://github.com/astral-sh/uv/pull/4595))
- Ignore `py` not found errors during interpreter discovery ([#4620](https://github.com/astral-sh/uv/pull/4620))
- Merge markers when applying constraints ([#4648](https://github.com/astral-sh/uv/pull/4648))
- Retry on spurious failures when caching built wheels ([#4605](https://github.com/astral-sh/uv/pull/4605))
- Sort indexes during graph edge removal ([#4649](https://github.com/astral-sh/uv/pull/4649))
- Treat Python version as a lower bound in `--universal` ([#4597](https://github.com/astral-sh/uv/pull/4597))
- Fix the incorrect handling of markers in `pip tree` ([#4611](https://github.com/astral-sh/uv/pull/4611))
- Improve toolchain and environment missing error messages ([#4596](https://github.com/astral-sh/uv/pull/4596))

### Documentation

- Explicitly mention use of seed packages during `uv venv --seed` ([#4588](https://github.com/astral-sh/uv/pull/4588))

## 0.2.17

### Bug fixes

- Avoid enforcing extra-only constraints ([#4570](https://github.com/astral-sh/uv/pull/4570))

## 0.2.16

### Enhancements

- Add a universal resolution mode to `uv pip compile` with `--universal` ([#4505](https://github.com/astral-sh/uv/pull/4505))
- Add support for `--no-strip-markers` in `uv pip compile` output ([#4503](https://github.com/astral-sh/uv/pull/4503))
- Add `--no-dedupe` support to `uv pip tree` ([#4449](https://github.com/astral-sh/uv/pull/4449))

### Bug fixes

- Enable more precise environment locking with `--prefix` ([#4506](https://github.com/astral-sh/uv/pull/4506))
- Allow local index references in `requirements.txt` files ([#4525](https://github.com/astral-sh/uv/pull/4525))
- Allow non-`file://` paths to serve as `--index-url` values ([#4524](https://github.com/astral-sh/uv/pull/4524))
- Make `.egg-info` filename parsing spec compliant ([#4533](https://github.com/astral-sh/uv/pull/4533))
- Gracefully handle non-existent packages in local indexes ([#4545](https://github.com/astral-sh/uv/pull/4545))
- Read content length from response rather than request ([#4488](https://github.com/astral-sh/uv/pull/4488))
- Read persistent configuration from non-workspace `pyproject.toml` ([#4526](https://github.com/astral-sh/uv/pull/4526))
- Avoid panic for invalid, non-base index URLs ([#4527](https://github.com/astral-sh/uv/pull/4527))

### Performance

- Skip submodule update for fresh clones ([#4482](https://github.com/astral-sh/uv/pull/4482))
- Use shared client in Git fetch implementation ([#4487](https://github.com/astral-sh/uv/pull/4487))

## 0.2.15

### Enhancements

- Add `--emit-build-options` flag to `uv pip compile` interface ([#4463](https://github.com/astral-sh/uv/pull/4463))
- Add `pythonw` support for gui scripts on Windows ([#4409](https://github.com/astral-sh/uv/pull/4409))
- Add `uv pip tree` ([#3859](https://github.com/astral-sh/uv/pull/3859))

### CLI

- Adjust the docs for the pip CLI commands ([#4445](https://github.com/astral-sh/uv/pull/4445))
- Fix casing of `--no-compile` alias ([#4453](https://github.com/astral-sh/uv/pull/4453))

### Bug fixes

- Fix ordering of prefer-system toolchain preference ([#4441](https://github.com/astral-sh/uv/pull/4441))
- Respect index strategy in source distribution builds ([#4468](https://github.com/astral-sh/uv/pull/4468))

### Documentation

- Add documentation for using uv in a Docker image ([#4433](https://github.com/astral-sh/uv/pull/4433))

## 0.2.14

### Enhancements

- Support toolchain requests with platform-tag style Python implementations and version ([#4407](https://github.com/astral-sh/uv/pull/4407))

### CLI

- Use "Prepared" instead of "Downloaded" in logs ([#4394](https://github.com/astral-sh/uv/pull/4394))

### Bug fixes

- Treat mismatched directory and file urls as unsatisfied requirements ([#4393](https://github.com/astral-sh/uv/pull/4393))

## 0.2.13

### Enhancements

- Add resolver tracing logs for when we filter requirements ([#4381](https://github.com/astral-sh/uv/pull/4381))

## 0.2.12

### Enhancements

- Allow specific `--only-binary` and `--no-binary` packages to override `:all:` ([#4067](https://github.com/astral-sh/uv/pull/4067))
- Flatten ORs and ANDs in marker construction ([#4260](https://github.com/astral-sh/uv/pull/4260))
- Skip invalid interpreters when searching for requested interpreter executable name ([#4308](https://github.com/astral-sh/uv/pull/4308))
- Display keyring stderr during queries ([#4343](https://github.com/astral-sh/uv/pull/4343))
- Allow discovery of uv binary relative to package root ([#4336](https://github.com/astral-sh/uv/pull/4336))
- Use relative path for `lib64` symlink ([#4268](https://github.com/astral-sh/uv/pull/4268))

### CLI

- Add uv version to debug output ([#4259](https://github.com/astral-sh/uv/pull/4259))
- Allow `--no-binary` with `uv pip compile` ([#4301](https://github.com/astral-sh/uv/pull/4301))
- Hide `--no-system` from the CLI ([#4292](https://github.com/astral-sh/uv/pull/4292))
- Make `--reinstall`, `--upgrade`, and `--refresh` shared arguments ([#4319](https://github.com/astral-sh/uv/pull/4319))

### Configuration

- Add `UV_EXCLUDE_NEWER` environment variable ([#4287](https://github.com/astral-sh/uv/pull/4287))

### Bug fixes

- Allow normalization to completely eliminate markers ([#4271](https://github.com/astral-sh/uv/pull/4271))
- Avoid treating direct path archives as always dynamic ([#4283](https://github.com/astral-sh/uv/pull/4283))
- De-duplicate markers during normalization ([#4263](https://github.com/astral-sh/uv/pull/4263))
- Fix incorrect parsing of requested Python version as empty version specifiers ([#4289](https://github.com/astral-sh/uv/pull/4289))
- Suggest correct command to create a virtual environment when encountering externally managed interpreters ([#4314](https://github.com/astral-sh/uv/pull/4314))
- Use consistent order for extra groups in lockfile ([#4275](https://github.com/astral-sh/uv/pull/4275))

### Documentation

- Add `pip-compile` defaults to `PIP_COMPATIBILITY.md` ([#4302](https://github.com/astral-sh/uv/pull/4302))
- Expand on `pip-compile` default differences ([#4306](https://github.com/astral-sh/uv/pull/4306))
- Tweak copy on some command-line arguments ([#4293](https://github.com/astral-sh/uv/pull/4293))
- Move the preview changelog so the GitHub Release shows stable changes ([#4290](https://github.com/astral-sh/uv/pull/4290))

## 0.2.11

### Enhancements

- Add support for local directories with `--index-url` ([#4226](https://github.com/astral-sh/uv/pull/4226))
- Add mTLS support ([#4171](https://github.com/astral-sh/uv/pull/4171))
- Allow version specifiers to be used in Python version requests ([#4214](https://github.com/astral-sh/uv/pull/4214))

### Bug fixes

- Always install as editable when duplicate dependencies are requested ([#4208](https://github.com/astral-sh/uv/pull/4208))
- Avoid crash with `XDG_CONFIG_HOME=/dev/null` ([#4200](https://github.com/astral-sh/uv/pull/4200))
- Improve handling of missing interpreters during discovery ([#4218](https://github.com/astral-sh/uv/pull/4218))
- Make missing `METADATA` file a recoverable error ([#4247](https://github.com/astral-sh/uv/pull/4247))
- Represent build tag as `u64` ([#4253](https://github.com/astral-sh/uv/pull/4253))

### Documentation

- Document Windows 10 requirement ([#4210](https://github.com/astral-sh/uv/pull/4210))

### Release

- Re-add `aarch64-unknown-linux-gnu` binary to release assets ([#4254](https://github.com/astral-sh/uv/pull/4254))

## 0.2.10

### Enhancements

- Accept `file://` URLs for `requirements.txt` et all references ([#4145](https://github.com/astral-sh/uv/pull/4145))
- Add support for `--prefix` ([#4085](https://github.com/astral-sh/uv/pull/4085))

### CLI

- Add `pyproject.toml` to CLI help ([#4181](https://github.com/astral-sh/uv/pull/4181))
- Drop "registry" prefix from request timeout log ([#4144](https://github.com/astral-sh/uv/pull/4144))

### Bug fixes

- Allow transitive URLs via recursive extras ([#4155](https://github.com/astral-sh/uv/pull/4155))
- Avoid pre-fetching for unbounded minimum versions ([#4149](https://github.com/astral-sh/uv/pull/4149))
- Avoid showing dev hints for Python requirements ([#4111](https://github.com/astral-sh/uv/pull/4111))
- Include non-standard ports in keyring host queries ([#4061](https://github.com/astral-sh/uv/pull/4061))
- Omit URL dependencies from pre-release hints ([#4140](https://github.com/astral-sh/uv/pull/4140))
- Improve static metadata extraction for Poetry projects ([#4182](https://github.com/astral-sh/uv/pull/4182))

### Documentation

- Document bytecode compilation in pip compatibility guide ([#4195](https://github.com/astral-sh/uv/pull/4195))
- Fix PEP 508 link in preview doc `specifying_dependencies` ([#4158](https://github.com/astral-sh/uv/pull/4158))
- Clarify role of `--system` flag ([#4031](https://github.com/astral-sh/uv/pull/4031))

## 0.2.9

### Enhancements

- Respect existing `.egg-link` files in site packages ([#4082](https://github.com/astral-sh/uv/pull/4082))

### Bug fixes

- Avoid extra-only filtering for constraints ([#4095](https://github.com/astral-sh/uv/pull/4095))

### Documentation

- Add install link for specific version to README ([#4105](https://github.com/astral-sh/uv/pull/4105))

## 0.2.8

### Bug fixes

- Fix `uv venv` handling when `VIRTUAL_ENV` refers to an non-existent environment ([#4073](https://github.com/astral-sh/uv/pull/4073))

## 0.2.7

### CLI

- Support `NO_COLOR` and `FORCE_COLOR` environment variables ([#3979](https://github.com/astral-sh/uv/pull/3979))

### Performance

- Avoid building packages with dynamic versions ([#4058](https://github.com/astral-sh/uv/pull/4058))
- Avoid work-stealing in bytecode compilation ([#4004](https://github.com/astral-sh/uv/pull/4004))

### Bug fixes

- Avoid dropping `pip sync` requirements with markers ([#4051](https://github.com/astral-sh/uv/pull/4051))
- Bias towards local directories for bare editable requirements ([#3995](https://github.com/astral-sh/uv/pull/3995))
- Preserve fragments when applying verbatim redirects ([#4038](https://github.com/astral-sh/uv/pull/4038))
- Avoid 'are incompatible' for singular bounded versions ([#4003](https://github.com/astral-sh/uv/pull/4003))

## 0.2.6

### Enhancements

- Support PEP 508 requirements for editables ([#3946](https://github.com/astral-sh/uv/pull/3946))
- Discard fragments when parsing unnamed URLs ([#3940](https://github.com/astral-sh/uv/pull/3940))
- Port all Git functionality to use Git CLI ([#3833](https://github.com/astral-sh/uv/pull/3833))
- Use statically linked C runtime on Windows ([#3966](https://github.com/astral-sh/uv/pull/3966))

### Bug fixes

- Disable concurrent progress bars in Jupyter Notebooks ([#3890](https://github.com/astral-sh/uv/pull/3890))
- Initialize multi-progress state before individual bars ([#3901](https://github.com/astral-sh/uv/pull/3901))
- Add missing `i686` alias for `x86` ([#3899](https://github.com/astral-sh/uv/pull/3899))
- Add missing `ppc64le` alias for `powerpc64le` ([#3963](https://github.com/astral-sh/uv/pull/3963))
- Fix reference to `--python-version` patch behavior ([#3989](https://github.com/astral-sh/uv/pull/3989))
- Avoid race condition in `OnceMap` ([#3987](https://github.com/astral-sh/uv/pull/3987))

## 0.2.5

### Enhancements

- Add support for x86 Windows ([#3873](https://github.com/astral-sh/uv/pull/3873))
- Add support for `prepare_metadata_for_build_editable` hook ([#3870](https://github.com/astral-sh/uv/pull/3870))
- Add concurrent progress bars for downloads ([#3252](https://github.com/astral-sh/uv/pull/3252))

### Bug fixes

- Update bundled Python URLs and add `"arm"` architecture variant ([#3855](https://github.com/astral-sh/uv/pull/3855))

## 0.2.4

### CLI

- Allow `--system` and `--python` to be passed together ([#3830](https://github.com/astral-sh/uv/pull/3830))

### Bug fixes

- Ignore `libc` on other platforms ([#3825](https://github.com/astral-sh/uv/pull/3825))

## 0.2.3

### Enhancements

- Incorporate build tag into wheel prioritization ([#3781](https://github.com/astral-sh/uv/pull/3781))
- Avoid displaying log for satisfied editables if none are requested ([#3795](https://github.com/astral-sh/uv/pull/3795))
- Improve logging during interpreter discovery ([#3790](https://github.com/astral-sh/uv/pull/3790))
- Improve logging for environment locking ([#3792](https://github.com/astral-sh/uv/pull/3792))
- Improve logging of interpreter implementation ([#3791](https://github.com/astral-sh/uv/pull/3791))
- Remove extra details from interpreter query traces ([#3803](https://github.com/astral-sh/uv/pull/3803))
- Use colon more consistently in error messages ([#3788](https://github.com/astral-sh/uv/pull/3788))

### Configuration

- Add JSON alias for `unsafe-any-match` ([#3820](https://github.com/astral-sh/uv/pull/3820))

### Release

- Remove redundant dynamically linked Linux binary again (#3762)" ([#3778](https://github.com/astral-sh/uv/pull/3778))
- Remove `aarch64-unknown-linux-gnu` from list of expected binaries ([#3761](https://github.com/astral-sh/uv/pull/3761))

### Bug fixes

- Always include package names for Git and HTTPS dependencies ([#3821](https://github.com/astral-sh/uv/pull/3821))
- Fix interpreter cache collisions for relative virtualenv paths ([#3823](https://github.com/astral-sh/uv/pull/3823))
- Ignore unnamed requirements in preferences ([#3826](https://github.com/astral-sh/uv/pull/3826))
- Search for `python3` in unix virtual environments ([#3798](https://github.com/astral-sh/uv/pull/3798))
- Use a cross-platform representation for relative paths in `pip compile` ([#3804](https://github.com/astral-sh/uv/pull/3804))

## 0.2.2

### Enhancements

- Report yanks for cached and resolved packages ([#3772](https://github.com/astral-sh/uv/pull/3772))
- Improve error message when default Python is not found ([#3770](https://github.com/astral-sh/uv/pull/3770))

### Bug fixes

- Do not treat interpereters discovered via `CONDA_PREFIX` as system interpreters ([#3771](https://github.com/astral-sh/uv/pull/3771))

## 0.2.1

### Bug fixes

- Re-added the dynamically-linked Linux binary ([#3762](https://github.com/astral-sh/uv/pull/3762))

## 0.2.0

Starting with this release, uv will use the **minor** version tag to indicate breaking changes.

### Breaking

In this release, discovery of Python interpreters has changed. These changes should have a limited effect in most
use-cases, however, it has been marked as a breaking change because the interpreter used by uv could change in
some edge cases.

When multiple Python interpreters are installed, uv makes an attempt to find the exact version you requested.
Previously, uv would stop at the first Python interpreter it discovered — if the interpreter did not satisfy
the requested version, uv would fail. Now, uv will query multiple Python interpreters until it finds the
requested version, skipping interpreters that are broken or do not satisfy the request.

Additionally, uv now allows requests for interpreter implementations such as `pypy` and `cpython`. For example,
the request `--python cpython` will ignore a `python` executable that's implemented by `pypy`. These requests may
also include a version, e.g., `--python pypy@3.10`. By default, uv will accept *any* interpreter implementation.

In summary, the following Python interpreter requests are now allowed:

- A Python version without an implementation name, e.g., `3.10`
- A path to a directory containing a Python installation, e.g., `./foo/.venv`
- A path to a Python executable, e.g., `~/bin/python`
- A Python implementation without a version, e.g., `pypy` or `cpython`
- A Python implementation name and version, e.g., `pypy3.8` or `pypy@3.8`
- The name of a Python executable (for lookup in the `PATH`), e.g., `foopython3`

Previously, interpreter requests that were not versions or paths were always treated as executable
names.

To align the user expectations, uv now respects the interpreter that starts it. For example, `python -m uv ...` will
now prefer the `python` interpreter that was used to start uv instead of searching for a virtual environment.

We now check if discovered interpreters are virtual environments. This means that setting `VIRTUAL_ENV` to a Python
installation directory that is *not* a virtual environment will no longer work. Instead, use `--system` or `--python <path>`
to request the interpreter.

### Enhancements

- Rewrite Python interpreter discovery ([#3266](https://github.com/astral-sh/uv/pull/3266))
- Add support for requesting `pypy` interpreters by implementation name ([#3706](https://github.com/astral-sh/uv/pull/3706))
- Discover and prefer the parent interpreter when invoked with `python -m uv` [#3736](https://github.com/astral-sh/uv/pull/3736)
- Add PEP 714 support for HTML API client ([#3697](https://github.com/astral-sh/uv/pull/3697))
- Add PEP 714 support for JSON API client ([#3698](https://github.com/astral-sh/uv/pull/3698))
- Write relative paths with unnamed requirement syntax ([#3682](https://github.com/astral-sh/uv/pull/3682))
- Allow relative Python executable paths in Windows trampoline ([#3717](https://github.com/astral-sh/uv/pull/3717))
- Add support for clang and msvc in missing header error ([#3753](https://github.com/astral-sh/uv/pull/3753))

### CLI

- Allow `--constraint` files in `pip sync` ([#3741](https://github.com/astral-sh/uv/pull/3741))
- Allow `--config-file` to be passed before or after command name ([#3730](https://github.com/astral-sh/uv/pull/3730))
- Make `--offline` a global argument ([#3729](https://github.com/astral-sh/uv/pull/3729))

### Performance

- Improve performance in complex resolutions by reducing cost of PubGrub package clones ([#3688](https://github.com/astral-sh/uv/pull/3688))

### Bug fixes

- Evaluate arbitrary markers to `false` ([#3681](https://github.com/astral-sh/uv/pull/3681))
- Improve `DirWithoutEntrypoint` error message ([#3690](https://github.com/astral-sh/uv/pull/3690))
- Improve display of root package in range errors ([#3711](https://github.com/astral-sh/uv/pull/3711))
- Propagate URL errors in verbatim parsing ([#3720](https://github.com/astral-sh/uv/pull/3720))
- Report yanked packages in `--dry-run` ([#3740](https://github.com/astral-sh/uv/pull/3740))

### Release

- Drop native `manylinux` wheel in favor of dual-tagged wheel ([#3685](https://github.com/astral-sh/uv/pull/3685))
- The `python-patch` test feature is no longer on by default and must be manually enabled to test patch version behavior ([#3746](https://github.com/astral-sh/uv/pull/3746))

### Documentation

- Add `--prefix` link to compatibility guide ([#3734](https://github.com/astral-sh/uv/pull/3734))
- Add `--only-binary` to compatibility guide ([#3735](https://github.com/astral-sh/uv/pull/3735))
- Add instructions for building and updating `uv-trampolines` ([#3731](https://github.com/astral-sh/uv/pull/3731))
- Add notes for testing on Windows ([#3658](https://github.com/astral-sh/uv/pull/3658))

## 0.1.45

### Enhancements

- Parse and store extras on editable requirements ([#3629](https://github.com/astral-sh/uv/pull/3629))
- Allow local versions in wheel filenames ([#3596](https://github.com/astral-sh/uv/pull/3596))
- Create lib64 symlink for 64-bit, non-macOS, POSIX environments ([#3584](https://github.com/astral-sh/uv/pull/3584))

### Configuration

- Add `UV_CONCURRENT_INSTALLS` variable in favor of `RAYON_NUM_THREADS` ([#3646](https://github.com/astral-sh/uv/pull/3646))
- Add serialization and deserialization for `--find-links` ([#3619](https://github.com/astral-sh/uv/pull/3619))
- Apply combination logic to merge CLI and persistent configuration ([#3618](https://github.com/astral-sh/uv/pull/3618))

### Performance

- Parallelize resolver ([#3627](https://github.com/astral-sh/uv/pull/3627))

### Bug fixes

- Reduce sensitivity of unknown option error to discard Python 2 interpreters ([#3580](https://github.com/astral-sh/uv/pull/3580))
- Respect installed packages in `uv run` ([#3603](https://github.com/astral-sh/uv/pull/3603))
- Separate cache construction from initialization ([#3607](https://github.com/astral-sh/uv/pull/3607))
- Add missing `"directory"` branch in source match ([#3608](https://github.com/astral-sh/uv/pull/3608))
- Fix source annotation in pip compile `annotation-style=line` output ([#3637](https://github.com/astral-sh/uv/pull/3637))
- Run cargo update to pull in h2 ([#3638](https://github.com/astral-sh/uv/pull/3638))
- URL-decode hashes in HTML fragments ([#3655](https://github.com/astral-sh/uv/pull/3655))
- Always print JSON output with `--format` json ([#3671](https://github.com/astral-sh/uv/pull/3671))

### Documentation

- Add `UV_CONFIG_FILE` environment variable to documentation ([#3653](https://github.com/astral-sh/uv/pull/3653))
- Explicitly mention `--user` in compatibility guide ([#3666](https://github.com/astral-sh/uv/pull/3666))

### Release

- Add musl ppc64le support ([#3537](https://github.com/astral-sh/uv/pull/3537))
- Retag musl aarch64 for manylinux2014 ([#3624](https://github.com/astral-sh/uv/pull/3624))

## 0.1.44

### Release

Reverts "Use manylinux: auto to enable `musllinux_1_2` aarch64 builds ([#3444](https://github.com/astral-sh/uv/pull/3444))"

The manylinux change appeared to introduce SSL errors when building aarch64 Docker images, e.g.,

> invalid peer certificate: BadSignature

The v0.1.42 behavior for aarch64 manylinux builds is restored in this release.

See [#3576](https://github.com/astral-sh/uv/pull/3576)

## 0.1.43

### Enhancements

- Annotate sources of requirements in `pip compile` output ([#3269](https://github.com/astral-sh/uv/pull/3269))
- Track origin for `setup.py` files and friends ([#3481](https://github.com/astral-sh/uv/pull/3481))

### Configuration

- Consolidate concurrency limits and expose as environment variables ([#3493](https://github.com/astral-sh/uv/pull/3493))

### Release

- Use manylinux: auto to enable `musllinux_1_2` aarch64 builds ([#3444](https://github.com/astral-sh/uv/pull/3444))
- Enable musllinux_1_1 wheels ([#3523](https://github.com/astral-sh/uv/pull/3523))

### Bug fixes

- Avoid keyword arguments for PEP 517 build hooks ([#3517](https://github.com/astral-sh/uv/pull/3517))
- Apply advisory locks when building source distributions ([#3525](https://github.com/astral-sh/uv/pull/3525))
- Avoid attempting to build editables when fetching metadata ([#3563](https://github.com/astral-sh/uv/pull/3563))
- Clone individual files on windows ReFS ([#3551](https://github.com/astral-sh/uv/pull/3551))
- Filter irrelevant requirements from source annotations ([#3479](https://github.com/astral-sh/uv/pull/3479))
- Make cache clearing robust to directories without read permissions ([#3524](https://github.com/astral-sh/uv/pull/3524))
- Respect constraints on editable dependencies ([#3554](https://github.com/astral-sh/uv/pull/3554))
- Skip Python 2 versions when locating Python ([#3476](https://github.com/astral-sh/uv/pull/3476))
- Make `--isolated` a global argument ([#3558](https://github.com/astral-sh/uv/pull/3558))
- Allow unknown `pyproject.toml` fields ([#3511](https://github.com/astral-sh/uv/pull/3511))
- Change error value detection for glibc ([#3487](https://github.com/astral-sh/uv/pull/3487))

## 0.1.42

This release includes stabilized support for persistent configuration in uv.

uv will now read project configuration from a `pyproject.toml` or `uv.toml` file in the current
directory or any parent directory, along with user configuration at `~/.config/uv/uv.toml`
(or `$XDG_CONFIG_HOME/uv/uv.toml`) on macOS and Linux, and `%APPDATA%\uv\uv.toml` on Windows.

See: [Persistent Configuration](https://github.com/astral-sh/uv?tab=readme-ov-file#persistent-configuration) for more.

### Enhancements

- Respect `MACOSX_DEPLOYMENT_TARGET` in `--python-platform` ([#3470](https://github.com/astral-sh/uv/pull/3470))

### Configuration

- Add documentation for persistent configuration ([#3467](https://github.com/astral-sh/uv/pull/3467))
- Add JSON Schema export to SchemaStore ([#3461](https://github.com/astral-sh/uv/pull/3461))
- Merge user and workspace settings ([#3462](https://github.com/astral-sh/uv/pull/3462))

### Bug fixes

- Use Metadata10 to parse PKG-INFO of legacy editable ([#3450](https://github.com/astral-sh/uv/pull/3450))
- Apply normcase to line from easy-install.pth ([#3451](https://github.com/astral-sh/uv/pull/3451))
- Upgrade `async_http_range_reader` to v0.8.0 to respect redirects in range requests ([#3460](https://github.com/astral-sh/uv/pull/3460))
- Use last non-EOL version for `--python-platform` macOS ([#3469](https://github.com/astral-sh/uv/pull/3469))

## 0.1.41

### Bug fixes

- Remove unconstrained version error from requirements ([#3443](https://github.com/astral-sh/uv/pull/3443))

## 0.1.40

### Enhancements

- Add `--allow-existing` to overwrite existing virtualenv ([#2548](https://github.com/astral-sh/uv/pull/2548))
- Respect and enable uninstalls of legacy editables (`.egg-link`) ([#3415](https://github.com/astral-sh/uv/pull/3415))
- Respect and enable uninstalls of existing `.egg-info` packages ([#3380](https://github.com/astral-sh/uv/pull/3380))

### CLI

- Accept `--no-upgrade`, `--no-refresh`, etc. on the CLI ([#3328](https://github.com/astral-sh/uv/pull/3328))

### Configuration

- Expose `UV_NO_BUILD_ISOLATION` as environment variable ([#3318](https://github.com/astral-sh/uv/pull/3318))
- Expose `UV_PYTHON` as an environment variable ([#3284](https://github.com/astral-sh/uv/pull/3284))
- Expose `UV_LINK_MODE` as environment variable ([#3315](https://github.com/astral-sh/uv/pull/3315))
- Add `UV_CUSTOM_COMPILE_COMMAND` to environment variable docs ([#3382](https://github.com/astral-sh/uv/pull/3382))

### Bug fixes

- Ignore 401 HTTP responses with multiple indexes ([#3292](https://github.com/astral-sh/uv/pull/3292))
- Avoid panic for file URLs ([#3306](https://github.com/astral-sh/uv/pull/3306))
- Quote version parse errors consistently ([#3325](https://github.com/astral-sh/uv/pull/3325))
- Detect current environment when `uv` is invoked from within a virtualenv ([#3379](https://github.com/astral-sh/uv/pull/3379))
- Unset target when creating virtual environments ([#3362](https://github.com/astral-sh/uv/pull/3362))
- Update activation scripts from virtualenv ([#3376](https://github.com/astral-sh/uv/pull/3376))
- Use canonical URLs in satisfaction check ([#3373](https://github.com/astral-sh/uv/pull/3373))

## 0.1.39

### Enhancements

- Add `--target` support to `sync` and `install` ([#3257](https://github.com/astral-sh/uv/pull/3257))
- Implement `--index-strategy unsafe-best-match` ([#3138](https://github.com/astral-sh/uv/pull/3138))

### Bug fixes

- Fix `platform_machine` tag for `--python-platform` on macOS ARM ([#3267](https://github.com/astral-sh/uv/pull/3267))

### Release

- Build a separate ARM wheel for macOS ([#3268](https://github.com/astral-sh/uv/pull/3268))
- Use `macos-12` to build release wheels ([#3264](https://github.com/astral-sh/uv/pull/3264))

## 0.1.38

### Enhancements

- Add alternate manylinux targets to `--python-platform` ([#3229](https://github.com/astral-sh/uv/pull/3229))
- An enum and backticks for lookahead error ([#3216](https://github.com/astral-sh/uv/pull/3216))
- Upgrade macOS target to `12.0` ([#3228](https://github.com/astral-sh/uv/pull/3228))
- Add keyring logs for URL and host fetches ([#3212](https://github.com/astral-sh/uv/pull/3212))
- Combine unresolvable error dependency clauses with the same root ([#3225](https://github.com/astral-sh/uv/pull/3225))

### CLI

- Gave a better name to the `--color` placeholder ([#3226](https://github.com/astral-sh/uv/pull/3226))
- Warn when an unsupported Python version is encountered ([#3250](https://github.com/astral-sh/uv/pull/3250))

### Configuration

- Use directory instead of file when searching for `uv.toml` file ([#3203](https://github.com/astral-sh/uv/pull/3203))

### Performance

- Only perform fetches of credentials for a realm and username combination once ([#3237](https://github.com/astral-sh/uv/pull/3237))
- Unroll self-dependencies via extras ([#3230](https://github.com/astral-sh/uv/pull/3230))
- Use read-write locks instead of mutexes in authentication handling ([#3210](https://github.com/astral-sh/uv/pull/3210))

### Bug fixes

- Avoid removing quites from requirements markers ([#3214](https://github.com/astral-sh/uv/pull/3214))
- Avoid adding extras when expanding constraints ([#3232](https://github.com/astral-sh/uv/pull/3232))
- Reinstall package when editable label is removed ([#3219](https://github.com/astral-sh/uv/pull/3219))

### Documentation

- Add `RAYON_NUM_THREADS` to environment variable docs ([#3223](https://github.com/astral-sh/uv/pull/3223))
- Document support for HTTP proxy variables ([#3247](https://github.com/astral-sh/uv/pull/3247))
- Fix documentation for `--python-platfor`m ([#3220](https://github.com/astral-sh/uv/pull/3220))

## 0.1.37

### Enhancements

- Change default HTTP read timeout to 30s ([#3182](https://github.com/astral-sh/uv/pull/3182))
- Add `--python-platform` to `sync` and `install` commands ([#3154](https://github.com/astral-sh/uv/pull/3154))
- Add ticks around error messages more consistently ([#3004](https://github.com/astral-sh/uv/pull/3004))
- Fix Docker publish permissions in release pipeline ([#3195](https://github.com/astral-sh/uv/pull/3195))
- Improve tracing for keyring provider ([#3207](https://github.com/astral-sh/uv/pull/3207))

### Performance

- Update keyring provider to be async ([#3089](https://github.com/astral-sh/uv/pull/3089))

### Bug fixes

- Fix fetch of credentials when cache is seeded with username ([#3206](https://github.com/astral-sh/uv/pull/3206))

### Documentation

- Improve `--python-platform` documentation ([#3202](https://github.com/astral-sh/uv/pull/3202))

## 0.1.36

### Enhancements

- Add support for embedded Python on Windows ([#3161](https://github.com/astral-sh/uv/pull/3161))
- Add Docker image publishing to release pipeline ([#3155](https://github.com/astral-sh/uv/pull/3155))

### Configuration

- Add `UV_CONSTRAINT` environment variable to provide value for `--constraint` ([#3162](https://github.com/astral-sh/uv/pull/3162))

### Bug fixes

- Avoid waiting for metadata for `--no-deps` editables ([#3188](https://github.com/astral-sh/uv/pull/3188))
- Fix `venvlauncher.exe` reference in venv creation ([#3160](https://github.com/astral-sh/uv/pull/3160))
- Fix authentication for URLs with a shared realm ([#3130](https://github.com/astral-sh/uv/pull/3130))
- Restrict observed requirements to direct when `--no-deps` is specified ([#3191](https://github.com/astral-sh/uv/pull/3191))

### Documentation

- Add a versioning policy to the README ([#3151](https://github.com/astral-sh/uv/pull/3151))

## 0.1.35

### Enhancements

- Add a `--python-platform` argument to enable resolving against a target platform ([#3111](https://github.com/astral-sh/uv/pull/3111))
- Enforce HTTP timeouts on a per-read (rather than per-request) basis ([#3144](https://github.com/astral-sh/uv/pull/3144))

### Bug fixes

- Avoid preferring constrained over unconstrained packages ([#3148](https://github.com/astral-sh/uv/pull/3148))
- Allow `UV_SYSTEM_PYTHON=1` in addition to `UV_SYSTEM_PYTHON=true` ([#3136](https://github.com/astral-sh/uv/pull/3136))

## 0.1.34

### CLI

- Allow `--python` and `--system` on `pip compile` ([#3115](https://github.com/astral-sh/uv/pull/3115))
- Remove `Option<bool>` for `--no-cache` ([#3129](https://github.com/astral-sh/uv/pull/3129))
- Rename `--compile` to `--compile-bytecode` ([#3102](https://github.com/astral-sh/uv/pull/3102))
- Accept `0`, `1`, and similar values for Boolean environment variables ([#3113](https://github.com/astral-sh/uv/pull/3113))

### Configuration

- Add `UV_REQUIRE_HASHES` environment variable ([#3125](https://github.com/astral-sh/uv/pull/3125))
- Add negation flags to the CLI ([#3050](https://github.com/astral-sh/uv/pull/3050))

### Bug fixes

- Avoid fetching unnecessary extra versions during resolution ([#3100](https://github.com/astral-sh/uv/pull/3100))
- Avoid deprioritizing recursive editables ([#3133](https://github.com/astral-sh/uv/pull/3133))
- Avoid treating localhost URLs as local file paths ([#3132](https://github.com/astral-sh/uv/pull/3132))
- Hide password in the index printed via `--emit-index-annotation` ([#3112](https://github.com/astral-sh/uv/pull/3112))
- Restore seeding of authentication cache from index URLs ([#3124](https://github.com/astral-sh/uv/pull/3124))

## 0.1.33

### Breaking changes

Using the keyring requires a username to be provided on index URLs now. Previously, the username `oauth2accesstoken`
was assumed. This will affect Google Artifact Registry users using `--keyring-provider subprocess` and an index URL
without a username. The suggested fix is to add the required username to index URLs, e.g., `https://oauth2accesstoken@<url>`.

See [#2976](https://github.com/astral-sh/uv/pull/2976#discussion_r1566521453) for details.

### Enhancements

- Allow passing a virtual environment path to `uv pip --python` ([#3064](https://github.com/astral-sh/uv/pull/3064))
- Add compatibility argument for `pip list --outdated` ([#3055](https://github.com/astral-sh/uv/pull/3055))

### CLI

- Enable auto-wrapping of `--help` output ([#3058](https://github.com/astral-sh/uv/pull/3058))
- Show `--require-hashes` CLI argument in help ([#3093](https://github.com/astral-sh/uv/pull/3093))

### Performance

- Incorporate heuristics to improve package prioritization ([#3087](https://github.com/astral-sh/uv/pull/3087))

### Bug fixes

- Fix HTTP authentication when the password includes percent encoded characters (e.g. with Google Artifact Registry) ([#2822](https://github.com/astral-sh/uv/issues/2822))
- Use usernames from URLs when looking for credentials in netrc files and the keyring [#2563](https://github.com/astral-sh/uv/issues/2563))
- Skip `HEAD` requests for indexes that return 403 (e.g. PyPICloud) ([#3070](https://github.com/astral-sh/uv/pull/3070))
- Use kebab-case consistently ([#3080](https://github.com/astral-sh/uv/pull/3080))
- Show package name in no version for direct dependency error ([#3056](https://github.com/astral-sh/uv/pull/3056))
- Avoid erroring when encountering `.tar.bz2` source distributions ([#3069](https://github.com/astral-sh/uv/pull/3069))

## 0.1.32

### Enhancements

- Add a `--require-hashes` command-line setting ([#2824](https://github.com/astral-sh/uv/pull/2824))
- Add hash-checking support to `install` and `sync` ([#2945](https://github.com/astral-sh/uv/pull/2945))
- Add support for URL requirements in `--generate-hashes` ([#2952](https://github.com/astral-sh/uv/pull/2952))
- Allow unnamed requirements for overrides ([#2999](https://github.com/astral-sh/uv/pull/2999))
- Enforce and backtrack on invalid versions in source metadata ([#2954](https://github.com/astral-sh/uv/pull/2954))
- Fall back to distributions without hashes in resolver ([#2949](https://github.com/astral-sh/uv/pull/2949))
- Implement `--emit-index-annotation` to annotate source index for each package ([#2926](https://github.com/astral-sh/uv/pull/2926))
- Log hard-link failures ([#3015](https://github.com/astral-sh/uv/pull/3015))
- Support free-threaded Python ([#2805](https://github.com/astral-sh/uv/pull/2805))
- Support unnamed requirements in `--require-hashes` ([#2993](https://github.com/astral-sh/uv/pull/2993))
- Respect link mode for builds, in `uv pip compile` and for `uv venv` seed packages ([#3016](https://github.com/astral-sh/uv/pull/3016))
- Force color for build error messages ([#3032](https://github.com/astral-sh/uv/pull/3032))
- Surface invalid metadata as hints in error reports ([#2850](https://github.com/astral-sh/uv/pull/2850))

### Configuration

- Add `UV_BREAK_SYSTEM_PACKAGES` environment variable ([#2995](https://github.com/astral-sh/uv/pull/2995))

### CLI

- Remove some restrictions in argument groups ([#3001](https://github.com/astral-sh/uv/pull/3001))

### Bug fixes

- Add `--find-links` source distributions to the registry cache ([#2986](https://github.com/astral-sh/uv/pull/2986))
- Allow comments after all `requirements.txt` entries ([#3018](https://github.com/astral-sh/uv/pull/3018))
- Avoid cache invalidation on credentials renewal ([#3010](https://github.com/astral-sh/uv/pull/3010))
- Avoid calling `normalize_path` with relative paths that extend beyond the current directory ([#3013](https://github.com/astral-sh/uv/pull/3013))
- Deduplicate symbolic links between `purelib` and `platlib` ([#3002](https://github.com/astral-sh/uv/pull/3002))
- Remove unused `--output-file` from `pip install` ([#2975](https://github.com/astral-sh/uv/pull/2975))
- Strip query string when parsing filename from HTML index ([#2961](https://github.com/astral-sh/uv/pull/2961))
- Update hashes without `--upgrade` if not present ([#2966](https://github.com/astral-sh/uv/pull/2966))

## 0.1.31

### Bug fixes

- Ignore direct URL distributions in prefetcher ([#2943](https://github.com/astral-sh/uv/pull/2943))

## 0.1.30

### Enhancements

- Show resolution diagnostics after `pip install` ([#2829](https://github.com/astral-sh/uv/pull/2829))

### Performance

- Speed up cold-cache `urllib3`-`boto3`-`botocore` performance with batched prefetching ([#2452](https://github.com/astral-sh/uv/pull/2452))

### Bug fixes

- Backtrack on distributions with invalid metadata ([#2834](https://github.com/astral-sh/uv/pull/2834))
- Include LICENSE files in source distribution ([#2855](https://github.com/astral-sh/uv/pull/2855))
- Respect `--no-build` and `--no-binary` in `--find-links` ([#2826](https://github.com/astral-sh/uv/pull/2826))
- Respect cached local `--find-links` in install plan ([#2907](https://github.com/astral-sh/uv/pull/2907))
- Avoid panic with multiple confirmation handlers ([#2903](https://github.com/astral-sh/uv/pull/2903))
- Use scheme parsing to determine absolute vs. relative URLs ([#2904](https://github.com/astral-sh/uv/pull/2904))
- Remove additional 'because' in resolution failure messages ([#2849](https://github.com/astral-sh/uv/pull/2849))
- Use `miette` when printing `pip sync` resolution failures ([#2848](https://github.com/astral-sh/uv/pull/2848))

## 0.1.29

### Enhancements

- Allow conflicting Git URLs that refer to the same commit SHA ([#2769](https://github.com/astral-sh/uv/pull/2769))
- Allow package lookups across multiple indexes via explicit opt-in (`--index-strategy unsafe-any-match`) ([#2815](https://github.com/astral-sh/uv/pull/2815))
- Allow no-op `--no-compile` flag on CLI ([#2816](https://github.com/astral-sh/uv/pull/2816))
- Upgrade `rs-async-zip` to support data descriptors ([#2809](https://github.com/astral-sh/uv/pull/2809))

### Bug fixes

- Avoid unused extras check in `pip install` for source trees ([#2811](https://github.com/astral-sh/uv/pull/2811))
- Deduplicate editables during install commands ([#2820](https://github.com/astral-sh/uv/pull/2820))
- Fix windows lock race: lock exclusive after all try lock errors ([#2800](https://github.com/astral-sh/uv/pull/2800))
- Preserve `.git` suffixes and casing in Git dependencies ([#2789](https://github.com/astral-sh/uv/pull/2789))
- Respect Git tags and branches that look like short commits ([#2795](https://github.com/astral-sh/uv/pull/2795))
- Enable virtualenv creation on Windows with cpython-x86 ([#2707](https://github.com/astral-sh/uv/pull/2707))

### Documentation

- Document that uv is safe to run concurrently ([#2818](https://github.com/astral-sh/uv/pull/2818))

## 0.1.28

### Enhancements

- Recursively resolve direct URL references upfront ([#2684](https://github.com/astral-sh/uv/pull/2684))

### Performance

- Populate the in-memory index when resolving lookahead URLs ([#2761](https://github.com/astral-sh/uv/pull/2761))

### Bug fixes

- Detect Fish via `FISH_VERSION` ([#2781](https://github.com/astral-sh/uv/pull/2781))
- Exclude installed distributions with multiple versions from consideration in the resolver ([#2779](https://github.com/astral-sh/uv/pull/2779))
- Resolve non-determistic behavior in preferences due to site-packages ordering ([#2780](https://github.com/astral-sh/uv/pull/2780))
- Use canonical URL to key redirect map ([#2764](https://github.com/astral-sh/uv/pull/2764))
- Use distribution database and index for all pre-resolution phases ([#2766](https://github.com/astral-sh/uv/pull/2766))
- Fix `uv self update` on Linux ([#2783](https://github.com/astral-sh/uv/pull/2783))

## 0.1.27

### Enhancements

- Add `--exclude-editable` support to `pip-freeze` ([#2740](https://github.com/astral-sh/uv/pull/2740))
- Add `pyproject.toml` et al to list of prompted packages ([#2746](https://github.com/astral-sh/uv/pull/2746))
- Consider installed packages during resolution ([#2596](https://github.com/astral-sh/uv/pull/2596))
- Recursively allow URL requirements for local dependencies ([#2702](https://github.com/astral-sh/uv/pull/2702))

### Configuration

- Add `UV_RESOLUTION` environment variable for `--resolution` ([#2720](https://github.com/astral-sh/uv/pull/2720))

### Bug fixes

- Respect overrides in all direct-dependency iterators ([#2742](https://github.com/astral-sh/uv/pull/2742))
- Respect subdirectories when reading static metadata ([#2728](https://github.com/astral-sh/uv/pull/2728))

## 0.1.26

### Bug fixes

- Bump simple cache version ([#2712](https://github.com/astral-sh/uv/pull/2712))

## 0.1.25

### Breaking changes

- Limit overrides and constraints to `requirements.txt` format ([#2632](https://github.com/astral-sh/uv/pull/2632))

### Enhancements

- Accept `setup.py` and `setup.cfg` files in compile ([#2634](https://github.com/astral-sh/uv/pull/2634))
- Add `--no-binary` and `--only-binary` support to `requirements.txt` ([#2680](https://github.com/astral-sh/uv/pull/2680))
- Allow pre-releases, locals, and URLs in non-editable path requirements ([#2671](https://github.com/astral-sh/uv/pull/2671))
- Use PEP 517 to extract dynamic `pyproject.toml` metadata ([#2633](https://github.com/astral-sh/uv/pull/2633))
- Add `Editable project location` and `Required-by` to `pip show` ([#2589](https://github.com/astral-sh/uv/pull/2589))
- Avoid `prepare_metadata_for_build_wheel` calls for Hatch packages with dynamic dependencies ([#2645](https://github.com/astral-sh/uv/pull/2645))
- Fall back to PEP 517 hooks for non-compliant PEP 621 metadata ([#2662](https://github.com/astral-sh/uv/pull/2662))
- Support `file://localhost/` schemes ([#2657](https://github.com/astral-sh/uv/pull/2657))
- Use normal resolver in `pip sync` ([#2696](https://github.com/astral-sh/uv/pull/2696))

### CLI

- Disallow `pyproject.toml` from `pip uninstall -r` ([#2663](https://github.com/astral-sh/uv/pull/2663))
- Unhide `--emit-index-url` and `--emit-find-links` ([#2691](https://github.com/astral-sh/uv/pull/2691))
- Use dense formatting for requirement version specifiers in diagnostics ([#2601](https://github.com/astral-sh/uv/pull/2601))

### Performance

- Add an in-memory cache for Git references ([#2682](https://github.com/astral-sh/uv/pull/2682))
- Do not force-recompile `.pyc` files ([#2642](https://github.com/astral-sh/uv/pull/2642))
- Read package metadata from `pyproject.toml` when it is statically defined ([#2676](https://github.com/astral-sh/uv/pull/2676))

### Bug fixes

- Don't error on multiple matching index URLs ([#2627](https://github.com/astral-sh/uv/pull/2627))
- Extract local versions from direct URL requirements ([#2624](https://github.com/astral-sh/uv/pull/2624))
- Respect `--no-index` with `--find-links` in `pip sync` ([#2692](https://github.com/astral-sh/uv/pull/2692))
- Use `Scripts` folder for virtualenv activation prompt ([#2690](https://github.com/astral-sh/uv/pull/2690))

## 0.1.24

### Breaking changes

- `uv pip uninstall` no longer supports specifying targets with the `-e` / `--editable` flag ([#2577](https://github.com/astral-sh/uv/pull/2577))

### Enhancements

- Add a garbage collection mechanism to the CLI ([#1217](https://github.com/astral-sh/uv/pull/1217))
- Add progress reporting for named requirement resolution ([#2605](https://github.com/astral-sh/uv/pull/2605))
- Add support for parsing unnamed URL requirements ([#2567](https://github.com/astral-sh/uv/pull/2567))
- Add support for unnamed local directory requirements ([#2571](https://github.com/astral-sh/uv/pull/2571))
- Enable PEP 517 builds for unnamed requirements ([#2600](https://github.com/astral-sh/uv/pull/2600))
- Enable install audits without resolving named requirements ([#2575](https://github.com/astral-sh/uv/pull/2575))
- Enable unnamed requirements for direct URLs ([#2569](https://github.com/astral-sh/uv/pull/2569))
- Respect HTTP client options when reading remote requirements files ([#2434](https://github.com/astral-sh/uv/pull/2434))
- Use PEP 517 build hooks to resolve unnamed requirements ([#2604](https://github.com/astral-sh/uv/pull/2604))
- Use c-string literals and update trampolines ([#2590](https://github.com/astral-sh/uv/pull/2590))
- Support unnamed requirements directly in `uv pip uninstall` ([#2577](https://github.com/astral-sh/uv/pull/2577))
- Add support for unnamed Git and HTTP requirements ([#2578](https://github.com/astral-sh/uv/pull/2578))
- Make self-update an opt-in Cargo feature ([#2606](https://github.com/astral-sh/uv/pull/2606))
- Update minimum rust version (cargo) to 1.76 ([#2618](https://github.com/astral-sh/uv/pull/2618))

### Bug fixes

- Fix self-updates on Windows ([#2598](https://github.com/astral-sh/uv/pull/2598))
- Fix authentication with usernames that contain `@` characters ([#2592](https://github.com/astral-sh/uv/pull/2592))
- Do not error when there are warnings on Python interpreter stderr ([#2599](https://github.com/astral-sh/uv/pull/2599))
- Prevent discovery of cache gitignore when building distributions ([#2615](https://github.com/astral-sh/uv/pull/2615))

### Rust API

- Make `InstallDist.direct_url` public ([#2584](https://github.com/astral-sh/uv/pull/2584))
- Make `AllowedYanks` public ([#2608](https://github.com/astral-sh/uv/pull/2608))

### Documentation

- Fix badge to current CI status ([#2612](https://github.com/astral-sh/uv/pull/2612))

## 0.1.23

### Enhancements

- Implement `--no-strip-extras` to preserve extras in compilation ([#2555](https://github.com/astral-sh/uv/pull/2555))
- Preserve hashes for pinned packages when compiling without `--upgrade` ([#2532](https://github.com/astral-sh/uv/pull/2532))
- Add a `uv self update` command ([#2228](https://github.com/astral-sh/uv/pull/2228))
- Use relative paths for user-facing messages ([#2559](https://github.com/astral-sh/uv/pull/2559))
- Add `CUSTOM_COMPILE_COMMAND` support to `uv pip compile` ([#2554](https://github.com/astral-sh/uv/pull/2554))
- Add SHA384 and SHA512 hash algorithms ([#2534](https://github.com/astral-sh/uv/pull/2534))
- Treat uninstallable packages as warnings, rather than errors ([#2557](https://github.com/astral-sh/uv/pull/2557))

### Bug fixes

- Allow `VIRTUAL_ENV` to take precedence over `CONDA_PREFIX` ([#2574](https://github.com/astral-sh/uv/pull/2574))
- Ensure mtime of site packages is updated during wheel installation ([#2545](https://github.com/astral-sh/uv/pull/2545))
- Re-test validity after every lenient parsing change ([#2550](https://github.com/astral-sh/uv/pull/2550))
- Run interpreter discovery under `-I` mode ([#2552](https://github.com/astral-sh/uv/pull/2552))
- Search in both `purelib` and `platlib` for site-packages population ([#2537](https://github.com/astral-sh/uv/pull/2537))
- Fix wheel builds and uploads for musl ARM ([#2518](https://github.com/astral-sh/uv/pull/2518))

### Documentation

- Add `--link-mode` defaults to CLI ([#2549](https://github.com/astral-sh/uv/pull/2549))
- Add an example workflow for compiling the current environment's packages ([#1968](https://github.com/astral-sh/uv/pull/1968))
- Add `uv pip check diagnostics` to `PIP_COMPATIBILITY.md` ([#2544](https://github.com/astral-sh/uv/pull/2544))

## 0.1.22

### Enhancements

- Add support for PyTorch-style local version semantics ([#2430](https://github.com/astral-sh/uv/pull/2430))
- Add support for Hatch's `{root:uri}` paths in editable installs ([#2492](https://github.com/astral-sh/uv/pull/2492))
- Implement `uv pip check` ([#2397](https://github.com/astral-sh/uv/pull/2397))
- Add pip-like linehaul information to user agent ([#2493](https://github.com/astral-sh/uv/pull/2493))
- Add additional ARM targets to release ([#2417](https://github.com/astral-sh/uv/pull/2417))

### Bug fixes

- Allow direct file path requirements to include fragments ([#2502](https://github.com/astral-sh/uv/pull/2502))
- Avoid panicking on cannot-be-a-base URLs ([#2461](https://github.com/astral-sh/uv/pull/2461))
- Drop `macosx_10_0` from compatible wheel tags on `aarch64` ([#2496](https://github.com/astral-sh/uv/pull/2496))
- Fix operating system detection on \*BSD ([#2505](https://github.com/astral-sh/uv/pull/2505))
- Fix priority of ABI tags ([#2489](https://github.com/astral-sh/uv/pull/2489))
- Fix priority of platform tags for manylinux ([#2483](https://github.com/astral-sh/uv/pull/2483))
- Make > operator exclude post and local releases ([#2471](https://github.com/astral-sh/uv/pull/2471))
- Re-add support for pyenv shims ([#2503](https://github.com/astral-sh/uv/pull/2503))
- Validate required package names against wheel package names ([#2516](https://github.com/astral-sh/uv/pull/2516))

## 0.1.21

### Enhancements

- Loosen `.dist-info` validation to accept arbitrary versions ([#2441](https://github.com/astral-sh/uv/pull/2441))

### Bug fixes

- Fix macOS architecture detection on i386 machines ([#2454](https://github.com/astral-sh/uv/pull/2454))

## 0.1.20

### Bug fixes

- Add in-URL credentials to store prior to creating requests ([#2446](https://github.com/astral-sh/uv/pull/2446))
- Error when direct URL requirements don't match `Requires-Python` ([#2196](https://github.com/astral-sh/uv/pull/2196))

## 0.1.19

### Configuration

- Add `UV_NATIVE_TLS` environment variable ([#2412](https://github.com/astral-sh/uv/pull/2412))
- Allow `SSL_CERT_FILE` without requiring `--native-tls` ([#2401](https://github.com/astral-sh/uv/pull/2401))
- Add support for retrieving credentials from `keyring` ([#2254](https://github.com/astral-sh/uv/pull/2254))

### Bug fixes

- Add backoff for transient Windows failures ([#2419](https://github.com/astral-sh/uv/pull/2419))
- Move architecture and operating system probing to Python ([#2381](https://github.com/astral-sh/uv/pull/2381))
- Respect `--native-tls` in `venv` ([#2433](https://github.com/astral-sh/uv/pull/2433))
- Treat non-existent site-packages as empty ([#2413](https://github.com/astral-sh/uv/pull/2413))

### Documentation

- Document HTTP authentication ([#2425](https://github.com/astral-sh/uv/pull/2425))

### Performance

- Improve performance of version range operations ([#2421](https://github.com/astral-sh/uv/pull/2421))

## 0.1.18

### Breaking changes

Users that rely on native root certificates (or the `SSL_CERT_FILE`) environment variable must now
pass the `--native-tls` command-line flag to enable this behavior.

- Enable TLS native root toggling at runtime ([#2362](https://github.com/astral-sh/uv/pull/2362))

### Enhancements

- Add `--dry-run` flag to `uv pip install` ([#1436](https://github.com/astral-sh/uv/pull/1436))
- Implement "Requires" field in `pip show` ([#2347](https://github.com/astral-sh/uv/pull/2347))
- Remove `wheel` from default PEP 517 backend ([#2341](https://github.com/astral-sh/uv/pull/2341))
- Add `UV_SYSTEM_PYTHON` environment variable as alias to `--system` ([#2354](https://github.com/astral-sh/uv/pull/2354))
- Add a `-vv` log level and make `-v` more readable ([#2301](https://github.com/astral-sh/uv/pull/2301))

### Bug fixes

- Expand environment variables prior to detecting scheme ([#2394](https://github.com/astral-sh/uv/pull/2394))
- Fix bug where `--no-binary :all:` prevented build of editable packages ([#2393](https://github.com/astral-sh/uv/pull/2393))
- Ignore inverse dependencies when building graph ([#2360](https://github.com/astral-sh/uv/pull/2360))
- Skip prefetching when `--no-deps` is specified ([#2373](https://github.com/astral-sh/uv/pull/2373))
- Trim injected `python_version` marker to (major, minor) ([#2395](https://github.com/astral-sh/uv/pull/2395))
- Wait for request stream to flush before returning resolution ([#2374](https://github.com/astral-sh/uv/pull/2374))
- Write relative paths for scripts in data directory ([#2348](https://github.com/astral-sh/uv/pull/2348))
- Add dedicated error message for direct filesystem paths in requirements ([#2369](https://github.com/astral-sh/uv/pull/2369))

## 0.1.17

### Enhancements

- Allow more-precise Git URLs to override less-precise Git URLs ([#2285](https://github.com/astral-sh/uv/pull/2285))
- Add support for Metadata 2.2 ([#2293](https://github.com/astral-sh/uv/pull/2293))
- Added ability to select bytecode invalidation mode of generated `.pyc` files ([#2297](https://github.com/astral-sh/uv/pull/2297))
- Add `Seek` fallback for zip files with data descriptors ([#2320](https://github.com/astral-sh/uv/pull/2320))

### Bug fixes

- Support reading UTF-16 requirements files ([#2283](https://github.com/astral-sh/uv/pull/2283))
- Trim rows in `pip list` ([#2298](https://github.com/astral-sh/uv/pull/2298))
- Avoid using setuptools shim of distutils ([#2305](https://github.com/astral-sh/uv/pull/2305))
- Communicate PEP 517 hook results via files ([#2314](https://github.com/astral-sh/uv/pull/2314))
- Increase default buffer size for wheel and source downloads ([#2319](https://github.com/astral-sh/uv/pull/2319))
- Add `Accept-Encoding: identity` to remaining stream paths ([#2321](https://github.com/astral-sh/uv/pull/2321))
- Avoid duplicating authorization header with netrc ([#2325](https://github.com/astral-sh/uv/pull/2325))
- Remove duplicate `INSTALLER` in `RECORD` ([#2336](https://github.com/astral-sh/uv/pull/2336))

### Documentation

- Add a custom suggestion to install wheel into the build environment ([#2307](https://github.com/astral-sh/uv/pull/2307))
- Document the environment variables that uv respects ([#2318](https://github.com/astral-sh/uv/pull/2318))

## 0.1.16

### Enhancements

- Add support for `--no-build-isolation` ([#2258](https://github.com/astral-sh/uv/pull/2258))
- Add support for `--break-system-packages` ([#2249](https://github.com/astral-sh/uv/pull/2249))
- Add support for `.netrc` authentication ([#2241](https://github.com/astral-sh/uv/pull/2241))
- Add support for `--format=freeze` and `--format=json` in `uv pip list` ([#1998](https://github.com/astral-sh/uv/pull/1998))
- Add support for remote `https://` requirements files (#1332) ([#2081](https://github.com/astral-sh/uv/pull/2081))
- Implement `uv pip show` ([#2115](https://github.com/astral-sh/uv/pull/2115))
- Allow `UV_PRERELEASE` to be set via environment variable ([#2240](https://github.com/astral-sh/uv/pull/2240))
- Include exit code for build failures ([#2108](https://github.com/astral-sh/uv/pull/2108))
- Query interpreter to determine correct `virtualenv` paths, enabling `uv venv` with PyPy and others ([#2188](https://github.com/astral-sh/uv/pull/2188))
- Respect non-`sysconfig`-based system Pythons, enabling `--system` installs on Debian and others ([#2193](https://github.com/astral-sh/uv/pull/2193))

### Bug fixes

- Fallback to fresh request on non-validating 304 ([#2218](https://github.com/astral-sh/uv/pull/2218))
- Add `.stdout()` and `.stderr()` outputs to `Printer` ([#2227](https://github.com/astral-sh/uv/pull/2227))
- Close `RECORD` after reading entries during uninstall ([#2259](https://github.com/astral-sh/uv/pull/2259))
- Fix Conda Python detection on Windows ([#2279](https://github.com/astral-sh/uv/pull/2279))
- Fix parsing requirement where a variable follows an operator without a space ([#2273](https://github.com/astral-sh/uv/pull/2273))
- Prefer more recent minor versions in wheel tags ([#2263](https://github.com/astral-sh/uv/pull/2263))
- Retry on Python interpreter launch failures during `--compile` ([#2278](https://github.com/astral-sh/uv/pull/2278))
- Show appropriate activation command based on shell detection ([#2221](https://github.com/astral-sh/uv/pull/2221))
- Escape Windows paths with spaces in `venv` activation command ([#2223](https://github.com/astral-sh/uv/pull/2223))
- Add specialized activation message for `cmd.exe` ([#2226](https://github.com/astral-sh/uv/pull/2226))
- Cache wheel metadata in no-PEP 658 fallback ([#2255](https://github.com/astral-sh/uv/pull/2255))
- Use reparse points to detect Windows installer shims ([#2284](https://github.com/astral-sh/uv/pull/2284))

### Documentation

- Add `PIP_COMPATIBILITY.md` to document known deviations from `pip` ([#2244](https://github.com/astral-sh/uv/pull/2244))

## 0.1.15

### Enhancements

- Add a `--compile` option to `install` to enable bytecode compilation ([#2086](https://github.com/astral-sh/uv/pull/2086))
- Expose the `--exclude-newer` flag to limit candidate packages based on date ([#2166](https://github.com/astral-sh/uv/pull/2166))
- Add `uv` version to user agent ([#2136](https://github.com/astral-sh/uv/pull/2136))

### Bug fixes

- Set `.metadata` suffix on URL path ([#2123](https://github.com/astral-sh/uv/pull/2123))
- Fallback to non-range requests when HEAD returns 404 ([#2186](https://github.com/astral-sh/uv/pull/2186))
- Allow direct URLs in optional dependencies in editables ([#2206](https://github.com/astral-sh/uv/pull/2206))
- Allow empty values in WHEEL files ([#2170](https://github.com/astral-sh/uv/pull/2170))
- Avoid Windows Store shims in `--python python3`-like invocations ([#2212](https://github.com/astral-sh/uv/pull/2212))
- Expand Windows shim detection to include `python3.12.exe` ([#2209](https://github.com/astral-sh/uv/pull/2209))
- HTML-decode URLs in HTML indexes ([#2215](https://github.com/astral-sh/uv/pull/2215))
- Make direct dependency detection respect markers ([#2207](https://github.com/astral-sh/uv/pull/2207))
- Respect `py --list-paths` fallback in `--python python3` invocations on Windows ([#2214](https://github.com/astral-sh/uv/pull/2214))
- Respect local freshness when auditing installed environment ([#2169](https://github.com/astral-sh/uv/pull/2169))
- Respect markers on URL dependencies in editables ([#2176](https://github.com/astral-sh/uv/pull/2176))
- Respect nested editable requirements in parser ([#2204](https://github.com/astral-sh/uv/pull/2204))
- Run Windows against Python 3.13 ([#2171](https://github.com/astral-sh/uv/pull/2171))
- Error when editables don't match `Requires-Python` ([#2194](https://github.com/astral-sh/uv/pull/2194))

## 0.1.14

### Enhancements

- Add support for `--system-site-packages` in `uv venv` ([#2101](https://github.com/astral-sh/uv/pull/2101))
- Add support for Python installed from Windows Store ([#2122](https://github.com/astral-sh/uv/pull/2122))
- Expand environment variables in `-r` and `-c` subfile paths ([#2143](https://github.com/astral-sh/uv/pull/2143))
- Treat empty index URL strings as null instead of erroring ([#2137](https://github.com/astral-sh/uv/pull/2137))
- Use space as delimiter for `UV_EXTRA_INDEX_URL` ([#2140](https://github.com/astral-sh/uv/pull/2140))
- Report line and column numbers in `requirements.txt` parser errors ([#2100](https://github.com/astral-sh/uv/pull/2100))
- Improve error messages when `uv` is offline ([#2110](https://github.com/astral-sh/uv/pull/2110))

### Bug fixes

- Future-proof the `pip` entrypoints special-case ([#1982](https://github.com/astral-sh/uv/pull/1982))
- Allow empty extras in `pep508-rs` and add more corner case to tests ([#2128](https://github.com/astral-sh/uv/pull/2128))
- Adjust base Python lookup logic for Windows to respect Windows Store ([#2121](https://github.com/astral-sh/uv/pull/2121))
- Consider editable dependencies to be 'direct' for `--resolution` ([#2114](https://github.com/astral-sh/uv/pull/2114))
- Preserve environment variables in resolved Git dependencies ([#2125](https://github.com/astral-sh/uv/pull/2125))
- Use `prefix` instead of `base_prefix` for environment root ([#2117](https://github.com/astral-sh/uv/pull/2117))
- Wrap unsafe script shebangs in `/bin/sh` ([#2097](https://github.com/astral-sh/uv/pull/2097))
- Make WHEEL parsing error line numbers one indexed ([#2151](https://github.com/astral-sh/uv/pull/2151))
- Determine `site-packages` path based on implementation name ([#2094](https://github.com/astral-sh/uv/pull/2094))

### Documentation

- Add caveats on `--system` support to the README ([#2131](https://github.com/astral-sh/uv/pull/2131))
- Add instructions for `SSL_CERT_FILE` env var ([#2124](https://github.com/astral-sh/uv/pull/2124))

## 0.1.13

### Bug fixes

- Prioritize `PATH` over `py --list-paths` in Windows selection ([#2057](https://github.com/astral-sh/uv/pull/2057)). This fixes an issue in which the `--system` flag would not work correctly on Windows in GitHub Actions.
- Avoid canonicalizing user-provided interpreters ([#2072](https://github.com/astral-sh/uv/pull/2072)). This fixes an issue in which the `--python` flag would not work correctly with pyenv and other interpreters.
- Allow pre-releases for requirements in constraints files ([#2069](https://github.com/astral-sh/uv/pull/2069))
- Avoid truncating EXTERNALLY-MANAGED error message ([#2073](https://github.com/astral-sh/uv/pull/2073))
- Extend activation highlighting to entire `venv` command ([#2070](https://github.com/astral-sh/uv/pull/2070))
- Reverse the order of `--index-url` and `--extra-index-url` priority ([#2083](https://github.com/astral-sh/uv/pull/2083))
- Avoid assuming `RECORD` file is in `platlib` ([#2091](https://github.com/astral-sh/uv/pull/2091))

## 0.1.12

### CLI

- Add a `--python` flag to allow installation into arbitrary Python interpreters ([#2000](https://github.com/astral-sh/uv/pull/2000))
- Add a `--system` flag for opt-in non-virtualenv installs ([#2046](https://github.com/astral-sh/uv/pull/2046))

### Enhancements

- Add a `--pre` alias for `--prerelease=allow` ([#2049](https://github.com/astral-sh/uv/pull/2049))
- Enable `freeze` and `list` to introspect non-virtualenv Pythons ([#2033](https://github.com/astral-sh/uv/pull/2033))
- Support environment variables in index URLs in requirements files ([#2036](https://github.com/astral-sh/uv/pull/2036))
- Add `--exclude-editable` and `--exclude` args to `uv pip list` ([#1985](https://github.com/astral-sh/uv/pull/1985))
- Always remove color codes from output file ([#2018](https://github.com/astral-sh/uv/pull/2018))
- Support recursive extras in direct `pyproject.toml` files ([#1990](https://github.com/astral-sh/uv/pull/1990))
- Un-cache editable requirements with dynamic metadata ([#2029](https://github.com/astral-sh/uv/pull/2029))
- Use a non-local lockfile for locking system interpreters ([#2045](https://github.com/astral-sh/uv/pull/2045))
- Surface the `EXTERNALLY-MANAGED` message to users ([#2032](https://github.com/astral-sh/uv/pull/2032))

## 0.1.11

### Enhancements

- Add support for pip-compile's `--unsafe-package` flag ([#1889](https://github.com/astral-sh/uv/pull/1889))
- Improve interpreter discovery logging ([#1909](https://github.com/astral-sh/uv/pull/1909))
- Implement `uv pip list` ([#1662](https://github.com/astral-sh/uv/pull/1662))
- Allow round-trip via `freeze` command ([#1936](https://github.com/astral-sh/uv/pull/1936))
- Don't write pip compile output to stdout with `-q` ([#1962](https://github.com/astral-sh/uv/pull/1962))
- Add long-form version output ([#1930](https://github.com/astral-sh/uv/pull/1930))

### Compatibility

- Accept single string for `backend-path` ([#1969](https://github.com/astral-sh/uv/pull/1969))
- Add compatibility for deprecated `python_implementation` marker ([#1933](https://github.com/astral-sh/uv/pull/1933))
- Generate versioned `pip` launchers ([#1918](https://github.com/astral-sh/uv/pull/1918))

### Bug fixes

- Avoid erroring for source distributions with symlinks in archive ([#1944](https://github.com/astral-sh/uv/pull/1944))
- Expand scope of archive timestamping ([#1960](https://github.com/astral-sh/uv/pull/1960))
- Gracefully handle virtual environments with conflicting packages ([#1893](https://github.com/astral-sh/uv/pull/1893))
- Invalidate dependencies when editables are updated ([#1955](https://github.com/astral-sh/uv/pull/1955))
- Make < exclusive for non-pre-release markers ([#1878](https://github.com/astral-sh/uv/pull/1878))
- Properly apply constraints in venv audit ([#1956](https://github.com/astral-sh/uv/pull/1956))
- Re-sync editables on-change ([#1959](https://github.com/astral-sh/uv/pull/1959))
- Remove current directory from PATH in PEP 517 hooks ([#1975](https://github.com/astral-sh/uv/pull/1975))
- Remove `--upgrade` and `--quiet` flags from generated output files ([#1873](https://github.com/astral-sh/uv/pull/1873))
- Use full python version in `pyvenv.cfg` ([#1979](https://github.com/astral-sh/uv/pull/1979))

### Performance

- fix `uv pip install` handling of gzip'd response and PEP 691 ([#1978](https://github.com/astral-sh/uv/pull/1978))
- Remove `spawn_blocking` from version map ([#1966](https://github.com/astral-sh/uv/pull/1966))

### Documentation

- Clarify `lowest` vs. `lowest-direct` resolution strategies ([#1954](https://github.com/astral-sh/uv/pull/1954))
- Improve error message for network timeouts ([#1961](https://github.com/astral-sh/uv/pull/1961))

## 0.1.10

### Enhancements

- Omit `--find-links` from annotation header unless requested ([#1898](https://github.com/astral-sh/uv/pull/1898))
- Write to stdout when `--output-file` is present ([#1892](https://github.com/astral-sh/uv/pull/1892))

### Bug fixes

- Retain authentication when making range requests ([#1902](https://github.com/astral-sh/uv/pull/1902))
- Fix uv-created venv detection ([#1908](https://github.com/astral-sh/uv/pull/1908))
- Fix Windows `py` failure from spurious stderr ([#1885](https://github.com/astral-sh/uv/pull/1885))
- Ignore Python 2 installations when querying for interpreters ([#1905](https://github.com/astral-sh/uv/pull/1905))

## 0.1.9

### Enhancements

- Add support for `config_settings` in PEP 517 hooks ([#1833](https://github.com/astral-sh/uv/pull/1833))
- feat: allow passing extra config k,v pairs for pyvenv.cfg when creating a venv ([#1852](https://github.com/astral-sh/uv/pull/1852))

### Bug fixes

- Ensure authentication is passed from the index url to distribution files ([#1886](https://github.com/astral-sh/uv/pull/1886))
- Use `rustls-tls-native-roots` in `uv` crate ([#1888](https://github.com/astral-sh/uv/pull/1888))
- pep440: fix version ordering ([#1883](https://github.com/astral-sh/uv/pull/1883))
- Hide index URLs from header if not emitted ([#1835](https://github.com/astral-sh/uv/pull/1835))

### Documentation

- Add changelog ([#1881](https://github.com/astral-sh/uv/pull/1881))

## 0.1.8

### Bug fixes

- Allow duplicate URLs that resolve to the same canonical URL ([#1877](https://github.com/astral-sh/uv/pull/1877))
- Retain authentication attached to URLs when making requests to the same host ([#1874](https://github.com/astral-sh/uv/pull/1874))
- Win Trampoline: Use Python executable path encoded in binary ([#1803](https://github.com/astral-sh/uv/pull/1803))
- Expose types to implement custom `ResolverProvider` ([#1862](https://github.com/astral-sh/uv/pull/1862))
- Search `PATH` when `python` can't be found with `py` ([#1711](https://github.com/astral-sh/uv/pull/1711))
- Avoid displaying "root" package when formatting terms ([#1871](https://github.com/astral-sh/uv/pull/1871))

### Documentation

- Use more universal windows install instructions ([#1811](https://github.com/astral-sh/uv/pull/1811))

### Rust API

- Expose types to implement custom ResolverProvider ([#1862](https://github.com/astral-sh/uv/pull/1862))

## 0.1.7

### Enhancements

- Stream zip archive when fetching non-range-request metadata ([#1792](https://github.com/astral-sh/uv/pull/1792))
- Support setting request timeout with `UV_HTTP_TIMEOUT` and `HTTP_TIMEOUT` ([#1780](https://github.com/astral-sh/uv/pull/1780))
- Improve error message when git ref cannot be fetched ([#1826](https://github.com/astral-sh/uv/pull/1826))

### Configuration

- Implement `--annotation-style` parameter for `uv pip compile` ([#1679](https://github.com/astral-sh/uv/pull/1679))

### Bug fixes

- Add fixup for `prefect<1.0.0` ([#1825](https://github.com/astral-sh/uv/pull/1825))
- Add support for `>dev` specifier ([#1776](https://github.com/astral-sh/uv/pull/1776))
- Avoid enforcing URL correctness for installed distributions ([#1793](https://github.com/astral-sh/uv/pull/1793))
- Don't expect pinned packages for editables with non-existent extras ([#1847](https://github.com/astral-sh/uv/pull/1847))
- Linker copies files as a fallback when ref-linking fails ([#1773](https://github.com/astral-sh/uv/pull/1773))
- Move conflicting dependencies into PubGrub ([#1796](https://github.com/astral-sh/uv/pull/1796))
- Normalize `VIRTUAL_ENV` path in activation scripts ([#1817](https://github.com/astral-sh/uv/pull/1817))
- Preserve executable bit when untarring archives ([#1790](https://github.com/astral-sh/uv/pull/1790))
- Retain passwords in Git URLs ([#1717](https://github.com/astral-sh/uv/pull/1717))
- Sort output when installing seed packages ([#1822](https://github.com/astral-sh/uv/pull/1822))
- Treat ARM wheels as higher-priority than universal ([#1843](https://github.com/astral-sh/uv/pull/1843))
- Use `git` command to fetch repositories instead of `libgit2` for robust SSH support ([#1781](https://github.com/astral-sh/uv/pull/1781))
- Use redirected URL as base for relative paths ([#1816](https://github.com/astral-sh/uv/pull/1816))
- Use the right marker for the `implementation` field of `pyvenv.cfg` ([#1785](https://github.com/astral-sh/uv/pull/1785))
- Wait for distribution metadata with `--no-deps` ([#1812](https://github.com/astral-sh/uv/pull/1812))
- platform-host: check /bin/sh, then /bin/dash and then /bin/ls ([#1818](https://github.com/astral-sh/uv/pull/1818))
- Ensure that builds within the cache aren't considered Git repositories ([#1782](https://github.com/astral-sh/uv/pull/1782))
- Strip trailing `+` from version number of local Python builds ([#1771](https://github.com/astral-sh/uv/pull/1771))

### Documentation

- Add docs for git authentication ([#1844](https://github.com/astral-sh/uv/pull/1844))
- Update venv activation for windows ([#1836](https://github.com/astral-sh/uv/pull/1836))
- Update README.md to include extras example ([#1806](https://github.com/astral-sh/uv/pull/1806))

## 0.1.6

### Enhancements

- Expose find_uv_bin and declare typing support ([#1728](https://github.com/astral-sh/uv/pull/1728))
- Implement `uv cache dir` ([#1734](https://github.com/astral-sh/uv/pull/1734))
- Support `venv --prompt` ([#1570](https://github.com/astral-sh/uv/pull/1570))
- Print activation instructions for a venv after one has been created ([#1580](https://github.com/astral-sh/uv/pull/1580))

### CLI

- Add shell completions generation ([#1675](https://github.com/astral-sh/uv/pull/1675))
- Move `uv clean` to `uv cache clean` ([#1733](https://github.com/astral-sh/uv/pull/1733))
- Allow `-f` alias for `--find-links` ([#1735](https://github.com/astral-sh/uv/pull/1735))

### Configuration

- Control pip timeout duration via environment variable ([#1694](https://github.com/astral-sh/uv/pull/1694))

### Bug fixes

- Add support for absolute paths on Windows ([#1725](https://github.com/astral-sh/uv/pull/1725))
- Don't preserve timestamp in streaming unzip ([#1749](https://github.com/astral-sh/uv/pull/1749))
- Ensure extras trigger an install ([#1727](https://github.com/astral-sh/uv/pull/1727))
- Only preserve the executable bit ([#1743](https://github.com/astral-sh/uv/pull/1743))
- Preserve trailing slash for `--find-links` URLs ([#1720](https://github.com/astral-sh/uv/pull/1720))
- Respect `--index-url` provided via requirements.txt ([#1719](https://github.com/astral-sh/uv/pull/1719))
- Set index URLs for seeding venv ([#1755](https://github.com/astral-sh/uv/pull/1755))
- Support dotted function paths for script entrypoints ([#1622](https://github.com/astral-sh/uv/pull/1622))
- Support recursive extras for URL dependencies ([#1729](https://github.com/astral-sh/uv/pull/1729))
- Better error message for missing space before semicolon in requirements ([#1746](https://github.com/astral-sh/uv/pull/1746))
- Add warning when dependencies are empty with Poetry metadata ([#1650](https://github.com/astral-sh/uv/pull/1650))
- Ignore invalid extras from PyPI ([#1731](https://github.com/astral-sh/uv/pull/1731))
- Improve Poetry warning ([#1730](https://github.com/astral-sh/uv/pull/1730))
- Remove uv version from uv pip compile header ([#1716](https://github.com/astral-sh/uv/pull/1716))
- Fix handling of range requests on servers that return "Method not allowed" ([#1713](https://github.com/astral-sh/uv/pull/1713))
- re-introduce cache healing when we see an invalid cache entry ([#1707](https://github.com/astral-sh/uv/pull/1707))

### Documentation

- Clarify Windows install command in README.md ([#1751](https://github.com/astral-sh/uv/pull/1751))
- Add instructions for installing on Arch Linux ([#1765](https://github.com/astral-sh/uv/pull/1765))

### Rust API

- Allow passing in a custom reqwest Client ([#1745](https://github.com/astral-sh/uv/pull/1745))

## 0.1.5

### Enhancements

- Add `CACHEDIR.TAG` to uv-created virtualenvs ([#1653](https://github.com/astral-sh/uv/pull/1653))

### Bug fixes

- Build source distributions in the cache directory instead of the global temporary directory ([#1628](https://github.com/astral-sh/uv/pull/1628))
- Do not remove uv itself on pip sync ([#1649](https://github.com/astral-sh/uv/pull/1649))
- Ensure we retain existing environment variables during `python -m uv` ([#1667](https://github.com/astral-sh/uv/pull/1667))
- Add yank warnings at end of messages ([#1669](https://github.com/astral-sh/uv/pull/1669))

### Documentation

- Add brew to readme ([#1629](https://github.com/astral-sh/uv/pull/1629))
- Document RUST_LOG=trace for additional logging verbosity ([#1670](https://github.com/astral-sh/uv/pull/1670))
- Document local testing instructions ([#1672](https://github.com/astral-sh/uv/pull/1672))
- Minimal markdown nits ([#1664](https://github.com/astral-sh/uv/pull/1664))
- Use `--override` rather than `-o` to specify overrides in README.md ([#1668](https://github.com/astral-sh/uv/pull/1668))
- Remove setuptools & wheel from seed packages on Python 3.12+ (#1602) ([#1613](https://github.com/astral-sh/uv/pull/1613))

## 0.1.4

### Enhancements

- Add CMD support ([#1523](https://github.com/astral-sh/uv/pull/1523))
- Improve tracing when encountering invalid `requires-python` values ([#1568](https://github.com/astral-sh/uv/pull/1568))

### Bug fixes

- Add graceful fallback for Artifactory indexes ([#1574](https://github.com/astral-sh/uv/pull/1574))
- Allow URL requirements in editable installs ([#1614](https://github.com/astral-sh/uv/pull/1614))
- Allow repeated dependencies when installing ([#1558](https://github.com/astral-sh/uv/pull/1558))
- Always run `get_requires_for_build_wheel` ([#1590](https://github.com/astral-sh/uv/pull/1590))
- Avoid propagating top-level options to sub-resolutions ([#1607](https://github.com/astral-sh/uv/pull/1607))
- Consistent use of `BIN_NAME` in activation scripts ([#1577](https://github.com/astral-sh/uv/pull/1577))
- Enforce URL constraints for non-URL dependencies ([#1565](https://github.com/astral-sh/uv/pull/1565))
- Allow non-nested archives for `hexdump` and others ([#1564](https://github.com/astral-sh/uv/pull/1564))
- Avoid using `white` coloring in terminal output ([#1576](https://github.com/astral-sh/uv/pull/1576))
- Bump simple metadata cache version ([#1617](https://github.com/astral-sh/uv/pull/1617))
- Better error messages on expect failures in resolver ([#1583](https://github.com/astral-sh/uv/pull/1583))

### Documentation

- Add license to activator scripts ([#1610](https://github.com/astral-sh/uv/pull/1610))

## 0.1.3

### Enhancements

- Add support for `UV_EXTRA_INDEX_URL` ([#1515](https://github.com/astral-sh/uv/pull/1515))
- Use the system trust store for HTTPS requests ([#1512](https://github.com/astral-sh/uv/pull/1512))
- Automatically detect virtual environments when used via `python -m uv` ([#1504](https://github.com/astral-sh/uv/pull/1504))
- Add warning for empty requirements files ([#1519](https://github.com/astral-sh/uv/pull/1519))
- Support MD5 hashes ([#1556](https://github.com/astral-sh/uv/pull/1556))

### Bug fixes

- Add support for extras in editable requirements ([#1531](https://github.com/astral-sh/uv/pull/1531))
- Apply percent-decoding to file-based URLs ([#1541](https://github.com/astral-sh/uv/pull/1541))
- Apply percent-decoding to filepaths in HTML find-links ([#1544](https://github.com/astral-sh/uv/pull/1544))
- Avoid attempting rename in copy fallback path ([#1546](https://github.com/astral-sh/uv/pull/1546))
- Fix list rendering in `venv --help` output ([#1459](https://github.com/astral-sh/uv/pull/1459))
- Fix trailing commas on `Requires-Python` in HTML indexes ([#1507](https://github.com/astral-sh/uv/pull/1507))
- Read from `/bin/sh` if `/bin/ls` cannot be found when determining libc path ([#1433](https://github.com/astral-sh/uv/pull/1433))
- Remove URL encoding when determining file name ([#1555](https://github.com/astral-sh/uv/pull/1555))
- Support recursive extras ([#1435](https://github.com/astral-sh/uv/pull/1435))
- Use comparable representation for `PackageId` ([#1543](https://github.com/astral-sh/uv/pull/1543))
- fix OS detection for Alpine Linux ([#1545](https://github.com/astral-sh/uv/pull/1545))
- only parse /bin/sh (not /bin/ls) ([#1493](https://github.com/astral-sh/uv/pull/1493))
- pypi-types: fix lenient requirement parsing ([#1529](https://github.com/astral-sh/uv/pull/1529))
- Loosen package script regexp to match spec ([#1482](https://github.com/astral-sh/uv/pull/1482))
- Use string display instead of debug for url parse trace ([#1498](https://github.com/astral-sh/uv/pull/1498))

### Documentation

- Provide example of file based package install. ([#1424](https://github.com/astral-sh/uv/pull/1424))
- Adjust link ([#1434](https://github.com/astral-sh/uv/pull/1434))
- Add troubleshooting section to benchmarks guide ([#1485](https://github.com/astral-sh/uv/pull/1485))
- infra: source github templates ([#1425](https://github.com/astral-sh/uv/pull/1425))

## 0.1.2

### Enhancements

- Add `--upgrade` support to `pip install` ([#1379](https://github.com/astral-sh/uv/pull/1379))
- Add `-U`/`-P` short flags for `--upgrade`/`--upgrade-package` ([#1394](https://github.com/astral-sh/uv/pull/1394))
- Add `UV_NO_CACHE` environment variable ([#1383](https://github.com/astral-sh/uv/pull/1383))
- uv-cache: Add hidden alias for --no-cache-dir ([#1380](https://github.com/astral-sh/uv/pull/1380))

### Bug fixes

- Add fix-up for invalid star comparison with major-only version ([#1410](https://github.com/astral-sh/uv/pull/1410))
- Add fix-up for trailing comma with trailing space ([#1409](https://github.com/astral-sh/uv/pull/1409))
- Allow empty fragments in HTML parser ([#1443](https://github.com/astral-sh/uv/pull/1443))
- Fix search for `python.exe` on Windows ([#1381](https://github.com/astral-sh/uv/pull/1381))
- Ignore invalid extra named `.none` ([#1428](https://github.com/astral-sh/uv/pull/1428))
- Parse `-r` and `-c` entries as relative to containing file ([#1421](https://github.com/astral-sh/uv/pull/1421))
- Avoid import contextlib in `_virtualenv` ([#1406](https://github.com/astral-sh/uv/pull/1406))
- Decode HTML escapes when extracting SHA ([#1440](https://github.com/astral-sh/uv/pull/1440))
- Fix broken URLs parsed from relative paths in registries ([#1413](https://github.com/astral-sh/uv/pull/1413))
- Improve error message for invalid sdist archives ([#1389](https://github.com/astral-sh/uv/pull/1389))

### Documentation

- Re-add license badge to the README ([#1333](https://github.com/astral-sh/uv/pull/1333))
- Replace "novel" in README ([#1365](https://github.com/astral-sh/uv/pull/1365))
- Tweak some grammar in the README ([#1387](https://github.com/astral-sh/uv/pull/1387))
- Update README.md to include venv activate ([#1411](https://github.com/astral-sh/uv/pull/1411))
- Update wording and add `alt` tag ([#1423](https://github.com/astral-sh/uv/pull/1423))

## 0.1.1

### Bug fixes

- Fix bug where `python3` is not found in the global path ([#1351](https://github.com/astral-sh/uv/pull/1351))

### Documentation

- Fix diagram alignment ([#1354](https://github.com/astral-sh/uv/pull/1354))
- Grammar nit ([#1345](https://github.com/astral-sh/uv/pull/1345))
<!-- prettier-ignore-end -->


