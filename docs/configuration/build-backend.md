# The uv build backend

!!! note

    The uv build backend is currently in preview and may change without warning.

    When preview mode is not enabled, uv uses [hatchling](https://pypi.org/project/hatchling/) as the default build backend.

A build backend transforms a source tree (i.e., a directory) into a source distribution or a wheel.
While uv supports all build backends (as specified by PEP 517), it includes a `uv_build` backend
that integrates tightly with uv to improve performance and user experience.

The uv build backend currently only supports Python code. An alternative backend is required if you
want to create a
[library with extension modules](../concepts/projects/init.md#projects-with-extension-modules).

To use the uv build backend as [build system](../concepts/projects/config.md#build-systems) in an
existing project, add it to the `[build-system]` section in your `pyproject.toml`:

```toml
[build-system]
requires = ["uv_build>=0.7.8,<0.8.0"]
build-backend = "uv_build"
```

!!! important

    The uv build backend follows the same [versioning policy](../reference/policies/versioning.md),
    setting an upper bound on the `uv_build` version ensures that the package continues to build in
    the future.

You can also create a new project that uses the uv build backend with `uv init`:

```shell
uv init --build-backend uv
```

`uv_build` is a separate package from uv, optimized for portability and small binary size. The `uv`
command includes a copy of the build backend, so when running `uv build`, the same version will be
used for the build backend as for the uv process. Other build frontends, such as `python -m build`,
will choose the latest compatible `uv_build` version.

## Modules

The default module name is the package name in lower case with dots and dashes replaced by
underscores, and the default module location is under the `src` directory, i.e., the build backend
expects to find `src/<package_name>/__init__.py`. These defaults can be changed with the
`module-name` and `module-root` setting. The example below expects a module in the project root with
`PIL/__init__.py` instead:

```toml
[tool.uv.build-backend]
module-name = "PIL"
module-root = ""
```

The build backend supports building stubs packages with a `-stubs` package or module name.

## Include and exclude configuration

To select which files to include in the source distribution, uv first adds the included files and
directories, then removes the excluded files and directories. This means that exclusions always take
precedence over inclusions.

When building the source distribution, the following files and directories are included:

- `pyproject.toml`
- The module under `tool.uv.build-backend.module-root`, by default
  `src/<module-name or project_name_with_underscores>/**`.
- `project.license-files` and `project.readme`.
- All directories under `tool.uv.build-backend.data`.
- All patterns from `tool.uv.build-backend.source-include`.

From these, `tool.uv.build-backend.source-exclude` and the default excludes are removed.

When building the wheel, the following files and directories are included:

- The module under `tool.uv.build-backend.module-root`, by default
  `src/<module-name or project_name_with_underscores>/**`.
- `project.license-files` and `project.readme`, as part of the project metadata.
- Each directory under `tool.uv.build-backend.data`, as data directories.

From these, `tool.uv.build-backend.source-exclude`, `tool.uv.build-backend.wheel-exclude` and the
default excludes are removed. The source dist excludes are applied to avoid source tree to wheel
source builds including more files than source tree to source distribution to wheel build.

There are no specific wheel includes. There must only be one top level module, and all data files
must either be under the module root or in the appropriate
[data directory](../reference/settings.md#build-backend_data). Most packages store small data in the
module root alongside the source code.

## Include and exclude syntax

Includes are anchored, which means that `pyproject.toml` includes only
`<project root>/pyproject.toml`. For example, `assets/**/sample.csv` includes all `sample.csv` files
in `<project root>/assets` or any child directory. To recursively include all files under a
directory, use a `/**` suffix, e.g. `src/**`.

!!! note

    For performance and reproducibility, avoid patterns without an anchor such as `**/sample.csv`.

Excludes are not anchored, which means that `__pycache__` excludes all directories named
`__pycache__` and its children anywhere. To anchor a directory, use a `/` prefix, e.g., `/dist` will
exclude only `<project root>/dist`.

All fields accepting patterns use the reduced portable glob syntax from
[PEP 639](https://peps.python.org/pep-0639/#add-license-FILES-key), with the addition that
characters can be escaped with a backslash.
