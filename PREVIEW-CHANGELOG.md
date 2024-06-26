# Changelog

## 0.2.17

### Preview features

- Add `--extra` to `uv add` and enable fine grained updates ([#4566](https://github.com/astral-sh/uv/pull/4566))

## 0.2.16

### Preview features

- Add `--package` argument to `uv add` and `uv remove` ([#4556](https://github.com/astral-sh/uv/pull/4556))
- Add `uv tool install` ([#4492](https://github.com/astral-sh/uv/pull/4492))
- Fallback to interpreter discovery in `uv run` ([#4549](https://github.com/astral-sh/uv/pull/4549))
- Make `uv.sources` without `--preview` non-fatal ([#4558](https://github.com/astral-sh/uv/pull/4558))
- Remove non-existent extras from lockfile ([#4479](https://github.com/astral-sh/uv/pull/4479))
- Support conflicting URL in separate forks ([#4435](https://github.com/astral-sh/uv/pull/4435))
- Automatically detect workspace packages in `uv add` ([#4557](https://github.com/astral-sh/uv/pull/4557))
- Omit `distribution.sdist` from lockfile when it is redundant ([#4528](https://github.com/astral-sh/uv/pull/4528))
- Remove `source` and `version` from lock file when unambiguous ([#4513](https://github.com/astral-sh/uv/pull/4513))
- Allow `uv lock` to read overrides from `tool.uv` (#4108) ([#4369](https://github.com/astral-sh/uv/pull/4369))

## 0.2.15

<!-- No changes -->

## 0.2.14

### Preview features

- Expose `toolchain-preference` as a CLI and configuration file option ([#4424](https://github.com/astral-sh/uv/pull/4424))
- Improve handling of command arguments in `uv run` and `uv tool run` ([#4404](https://github.com/astral-sh/uv/pull/4404))
- Add `tool.uv.sources` support for `uv add` ([#4406](https://github.com/astral-sh/uv/pull/4406))
- Use correct lock path for workspace dependencies ([#4421](https://github.com/astral-sh/uv/pull/4421))
- Filter out sibling dependencies in resolver forks ([#4415](https://github.com/astral-sh/uv/pull/4415))

## 0.2.13

### Preview features

- Add `--workspace` option to `uv add` ([#4362](https://github.com/astral-sh/uv/pull/4362))
- Ignore query errors during `uv toolchain list` ([#4382](https://github.com/astral-sh/uv/pull/4382))
- Respect `.python-version` files and fetch manged toolchains in uv project commands ([#4361](https://github.com/astral-sh/uv/pull/4361))
- Respect `.python-version` in `uv venv --preview` ([#4360](https://github.com/astral-sh/uv/pull/4360))

## 0.2.12

### Preview features

- Add `--force` option to `uv toolchain install` ([#4313](https://github.com/astral-sh/uv/pull/4313))
- Add `--no-build`, `--no-build-package`, and binary variants ([#4322](https://github.com/astral-sh/uv/pull/4322))
- Add `EXTERNALLY-MANAGED` markers to managed toolchains ([#4312](https://github.com/astral-sh/uv/pull/4312))
- Add `uv toolchain find` ([#4206](https://github.com/astral-sh/uv/pull/4206))
- Add persistent configuration for non-`pip` APIs ([#4294](https://github.com/astral-sh/uv/pull/4294))
- Add support for adding/removing development dependencies ([#4327](https://github.com/astral-sh/uv/pull/4327))
- Add support for listing system toolchains ([#4172](https://github.com/astral-sh/uv/pull/4172))
- Add support for toolchain requests by key ([#4332](https://github.com/astral-sh/uv/pull/4332))
- Allow multiple toolchains to be requested in `uv toolchain install` ([#4334](https://github.com/astral-sh/uv/pull/4334))
- Fix relative and absolute path handling in lockfiles ([#4266](https://github.com/astral-sh/uv/pull/4266))
- Load configuration options from workspace root ([#4295](https://github.com/astral-sh/uv/pull/4295))
- Omit project name from workspace errors ([#4299](https://github.com/astral-sh/uv/pull/4299))
- Read Python version files during toolchain installs ([#4335](https://github.com/astral-sh/uv/pull/4335))
- Remove extraneous installations in `uv sync` by default ([#4366](https://github.com/astral-sh/uv/pull/4366))
- Respect `requires-python` in `uv lock` ([#4282](https://github.com/astral-sh/uv/pull/4282))
- Respect workspace-wide `requires-python` in interpreter selection ([#4298](https://github.com/astral-sh/uv/pull/4298))
- Support unnamed requirements in `uv add` ([#4326](https://github.com/astral-sh/uv/pull/4326))
- Use portable slash paths in lockfile ([#4324](https://github.com/astral-sh/uv/pull/4324))
- Use registry URL for fetching source distributions from lockfile ([#4280](https://github.com/astral-sh/uv/pull/4280))
- `uv sync --no-clean` ([#4367](https://github.com/astral-sh/uv/pull/4367))
- Filter dependencies by tracking markers on resolver forks ([#4339](https://github.com/astral-sh/uv/pull/4339))
- Use `Requires-Python` to filter dependencies during universal resolution ([#4273](https://github.com/astral-sh/uv/pull/4273))

## 0.2.11

### Preview features

- Add changelog for preview changes ([#4251](https://github.com/astral-sh/uv/pull/4251))
- Allow direct URLs for dev dependencies ([#4233](https://github.com/astral-sh/uv/pull/4233))
- Create temporary environments in dedicated cache bucket ([#4223](https://github.com/astral-sh/uv/pull/4223))
- Improve output when an older toolchain version is already installed ([#4248](https://github.com/astral-sh/uv/pull/4248))
- Initial implementation of `uv add` and `uv remove` ([#4193](https://github.com/astral-sh/uv/pull/4193))
- Refactor project interpreter request for `requires-python` specifiers ([#4216](https://github.com/astral-sh/uv/pull/4216))
- Replace `toolchain fetch` with `toolchain install` ([#4228](https://github.com/astral-sh/uv/pull/4228))
- Support locking relative paths ([#4205](https://github.com/astral-sh/uv/pull/4205))
- Warn when 'requires-python' does not include a lower bound ([#4234](https://github.com/astral-sh/uv/pull/4234))

## 0.2.10

### Preview features

- Add `uv toolchain install` ([#4164](https://github.com/astral-sh/uv/pull/4164))
- Add `uv toolchain list` ([#4163](https://github.com/astral-sh/uv/pull/4163))
- Add extra and dev dependency validation to lockfile ([#4112](https://github.com/astral-sh/uv/pull/4112))
- Add markers to edges rather than distributions ([#4166](https://github.com/astral-sh/uv/pull/4166))
- Cap `Requires-Python` comparisons at the patch version ([#4150](https://github.com/astral-sh/uv/pull/4150))
- Do not create a virtual environment when locking ([#4147](https://github.com/astral-sh/uv/pull/4147))
- Don't panic with invalid wheel source ([#4191](https://github.com/astral-sh/uv/pull/4191))
- Fetch managed toolchains in `uv run` ([#4143](https://github.com/astral-sh/uv/pull/4143))
- Fix PEP 508 link in preview doc `specifying_dependencies` ([#4158](https://github.com/astral-sh/uv/pull/4158))
- Ignore tags in universal resolution ([#4174](https://github.com/astral-sh/uv/pull/4174))
- Implement `Toolchain::find_or_fetch` and use in `uv venv --preview` ([#4138](https://github.com/astral-sh/uv/pull/4138))
- Lock all packages in workspace ([#4016](https://github.com/astral-sh/uv/pull/4016))
- Recreate project environment if `--python` or `requires-python` doesn't match ([#3945](https://github.com/astral-sh/uv/pull/3945))
- Respect `--find-links` in `lock` and `sync` ([#4183](https://github.com/astral-sh/uv/pull/4183))
- Set `--dev` to default for `uv run` and `uv sync` ([#4118](https://github.com/astral-sh/uv/pull/4118))
- Track `Markers` via a PubGrub package variant ([#4123](https://github.com/astral-sh/uv/pull/4123))
- Use union of `requires-python` in workspace ([#4041](https://github.com/astral-sh/uv/pull/4041))
- make universal resolver fork only when markers are disjoint ([#4135](https://github.com/astral-sh/uv/pull/4135))

## 0.2.9

### Preview features

- Add support for development dependencies ([#4036](https://github.com/astral-sh/uv/pull/4036))
- Avoid enforcing distribution ID uniqueness for extras ([#4104](https://github.com/astral-sh/uv/pull/4104))
- Ignore upper-bounds on `Requires-Python` ([#4086](https://github.com/astral-sh/uv/pull/4086))

## 0.2.8

### Preview features

- Default to current Python minor if `Requires-Python` is absent ([#4070](https://github.com/astral-sh/uv/pull/4070))
- Enforce `Requires-Python` when syncing ([#4068](https://github.com/astral-sh/uv/pull/4068))
- Track supported Python range in lockfile ([#4065](https://github.com/astral-sh/uv/pull/4065))

## 0.2.7

### Preview features

- Fix a bug where no warning is output when parsing of workspace settings fails. ([#4014](https://github.com/astral-sh/uv/pull/4014))
- Normalize extras in lockfile ([#3958](https://github.com/astral-sh/uv/pull/3958))
- Respect `Requires-Python` in universal resolution ([#3998](https://github.com/astral-sh/uv/pull/3998))

## 0.2.6

### Preview features

- Add `uv run --package` ([#3864](https://github.com/astral-sh/uv/pull/3864))
- Add index URL parameters to Project CLI ([#3984](https://github.com/astral-sh/uv/pull/3984))
- Avoid re-adding solutions to forked state ([#3967](https://github.com/astral-sh/uv/pull/3967))
- Draft for user docs for workspaces ([#3866](https://github.com/astral-sh/uv/pull/3866))
- Include all extras when generating lockfile ([#3912](https://github.com/astral-sh/uv/pull/3912))
- Remove unstable uv lock from pip interface ([#3970](https://github.com/astral-sh/uv/pull/3970))
- Respect resolved Git SHAs in `uv lock` ([#3956](https://github.com/astral-sh/uv/pull/3956))
- Use lockfile in `uv run` ([#3894](https://github.com/astral-sh/uv/pull/3894))
- Use lockfile versions as resolution preferences ([#3921](https://github.com/astral-sh/uv/pull/3921))
- Use universal resolution in `uv lock` ([#3969](https://github.com/astral-sh/uv/pull/3969))

## 0.2.5

### Preview features

- Add context to failed `uv tool run` ([#3882](https://github.com/astral-sh/uv/pull/3882))
- Add persistent storage of installed toolchains ([#3797](https://github.com/astral-sh/uv/pull/3797))
- Gate discovery of managed toolchains with preview ([#3835](https://github.com/astral-sh/uv/pull/3835))
- Initial workspace support ([#3705](https://github.com/astral-sh/uv/pull/3705))
- Move editable discovery behind `--preview` for now ([#3884](https://github.com/astral-sh/uv/pull/3884))

## 0.2.4

<!-- No changes -->

## 0.2.3

### Preview features

- Allow specification of additional requirements in `uv tool run` ([#3678](https://github.com/astral-sh/uv/pull/3678))

## 0.2.2

<!-- No changes -->

## 0.2.1

### Preview features

- Allow users to specify a custom source package to `uv tool run` ([#3677](https://github.com/astral-sh/uv/pull/3677))

## 0.2.0

### Preview features

- Add initial implementation of `uv tool run` ([#3657](https://github.com/astral-sh/uv/pull/3657))
- Add offline support to `uv tool run` and `uv run` ([#3676](https://github.com/astral-sh/uv/pull/3676))
- Better error message for `uv run` failures ([#3691](https://github.com/astral-sh/uv/pull/3691))
- Discover workspaces without using them in resolution ([#3585](https://github.com/astral-sh/uv/pull/3585))
- Support editables in `uv sync` ([#3692](https://github.com/astral-sh/uv/pull/3692))
- Track editable requirements in lockfile ([#3725](https://github.com/astral-sh/uv/pull/3725))

## 0.1.45

### Preview features

- Add direct URL conversion to lockfile ([#3633](https://github.com/astral-sh/uv/pull/3633))
- Add hashes and versions to all distributions ([#3589](https://github.com/astral-sh/uv/pull/3589))
- Add local path conversions from lockfile ([#3609](https://github.com/astral-sh/uv/pull/3609))
- Add missing `"directory"` branch in source match ([#3608](https://github.com/astral-sh/uv/pull/3608))
- Add registry file size to lockfile ([#3652](https://github.com/astral-sh/uv/pull/3652))
- Add registry source distribution support to lockfile ([#3649](https://github.com/astral-sh/uv/pull/3649))
- Refactor editables for supporting them in bluejay commands ([#3639](https://github.com/astral-sh/uv/pull/3639))
- Rename `sourcedist` to `sdist` in lockfile ([#3590](https://github.com/astral-sh/uv/pull/3590))
- Respect installed packages in `uv run` ([#3603](https://github.com/astral-sh/uv/pull/3603))
- Support lossless serialization for Git dependencies in lockfile ([#3630](https://github.com/astral-sh/uv/pull/3630))

## 0.1.44

<!-- No changes -->

## 0.1.43

### Preview features

- Create virtualenv if it doesn't exist in project API ([#3499](https://github.com/astral-sh/uv/pull/3499))
- Discover `uv run` projects hierarchically ([#3494](https://github.com/astral-sh/uv/pull/3494))
- Read and write `uv.lock` based on project root ([#3497](https://github.com/astral-sh/uv/pull/3497))
- Read package name from `pyproject.toml` in `uv run` ([#3496](https://github.com/astral-sh/uv/pull/3496))
- Rebrand workspace API as project API ([#3489](https://github.com/astral-sh/uv/pull/3489))

## 0.1.42

### Preview features

- Use environment layering for `uv run --with` ([#3447](https://github.com/astral-sh/uv/pull/3447))
- Warn when missing minimal bounds when using `tool.uv.sources` ([#3452](https://github.com/astral-sh/uv/pull/3452))

## 0.1.41

<!-- No changes -->

## 0.1.40

### Preview features

- Add basic `tool.uv.sources` support ([#3263](https://github.com/astral-sh/uv/pull/3263))
- Improve non-git error message ([#3403](https://github.com/astral-sh/uv/pull/3403))
- Preserve given for `tool.uv.sources` paths ([#3412](https://github.com/astral-sh/uv/pull/3412))
- Restore verbatim in error message ([#3402](https://github.com/astral-sh/uv/pull/3402))
- Use preview mode for tool.uv.sources ([#3277](https://github.com/astral-sh/uv/pull/3277))
- Use top-level `--isolated` for `uv run` ([#3431](https://github.com/astral-sh/uv/pull/3431))
- add basic "install from lock file" operation ([#3340](https://github.com/astral-sh/uv/pull/3340))
- uv-resolver: add initial version of universal lock file format ([#3314](https://github.com/astral-sh/uv/pull/3314))
