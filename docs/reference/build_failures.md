# Build failures

This page lists common reasons why resolution and installation fails with a build error and how to
fix them.

### Why does uv build a package?

When generating the cross-platform lockfile, uv needs to determine the dependencies of all packages,
even those only installed on other platforms. uv tries to avoid package builds during resolution. It
uses any wheel if exist for that version, then tries to find static metadata in the source
distribution (mainly pyproject.toml with static `project.version`, `project.dependencies` and
`project.optional-dependencies` or METADATA of at least version 2.2). Only if all of that fails, it
builds the package.

When installing, uv needs to have a wheel for the current platform for each package. If no matching
wheel exists in the index, uv tries to build the source distribution.

You can check which wheels exist for a PyPI project under “Download Files”, e.g.
https://pypi.org/project/numpy/2.1.1/#files. Wheels with `...-py3-none-any.whl` filenames work
everywhere, others have the operating system and platform in the filename. For the linked numpy
version, you can see that Python 3.10 to 3.13 on MacOS, Linux and Windows are supported.

### Fixes and Workarounds

- If the build error mentions a missing header or library, there is often a matching package in your
  system package manager.

       Example: When `uv pip install mysqlclient==2.2.4` fails on Ubuntu, you need to run
       `sudo apt install default-libmysqlclient-dev build-essential pkg-config` to install the MySQL
       headers ([https://pypi.org/project/mysqlclient/2.2.4/](https://pypi.org/project/mysqlclient/2.2.4/#Linux))

- If the build error mentions a failing import, consider
  [deactivating build isolation](https://docs.astral.sh/uv/concepts/projects/#build-isolation).
- If a package fails to build during resolution and the version that failed to build is older than
  the version you want to use, try adding a
  [constraint](https://docs.astral.sh/uv/reference/settings/#constraint-dependencies) with a lower
  bound (e.g. `numpy>=1.17`). Sometimes, due to algorithmic limitations, the uv resolver tries to
  find a fitting version using unreasonably old packages, which can be prevented by using lower
  bounds.
- Consider using a different Python version for locking and/or installation (`-p`). If you are using
  an older Python version, you may need to use an older version of certain packages with native code
  too, especially for scientific code. Example: torch 1.12.0 support Python 3.7 to 3.10
  (https://pypi.org/project/torch/1.12.0/#files), while numpy 2.1.0 supports Python 3.10 to 3.13
  (https://numpy.org/doc/stable/release/2.1.0-notes.html#numpy-2-1-0-release-notes), so both
  together mean you need Python 3.10 (or upgrade torch).
- If locking fails due to building a package from a platform you do not support, consider
  [declaring resolver environments](https://docs.astral.sh/uv/reference/settings/#environments) with
  your supported platforms.
- If you support a large range of Python versions, consider using markers to use older versions for
  older Python versions and newer versions for newer Python version. In the example, numpy tends to
  support four Python minor version at a time, so to support Python 3.8 to 3.13, the versions need
  to be split:

      ```
      numpy>=1.23; python_version >= "3.10"
      numpy<1.23; python_version < "3.10"
      ```

- If locking fails due to building a package from a different platform, as an escape hatch you can
  [provide dependency metadata manually](https://docs.astral.sh/uv/reference/settings/#dependency-metadata).
  As uv can not verify this information, it is important to specify correct metadata in this
  override.
