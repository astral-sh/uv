# Changelog

<!-- prettier-ignore-start -->


## 0.13.0

Unreleased.

### Breaking changes

- **Prefer native Python on Windows ARM64** ([#22100](https://github.com/astral-sh/uv/pull/22100))

  Previously, native ARM64 builds of uv preferred emulated `x86_64` Python installations because native wheel support was limited, now uv prefers ARM64 (a.k.a. `aarch64`) interpreters across Python versions. This follows the ecosystem-wide transition including [CPython](https://discuss.python.org/t/python-on-windows-arm64/104524), the official Windows [Python install manager](https://discuss.python.org/t/python-install-manager-26-4/108846), and GitHub's [actions/setup-python](https://github.com/actions/setup-python#supported-architectures).

  When a native interpreter is unavailable, uv continues to fall back to `x86_64`, then 32-bit `x86`.

  Set `UV_PYTHON_ARCH=x86_64` to keep using emulated Python, or request an explicit architecture such as `cpython-3.14-windows-x86_64`.

- **Reject editable requirements in included constraints files**
  ([#22282](https://github.com/astral-sh/uv/pull/22282))

  Previously, uv silently ignored editable (`-e`) requirements in constraints files included with
  `-c` from a requirements file. Now, uv rejects these requirements with an error, matching
  [pip's behavior](https://pip.pypa.io/en/stable/user_guide/#constraints-files).

  Move editable requirements to a requirements file passed with `-r`, or pass them directly with
  `--editable`, instead of including them in a constraints file.

- **Omit the distutils startup patch on Python 3.10 and later**
  ([#22096](https://github.com/astral-sh/uv/pull/22096))

  Previously, uv installed `_virtualenv.py` and `_virtualenv.pth` into every new virtual environment
  to prevent distutils configuration from changing installation paths. Now, uv omits these files on
  Python 3.10 and later, which already ignore the affected configuration keys. This reduces Python
  startup overhead. Python 3.9 and earlier retain the patch.

  You cannot opt out of this behavior. Existing virtual environments are not modified automatically.
  Recreate an environment with Python 3.10 or later to remove the patch.

  This stabilizes the `no-distutils-patch` preview feature.

- **Use `tar-codec` for tar archives by default** ([#22094](https://github.com/astral-sh/uv/pull/22094))

  Previously, uv used `astral-tokio-tar` to extract tar archives, build source distributions with `uv_build`, and read their metadata for `uv publish`. Now, uv uses `tar-codec`, which applies stricter validation when reading archives. This can cause uv to reject archives containing hard links or unsupported tar extensions that previous versions accepted. Source distributions created by `uv_build` can also have different archive bytes and hashes.

  You can opt out of this behavior by setting `UV_LEGACY_TAR_BACKEND=1`.

  This stabilizes the `tar-codec` preview feature.

## 0.12.x

See [changelogs/0.12.x](./changelogs/0.12.x.md)

## 0.11.x

See [changelogs/0.11.x](./changelogs/0.11.x.md)

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
