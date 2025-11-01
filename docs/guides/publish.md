# Publishing a package

uv supports building Python packages into source and binary distributions via `uv build`.

As uv does not yet have a dedicated command for publishing packages, you can use the PyPA tool
[`twine`](https://github.com/pypa/twine) to upload your package to a package registry, which can be
invoked via `uvx`.

## Preparing your project for packaging

Before attempting to publish your project, you'll want to make sure it's ready to be packaged for
distribution.

If your project does not include a `[build-system]` definition in the `pyproject.toml`, uv will not
build it by default. This means that your project may not be ready for distribution. Read more about
the effect of declaring a build system in the
[project concept](../concepts/projects.md#build-systems) documentation.

## Building your package

Build your package with `uv build`:

```console
$ uv build
```

By default, `uv build` will build the project in the current directory, and place the built
artifacts in a `dist/` subdirectory.

Alternatively, `uv build <SRC>` will build the package in the specified directory, while
`uv build --package <PACKAGE>` will build the specified package within the current workspace.

## Publishing your package

Publish your package with `twine`:

```console
$ uvx twine upload dist/*
```

!!! tip

    To provide credentials, use the `TWINE_USERNAME` and `TWINE_PASSWORD` environment variables.

## Installing your package

Test that the package can be installed and imported with `uv run`:

```console
$ uv run --with <PACKAGE> --no-project -- python -c "import <PACKAGE>"
```

The `--no-project` flag is used to avoid installing the package from your local project directory.

!!! tip

    If you have recently installed the package, you may need to include the
    `--refresh-package <PACKAGE>` option to avoid using a cached version of the package.

## Including man pages

If your package provides command-line tools, you may want to include man pages (manual pages) that
users can view with the `man` command. When installed via `uv tool install`, man pages are
automatically discovered and installed alongside your tool's executables.

### Structuring man pages

Place man pages in your package with the following structure:

```
your-package/
  your_package/
    __init__.py
  share/man/
    man1/
      your-tool.1
    man5/
      your-tool-config.5
```

Man pages should be organized by section:

- `man1/` - User commands and executables
- `man5/` - Configuration file formats
- `man7/` - Overviews and conventions
- Other sections (2, 3, 4, 6, 8, 9) as appropriate

### Build system configuration

Configure your build system to include man pages as data files:

#### hatchling

```toml
[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[tool.hatch.build.targets.wheel.shared-data]
"share/man" = "share/man"
```

#### setuptools

```python
from setuptools import setup

setup(
    name="your-package",
    # ...
    data_files=[
        ('share/man/man1', ['share/man/man1/your-tool.1']),
        ('share/man/man5', ['share/man/man5/your-tool-config.5']),
    ],
)
```

#### setuptools (pyproject.toml)

```toml
[build-system]
requires = ["setuptools"]
build-backend = "setuptools.build_meta"

[tool.setuptools]
packages = ["your_package"]

[tool.setuptools.data-files]
"share/man/man1" = ["share/man/man1/your-tool.1"]
"share/man/man5" = ["share/man/man5/your-tool-config.5"]
```

#### maturin

```toml
[tool.maturin]
data = { "share/man" = "share/man" }
```

### Verification

After building your package, verify that man pages are included correctly:

1. Build the package:

   ```console
   $ uv build
   ```

2. Extract and inspect the wheel:

   ```console
   $ unzip -l dist/your_package-*.whl | grep "data.*man"
   ```

   Man pages should appear at paths like:

   ```
   your_package-1.0.0.data/data/share/man/man1/your-tool.1
   ```

3. Test installation:
   ```console
   $ uv tool install dist/your_package-*.whl
   $ man your-tool
   ```

For more information about man page support in uv, see the [man pages guide](./tools/man-pages.md).

## Next steps

To learn more about publishing packages, check out the
[PyPA guides](https://packaging.python.org/en/latest/guides/section-build-and-publish/) on building
and publishing.

Or, read on for more details about the concepts in uv.
