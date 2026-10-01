# Changelog

<!-- prettier-ignore-start -->


## 0.13.0

Unreleased.

### Breaking changes

- **Prefer native Python on Windows ARM64** ([#22100](https://github.com/astral-sh/uv/pull/22100))

  Previously, native ARM64 builds of uv preferred emulated `x86_64` Python installations because native wheel support was limited, now uv prefers ARM64 (a.k.a. `aarch64`) interpreters across Python versions. This follows the ecosystem-wide transition including [CPython](https://discuss.python.org/t/python-on-windows-arm64/104524), the official Windows [Python install manager](https://discuss.python.org/t/python-install-manager-26-4/108846), and GitHub's [actions/setup-python](https://github.com/actions/setup-python#supported-architectures).
  
  uv continues falls back to `x86_64`, then 32-bit `x86`, when a native interpreter is unavailable.

  Set `UV_PYTHON_ARCH=x86_64` to keep using emulated Python, or request an explicit architecture such as `cpython-3.14-windows-x86_64`.

- **Omit the distutils startup patch on Python 3.10 and later**
  ([#22096](https://github.com/astral-sh/uv/pull/22096))

  Previously, uv installed `_virtualenv.py` and `_virtualenv.pth` into every new virtual environment
  to prevent distutils configuration from changing installation paths. Now, uv omits these files on
  Python 3.10 and later, which already ignore the affected configuration keys. This reduces Python
  startup overhead. Python 3.9 and earlier retain the patch.

  You cannot opt out of this behavior. Existing virtual environments are not modified automatically.
  Recreate an environment with Python 3.10 or later to remove the patch.

  This stabilizes the `no-distutils-patch` preview feature.

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

