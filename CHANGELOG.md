# Changelog

<!-- prettier-ignore-start -->


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
  
  Alpine 3.21 was released in Dec 2024 and is used in the official Alpine-based Python images. Our `uv:python3.x-alpine` images have been using 3.21 since uv v0.5.8. However, now the the `uv:alpine` image will use 3.21 instead of 3.20 and `uv:alpine3.20` will no longer be updated.
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


