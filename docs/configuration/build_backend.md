# The uv build backend

!!! note

    The uv build backend is currently in preview and may change in any future release.

    By default, uv currently uses the hatchling build backend.

A build backend transforms a source directory into a source distribution or a wheel. While uv
supports all build backends (PEP 517), it ships with the `uv_build` backend that integrates tightly
with uv.

The uv build backend currently only supports Python code and only builds universal wheels. An
alternative backend is required if you want to create a
[library with extension modules](../concepts/projects/init.md#projects-with-extension-modules).

To use the uv build backend, configure it in `pyproject.toml`:

```toml
[build-system]
requires = ["uv_build>=0.6.13,<0.7"]
build-backend = "uv_build"
```

You can also use `uv init` to generate a new project that uses the uv build backend:

```shell
uv init --build-backend uv
```

`uv_build` is a separate package from uv, optimized for a small size and high portability. `uv`
includes a copy of the build backend, so when running `uv build`, the same version will be used for
the build backend as for the uv process. Other build frontends, such as `python -m build`, will
choose the latest compatible `uv_build` version.

## Include and exclude configuration

To select which files to include in the source distribution, we first add the included files and
directories, then remove the excluded files and directories.

When building the source distribution, the following files and directories are included:

- `pyproject.toml`
- The module under `tool.uv.build-backend.module-root`, by default
  `src/<module-name or project_name_with_underscores>/**`.
- `project.license-files` and `project.readme`.
- All directories under `tool.uv.build-backend.data`.
- All patterns from `tool.uv.build-backend.source-include`.

From these, we remove the `tool.uv.build-backend.source-exclude` and the default excludes.

When building the wheel, the following files and directories are included:

- The module under `tool.uv.build-backend.module-root`, by default
  `src/<module-name or project_name_with_underscores>/**`.
- `project.license-files` and `project.readme`, as part of the project metadata.
- Each directory under `tool.uv.build-backend.data`, as data directories.

From these, we remove the `tool.uv.build-backend.source-exclude`,
`tool.uv.build-backend.wheel-exclude` and default excludes. The source dist excludes are applied to
avoid source tree to wheel source builds including more files than source tree to source
distribution to wheel build.

There are no specific wheel includes. There must only be one top level module, and all data files
must either be under the module root or in the appropriate
[data directory](../reference/settings.md#build-backend_data). Most packages store small data in the
module root alongside the source code.

## Include and exclude syntax

Includes are anchored, which means that `pyproject.toml` includes only
`<project root>/pyproject.toml`. For example, `assets/**/sample.csv` includes all `sample.csv` files
in `<project root>/assets` or any child directory. To recursively include all files under a
directory, use a `/**` suffix, e.g. `src/**`. For performance and reproducibility, avoid patterns
without an anchor such as `**/sample.csv`.

Excludes are not anchored, which means that `__pycache__` excludes all directories named
`__pycache__` and its children anywhere. To anchor a directory, use a `/` prefix, e.g., `/dist` will
exclude only `<project root>/dist`.

All fields accepting patterns use the reduced portable glob syntax from
[PEP 639](https://peps.python.org/pep-0639/#add-license-FILES-key).
