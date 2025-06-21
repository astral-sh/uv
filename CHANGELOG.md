# Changelog

<!-- prettier-ignore-start -->


## 0.8.0

## 0.7.13


### Python

- Add Python 3.14.0b2
- Add Python 3.13.5
- Fix stability of `uuid.getnode` on 3.13

See the
[`python-build-standalone` release notes](https://github.com/astral-sh/python-build-standalone/releases/tag/20250612)
for more details.

### Enhancements

- Download versions in `uv python pin` if not found ([#13946](https://github.com/astral-sh/uv/pull/13946))
- Use TTY detection to determine if SIGINT forwarding is enabled ([#13925](https://github.com/astral-sh/uv/pull/13925))
- Avoid fetching an exact, cached Git commit, even if it isn't locked ([#13748](https://github.com/astral-sh/uv/pull/13748))
- Add `zstd` and `deflate` to `Accept-Encoding` ([#13982](https://github.com/astral-sh/uv/pull/13982))
- Build binaries for riscv64  ([#12688](https://github.com/astral-sh/uv/pull/12688))

### Bug fixes

- Check if relative URL is valid directory before treating as index ([#13917](https://github.com/astral-sh/uv/pull/13917))
- Ignore Python discovery errors during `uv python pin` ([#13944](https://github.com/astral-sh/uv/pull/13944))
- Do not allow `uv add --group ... --script` ([#13997](https://github.com/astral-sh/uv/pull/13997))

### Preview changes

- Build backend: Support namespace packages ([#13833](https://github.com/astral-sh/uv/pull/13833))

### Documentation

- Add 3.14 to the supported platform reference ([#13990](https://github.com/astral-sh/uv/pull/13990))
- Add an `llms.txt` to uv ([#13929](https://github.com/astral-sh/uv/pull/13929))
- Add supported macOS version to the platform reference ([#13993](https://github.com/astral-sh/uv/pull/13993))
- Update platform support reference to include Python implementation list ([#13991](https://github.com/astral-sh/uv/pull/13991))
- Update pytorch.md ([#13899](https://github.com/astral-sh/uv/pull/13899))
- Update the CLI help and reference to include references to the Python bin directory ([#13978](https://github.com/astral-sh/uv/pull/13978))

## 0.7.12

### Enhancements

- Add `uv python pin --rm` to remove `.python-version` pins ([#13860](https://github.com/astral-sh/uv/pull/13860))
- Don't hint at versions removed by `excluded-newer` ([#13884](https://github.com/astral-sh/uv/pull/13884))
- Add hint to use `tool.uv.environments` on resolution error ([#13455](https://github.com/astral-sh/uv/pull/13455))
- Add hint to use `tool.uv.required-environments` on resolution error ([#13575](https://github.com/astral-sh/uv/pull/13575))
- Improve `python pin` error messages ([#13862](https://github.com/astral-sh/uv/pull/13862))

### Bug fixes

- Lock environments during `uv sync`, `uv add` and `uv remove` to prevent race conditions ([#13869](https://github.com/astral-sh/uv/pull/13869))
- Add `--no-editable` to `uv export` for `pylock.toml` ([#13852](https://github.com/astral-sh/uv/pull/13852))

### Documentation

- List `.gitignore` in project init files ([#13855](https://github.com/astral-sh/uv/pull/13855))
- Move the pip interface documentation into the concepts section ([#13841](https://github.com/astral-sh/uv/pull/13841))
- Remove the configuration section in favor of concepts / reference ([#13842](https://github.com/astral-sh/uv/pull/13842))
- Update Git and GitHub Actions docs to mention `gh auth login` ([#13850](https://github.com/astral-sh/uv/pull/13850))

### Preview

- Fix directory glob traversal fallback preventing exclusion of all files ([#13882](https://github.com/astral-sh/uv/pull/13882))

## 0.7.11

### Python

- Add Python 3.14.0b1
- Add Python 3.13.4
- Add Python 3.12.11
- Add Python 3.11.13
- Add Python 3.10.18
- Add Python 3.9.23

### Enhancements

- Add Pyodide support ([#12731](https://github.com/astral-sh/uv/pull/12731))
- Better error message for version specifier with missing operator ([#13803](https://github.com/astral-sh/uv/pull/13803))

### Bug fixes

- Downgrade `reqwest` and `hyper-util` to resolve connection reset errors over IPv6 ([#13835](https://github.com/astral-sh/uv/pull/13835))
- Prefer `uv`'s binary's version when checking if it's up to date ([#13840](https://github.com/astral-sh/uv/pull/13840))

### Documentation

- Use "terminal driver" instead of "shell" in `SIGINT` docs ([#13787](https://github.com/astral-sh/uv/pull/13787))

## 0.7.10

### Enhancements

- Add `--show-extras` to `uv tool list` ([#13783](https://github.com/astral-sh/uv/pull/13783))
- Add dynamically generated sysconfig replacement mappings ([#13441](https://github.com/astral-sh/uv/pull/13441))
- Add data locations to install wheel logs ([#13797](https://github.com/astral-sh/uv/pull/13797))

### Bug fixes

- Avoid redaction of placeholder `git` username when using SSH authentication ([#13799](https://github.com/astral-sh/uv/pull/13799))
- Propagate credentials to files on devpi indexes ending in `/+simple` ([#13743](https://github.com/astral-sh/uv/pull/13743))
- Restore retention of credentials for direct URLs in `uv export` ([#13809](https://github.com/astral-sh/uv/pull/13809))

## 0.7.9

### Python

The changes reverted in [0.7.8](#078) have been restored.

See the
[`python-build-standalone` release notes](https://github.com/astral-sh/python-build-standalone/releases/tag/20250529)
for more details.

### Enhancements

- Improve obfuscation of credentials in URLs ([#13560](https://github.com/astral-sh/uv/pull/13560))
- Allow running non-default Python implementations via `uvx` ([#13583](https://github.com/astral-sh/uv/pull/13583))
- Add `uvw` as alias for `uv` without console window on Windows ([#11786](https://github.com/astral-sh/uv/pull/11786))
- Allow discovery of x86-64 managed Python builds on macOS ([#13722](https://github.com/astral-sh/uv/pull/13722))
- Differentiate between implicit vs explicit architecture requests ([#13723](https://github.com/astral-sh/uv/pull/13723))
- Implement ordering for Python architectures to prefer native installations ([#13709](https://github.com/astral-sh/uv/pull/13709))
- Only show the first match per platform (and architecture) by default in `uv python list`  ([#13721](https://github.com/astral-sh/uv/pull/13721))
- Write the path of the parent environment to an `extends-environment` key in the `pyvenv.cfg` file of an ephemeral environment ([#13598](https://github.com/astral-sh/uv/pull/13598))
- Improve the error message when libc cannot be found, e.g., when using the distroless containers ([#13549](https://github.com/astral-sh/uv/pull/13549))

### Performance

- Avoid rendering info log level ([#13642](https://github.com/astral-sh/uv/pull/13642))
- Improve performance of `uv-python` crate's manylinux submodule ([#11131](https://github.com/astral-sh/uv/pull/11131))
- Optimize `Version` display ([#13643](https://github.com/astral-sh/uv/pull/13643))
- Reduce number of reference-checks for `uv cache clean` ([#13669](https://github.com/astral-sh/uv/pull/13669))

### Bug fixes

- Avoid reinstalling dependency group members with `--all-packages` ([#13678](https://github.com/astral-sh/uv/pull/13678))
- Don't fail direct URL hash checking with dependency metadata ([#13736](https://github.com/astral-sh/uv/pull/13736))
- Exit early on `self update` if global `--offline` is set ([#13663](https://github.com/astral-sh/uv/pull/13663))
- Fix cases where the uv lock is incorrectly marked as out of date ([#13635](https://github.com/astral-sh/uv/pull/13635))
- Include pre-release versions in `uv python install --reinstall` ([#13645](https://github.com/astral-sh/uv/pull/13645))
- Set `LC_ALL=C` for git when checking git worktree ([#13637](https://github.com/astral-sh/uv/pull/13637))
- Avoid rejecting Windows paths for remote Python download JSON targets ([#13625](https://github.com/astral-sh/uv/pull/13625))

### Preview

- Add `uv add --bounds` to configure version constraints ([#12946](https://github.com/astral-sh/uv/pull/12946))

### Documentation

- Add documentation about Python versions to Tools concept page ([#7673](https://github.com/astral-sh/uv/pull/7673))
- Add example of enabling Dependabot ([#13692](https://github.com/astral-sh/uv/pull/13692))
- Fix `exclude-newer` date format for persistent configuration files ([#13706](https://github.com/astral-sh/uv/pull/13706))
- Quote versions variables in GitLab documentation ([#13679](https://github.com/astral-sh/uv/pull/13679))
- Update Dependabot support status ([#13690](https://github.com/astral-sh/uv/pull/13690))
- Explicitly specify to add a new repo entry to the repos list item in the `.pre-commit-config.yaml` ([#10243](https://github.com/astral-sh/uv/pull/10243))
- Add integration with marimo guide ([#13691](https://github.com/astral-sh/uv/pull/13691))
- Add pronunciation to README ([#5336](https://github.com/astral-sh/uv/pull/5336))

## 0.7.8

### Python

We are reverting most of our Python changes from `uv 0.7.6` and `uv 0.7.7` due to
a miscompilation that makes the Python interpreter behave incorrectly, resulting
in spurious type-errors involving str. This issue seems to be isolated to
x86_64 Linux, and affected at least Python 3.12, 3.13, and 3.14.

The following changes that were introduced in those versions of uv are temporarily
being reverted while we test and deploy a proper fix for the miscompilation:

- Add Python 3.14 on musl
- free-threaded Python on musl
- Add Python 3.14.0a7
- Statically link `libpython` into the interpreter on Linux for a significant performance boost

See [the issue for details](https://github.com/astral-sh/uv/issues/13610).

### Documentation

- Remove misleading line in pin documentation ([#13611](https://github.com/astral-sh/uv/pull/13611))

## 0.7.7

### Python

- Work around third-party packages that (incorrectly) assume the interpreter is dynamically linking libpython
- Allow the experimental JIT to be enabled at runtime on Python 3.13 and 3.14 on macOS on aarch64 aka Apple Silicon

See the
[`python-build-standalone` release notes](https://github.com/astral-sh/python-build-standalone/releases/tag/20250521)
for more details.

### Bug fixes

- Make `uv version` lock and sync ([#13317](https://github.com/astral-sh/uv/pull/13317))
- Fix references to `ldd` in diagnostics to correctly refer to `ld.so` ([#13552](https://github.com/astral-sh/uv/pull/13552))

### Documentation

- Clarify adding SSH Git dependencies ([#13534](https://github.com/astral-sh/uv/pull/13534))

## 0.7.6

### Python

- Add Python 3.14 on musl
- Add free-threaded Python on musl
- Add Python 3.14.0a7
- Statically link `libpython` into the interpreter on Linux for a significant performance boost

See the
[`python-build-standalone` release notes](https://github.com/astral-sh/python-build-standalone/releases/tag/20250517)
for more details.

### Enhancements

- Improve compatibility of `VIRTUAL_ENV_PROMPT` value ([#13501](https://github.com/astral-sh/uv/pull/13501))
- Bump MSRV to 1.85 and Edition 2024 ([#13516](https://github.com/astral-sh/uv/pull/13516))

### Bug fixes

- Respect default extras in uv remove ([#13380](https://github.com/astral-sh/uv/pull/13380))

### Documentation

- Fix PowerShell code blocks ([#13511](https://github.com/astral-sh/uv/pull/13511))

## 0.7.5

### Bug fixes

- Support case-sensitive module discovery in the build backend ([#13468](https://github.com/astral-sh/uv/pull/13468))
- Bump Simple cache bucket to v16 ([#13498](https://github.com/astral-sh/uv/pull/13498))
- Don't error when the script is too short for the buffer ([#13488](https://github.com/astral-sh/uv/pull/13488))
- Add missing word in "script not supported" error ([#13483](https://github.com/astral-sh/uv/pull/13483))

## 0.7.4

### Enhancements

- Add more context to external errors ([#13351](https://github.com/astral-sh/uv/pull/13351))
- Align indentation of long arguments ([#13394](https://github.com/astral-sh/uv/pull/13394))
- Preserve order of dependencies which are sorted naively ([#13334](https://github.com/astral-sh/uv/pull/13334))
- Align progress bars by largest name length ([#13266](https://github.com/astral-sh/uv/pull/13266))
- Reinstall local packages in `uv add` ([#13462](https://github.com/astral-sh/uv/pull/13462))
- Rename `--raw-sources` to `--raw` ([#13348](https://github.com/astral-sh/uv/pull/13348))
- Show 'Downgraded' when `self update` is used to install an older version ([#13340](https://github.com/astral-sh/uv/pull/13340))
- Suggest `uv self update` if required uv version is newer ([#13305](https://github.com/astral-sh/uv/pull/13305))
- Add 3.14 beta images to uv Docker images ([#13390](https://github.com/astral-sh/uv/pull/13390))
- Add comma after "i.e." in Conda environment error ([#13423](https://github.com/astral-sh/uv/pull/13423))
- Be more precise in unpinned packages warning ([#13426](https://github.com/astral-sh/uv/pull/13426))
- Fix detection of sorted dependencies when include-group is used ([#13354](https://github.com/astral-sh/uv/pull/13354))
- Fix display of HTTP responses in trace logs for retry of errors ([#13339](https://github.com/astral-sh/uv/pull/13339))
- Log skip reasons during Python installation key interpreter match checks ([#13472](https://github.com/astral-sh/uv/pull/13472))
- Redact credentials when displaying URLs ([#13333](https://github.com/astral-sh/uv/pull/13333))

### Bug fixes

- Avoid erroring on `pylock.toml` dependency entries ([#13384](https://github.com/astral-sh/uv/pull/13384))
- Avoid panics for cannot-be-a-base URLs ([#13406](https://github.com/astral-sh/uv/pull/13406))
- Ensure cached realm credentials are applied if no password is found for index URL ([#13463](https://github.com/astral-sh/uv/pull/13463))
- Fix `.tgz` parsing to respect true extension ([#13382](https://github.com/astral-sh/uv/pull/13382))
- Fix double self-dependency ([#13366](https://github.com/astral-sh/uv/pull/13366))
- Reject `pylock.toml` in `uv add -r` ([#13421](https://github.com/astral-sh/uv/pull/13421))
- Retain dot-separated wheel tags during cache prune ([#13379](https://github.com/astral-sh/uv/pull/13379))
- Retain trailing comments after PEP 723 metadata block ([#13460](https://github.com/astral-sh/uv/pull/13460))

### Documentation

- Use "export" instead of "install" in `uv export` arguments ([#13430](https://github.com/astral-sh/uv/pull/13430))
- Remove extra newline ([#13461](https://github.com/astral-sh/uv/pull/13461))

### Preview features

- Build backend: Normalize glob paths ([#13465](https://github.com/astral-sh/uv/pull/13465))

## 0.7.3

### Enhancements

- Add `--dry-run` support to `uv self update` ([#9829](https://github.com/astral-sh/uv/pull/9829))
- Add `--show-with` to `uv tool list` to list packages included by `--with` ([#13264](https://github.com/astral-sh/uv/pull/13264))
- De-duplicate fetched index URLs ([#13205](https://github.com/astral-sh/uv/pull/13205))
- Support more zip compression formats: bzip2, lzma, xz, zstd ([#13285](https://github.com/astral-sh/uv/pull/13285))
- Add support for downloading GraalPy ([#13172](https://github.com/astral-sh/uv/pull/13172))
- Improve error message when a virtual environment Python symlink is broken ([#12168](https://github.com/astral-sh/uv/pull/12168))
- Use `fs_err` for paths in symlinking errors ([#13303](https://github.com/astral-sh/uv/pull/13303))
- Minify and embed managed Python JSON at compile time ([#12967](https://github.com/astral-sh/uv/pull/12967))

### Preview features

- Build backend: Make preview default and add configuration docs ([#12804](https://github.com/astral-sh/uv/pull/12804))
- Build backend: Allow escaping in globs ([#13313](https://github.com/astral-sh/uv/pull/13313))
- Build backend: Make builds reproducible across operating systems ([#13171](https://github.com/astral-sh/uv/pull/13171))

### Configuration

- Add `python-downloads-json-url` option for `uv.toml` to configure custom Python installations via JSON URL ([#12974](https://github.com/astral-sh/uv/pull/12974))

### Bug fixes

- Check nested IO errors for retries ([#13260](https://github.com/astral-sh/uv/pull/13260))
- Accept `musllinux_1_0` as a valid platform tag ([#13289](https://github.com/astral-sh/uv/pull/13289))
- Fix discovery of pre-release managed Python versions in range requests ([#13330](https://github.com/astral-sh/uv/pull/13330))
- Respect locked script preferences in `uv run --with` ([#13283](https://github.com/astral-sh/uv/pull/13283))
- Retry streaming downloads on broken pipe errors ([#13281](https://github.com/astral-sh/uv/pull/13281))
- Treat already-installed base environment packages as preferences in `uv run --with` ([#13284](https://github.com/astral-sh/uv/pull/13284))
- Avoid enumerating sources in errors for path Python requests ([#13335](https://github.com/astral-sh/uv/pull/13335))
- Avoid re-creating virtual environment with `--no-sync` ([#13287](https://github.com/astral-sh/uv/pull/13287))

### Documentation

- Remove outdated description of index strategy ([#13326](https://github.com/astral-sh/uv/pull/13326))
- Update "Viewing the version" docs ([#13241](https://github.com/astral-sh/uv/pull/13241))

## 0.7.2

### Enhancements

- Improve trace log for retryable errors ([#13228](https://github.com/astral-sh/uv/pull/13228))
- Use "error" instead of "warning" for self-update message ([#13229](https://github.com/astral-sh/uv/pull/13229))
- Error when `uv version` is used with project-specific flags but no project is found ([#13203](https://github.com/astral-sh/uv/pull/13203))

### Bug fixes

- Fix incorrect virtual environment invalidation for pre-release Python versions ([#13234](https://github.com/astral-sh/uv/pull/13234))
- Fix patching of `clang` in managed Python sysconfig ([#13237](https://github.com/astral-sh/uv/pull/13237))
- Respect `--project` in `uv version` ([#13230](https://github.com/astral-sh/uv/pull/13230))

## 0.7.1

### Enhancement

- Add support for BLAKE2b-256 ([#13204](https://github.com/astral-sh/uv/pull/13204))

### Bugfix

- Revert fix handling of authentication when encountering redirects ([#13215](https://github.com/astral-sh/uv/pull/13215))

## 0.7.0

This release contains various changes that improve correctness and user experience, but could break some workflows; many changes have been marked as breaking out of an abundance of caution. We expect most users to be able to upgrade without making changes.

### Breaking changes

- **Update `uv version` to display and update project versions ([#12349](https://github.com/astral-sh/uv/pull/12349))**

  Previously, `uv version` displayed uv's version. Now, `uv version` will display or update the project's version. This interface was [heavily requested](https://github.com/astral-sh/uv/issues/6298) and, after much consideration, we decided that transitioning the top-level command was the best option.

  Here's a brief example:

  ```console
  $ uv init example
  Initialized project `example` at `./example`
  $ cd example
  $ uv version
  example 0.1.0
  $ uv version --bump major
  example 0.1.0 => 1.0.0
  $ uv version --short
  1.0.0
  ```

  If used outside of a project, uv will fallback to showing its own version still:

  ```console
  $ uv version
  warning: failed to read project: No `pyproject.toml` found in current directory or any parent directory
    running `uv self version` for compatibility with old `uv version` command.
    this fallback will be removed soon, pass `--preview` to make this an error.

  uv 0.7.0 (4433f41c9 2025-04-29)
  ```

  As described in the warning, `--preview` can be used to error instead:

  ```console
  $ uv version --preview
  error: No `pyproject.toml` found in current directory or any parent directory
  ```

  The previous functionality of `uv version` was moved to `uv self version`.
- **Avoid fallback to subsequent indexes on authentication failure ([#12805](https://github.com/astral-sh/uv/pull/12805))**

  When using the `first-index` strategy (the default), uv will stop searching indexes for a package once it is found on a single index. Previously, uv considered a package as "missing" from an index during authentication failures, such as an HTTP 401 or HTTP 403 (normally, missing packages are represented by an HTTP 404). This behavior was motivated by unusual responses from some package indexes, but reduces the safety of uv's index strategy when authentication fails. Now, uv will consider an authentication failure as a stop-point when searching for a package across indexes. The `index.ignore-error-codes` option can be used to recover the existing behavior, e.g.:

  ```toml
  [[tool.uv.index]]
  name = "pytorch"
  url = "https://download.pytorch.org/whl/cpu"
  ignore-error-codes = [401, 403]
  ```

  Since PyTorch's indexes always return a HTTP 403 for missing packages, uv special-cases indexes on the `pytorch.org` domain to ignore that error code by default.
- **Require the command in `uvx <name>` to be available in the Python environment ([#11603](https://github.com/astral-sh/uv/pull/11603))**

  Previously, `uvx` would attempt to execute a command even if it was not provided by a Python package. For example, if we presume `foo` is an empty Python package which provides no command, `uvx foo` would invoke the `foo` command on the `PATH` (if present). Now, uv will error early if the `foo` executable is not provided by the requested Python package. This check is not enforced when `--from` is used, so patterns like `uvx --from foo bash -c "..."` are still valid. uv also still allows `uvx foo` where the `foo` executable is provided by a dependency of `foo` instead of `foo` itself, as this is fairly common for packages which depend on a dedicated package for their command-line interface.
- **Use index URL instead of package URL for keyring credential lookups ([#12651](https://github.com/astral-sh/uv/pull/12651))**

  When determining credentials for querying a package URL, uv previously sent the full URL to the `keyring` command. However, some keyring plugins expect to receive the *index URL* (which is usually a parent of the package URL). Now, uv requests credentials for the index URL instead. This behavior matches `pip`.
- **Remove `--version` from subcommands ([#13108](https://github.com/astral-sh/uv/pull/13108))**

  Previously, uv allowed the `--version` flag on arbitrary subcommands, e.g., `uv run --version`. However, the `--version` flag is useful for other operations since uv is a package manager. Consequently, we've removed the `--version` flag from subcommands — it is only available as `uv --version`.
- **Omit Python 3.7 downloads from managed versions ([#13022](https://github.com/astral-sh/uv/pull/13022))**

  Python 3.7 is EOL and not formally supported by uv; however, Python 3.7 was previously available for download on a subset of platforms.
- **Reject non-PEP 751 TOML files in install, compile, and export commands ([#13120](https://github.com/astral-sh/uv/pull/13120), [#13119](https://github.com/astral-sh/uv/pull/13119))**

  Previously, uv treated arbitrary `.toml` files passed to commands (e.g., `uv pip install -r foo.toml` or `uv pip compile -o foo.toml`) as `requirements.txt`-formatted files. Now, uv will error instead. If using PEP 751 lockfiles, use the standardized format for custom names instead, e.g., `pylock.foo.toml`.
- **Ignore arbitrary Python requests in version files ([#12909](https://github.com/astral-sh/uv/pull/12909))**

  uv allows arbitrary strings to be used for Python version requests, in which they are treated as an executable name to search for in the `PATH`. However, using this form of request in `.python-version` files is non-standard and conflicts with `pyenv-virtualenv` which writes environment names to `.python-version` files. In this release, uv will now ignore requests that are arbitrary strings when found in `.python-version` files.
- **Error on unknown dependency object specifiers ([12811](https://github.com/astral-sh/uv/pull/12811))**

  The `[dependency-groups]` entries can include "object specifiers", e.g. `set-phasers-to = ...` in:

  ```toml
  [dependency-groups]
  foo = ["pyparsing"]
  bar = [{set-phasers-to = "stun"}]
  ```

  However, the only current spec-compliant object specifier is `include-group`. Previously, uv would ignore unknown object specifiers. Now, uv will error.
- **Make `--frozen` and `--no-sources` conflicting options ([#12671](https://github.com/astral-sh/uv/pull/12671))**

  Using `--no-sources` always requires a new resolution and `--frozen` will always fail when used with it. Now, this conflict is encoded in the CLI options for clarity.
- **Treat empty `UV_PYTHON_INSTALL_DIR` and `UV_TOOL_DIR` as unset ([#12907](https://github.com/astral-sh/uv/pull/12907), [#12905](https://github.com/astral-sh/uv/pull/12905))**

  Previously, these variables were treated as set to the current working directory when set to an empty string. Now, uv will ignore these variables when empty. This matches uv's behavior for other environment variables which configure directories.

### Enhancements

- Disallow mixing requirements across PyTorch indexes ([#13179](https://github.com/astral-sh/uv/pull/13179))
- Add optional managed Python archive download cache ([#12175](https://github.com/astral-sh/uv/pull/12175))
- Add `poetry-core` as a `uv init` build backend option ([#12781](https://github.com/astral-sh/uv/pull/12781))
- Show tag hints when failing to find a compatible wheel in `pylock.toml` ([#13136](https://github.com/astral-sh/uv/pull/13136))
- Report Python versions in `pyvenv.cfg` version mismatch ([#13027](https://github.com/astral-sh/uv/pull/13027))

### Bug fixes

- Avoid erroring on omitted wheel-only packages in `pylock.toml` ([#13132](https://github.com/astral-sh/uv/pull/13132))
- Fix display name for `uvx --version` ([#13109](https://github.com/astral-sh/uv/pull/13109))
- Restore handling of authentication when encountering redirects ([#13050](https://github.com/astral-sh/uv/pull/13050))
- Respect build options (`--no-binary` et al) in `pylock.toml` ([#13134](https://github.com/astral-sh/uv/pull/13134))
- Use `upload-time` rather than `upload_time` in `uv.lock` ([#13176](https://github.com/astral-sh/uv/pull/13176))

### Documentation

- Changed `fish` completions append `>>` to overwrite `>` ([#13130](https://github.com/astral-sh/uv/pull/13130))
- Add `pylock.toml` mentions where relevant ([#13115](https://github.com/astral-sh/uv/pull/13115))
- Add ROCm example to the PyTorch guide ([#13200](https://github.com/astral-sh/uv/pull/13200))
- Upgrade PyTorch guide to CUDA 12.8 and PyTorch 2.7 ([#13199](https://github.com/astral-sh/uv/pull/13199))

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


