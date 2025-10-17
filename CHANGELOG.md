# Changelog

<!-- prettier-ignore-start -->


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

See [changeslogs/0.8.x](./changelogs/0.8.x.md)

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


