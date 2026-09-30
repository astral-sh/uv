# Changelog

<!-- prettier-ignore-start -->


## 0.13.0

Unreleased.

### Breaking changes

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

  uv now uses `tar-codec` when extracting tar archives, building source distributions with `uv_build`, and reading source distribution metadata for publishing. The stricter archive validation can reject malformed archives and archives with unsupported entries that `astral-tokio-tar` previously accepted. Source distributions built by `uv_build` may also have different archive bytes.

  Set `UV_NO_TAR_CODEC=1` to use `astral-tokio-tar` if a workflow depends on the previous behavior.

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
