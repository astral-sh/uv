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

## Next steps

To learn more about publishing packages, check out the
[PyPA guides](https://packaging.python.org/en/latest/guides/section-build-and-publish/) on building
and publishing.

Or, read on for more details about the concepts in uv.
