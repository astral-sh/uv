# Building distributions

To distribute your project to others (e.g., to upload it to an index like PyPI), you'll need to
build it into a distributable format.

Python projects are typically distributed as both source distributions (sdists) and binary
distributions (wheels). The former is typically a `.tar.gz` or `.zip` file containing the project's
source code along with some additional metadata, while the latter is a `.whl` file containing
pre-built artifacts that can be installed directly.

!!! important

    When using `uv build`, uv acts as a [build frontend](https://peps.python.org/pep-0517/#terminology-and-goals)
    and determines the Python version to use and invokes the build backend. The details of
    the builds, such as the included files and the distribution filenames, are determined by the build
    backend, as defined in [`[build-system]`](./config.md#build-systems). Information about build
    configuration can be found in the respective tool's documentation.

## Using `uv build`

`uv build` can be used to build both source distributions and binary distributions for your project.
By default, `uv build` will build the project in the current directory, and place the built
artifacts in a `dist/` subdirectory:

```console
$ uv build
$ ls dist/
example-0.1.0-py3-none-any.whl
example-0.1.0.tar.gz
```

You can build the project in a different directory by providing a path to `uv build`, e.g.,
`uv build path/to/project`.

`uv build` will first build a source distribution, and then build a binary distribution (wheel) from
that source distribution.

You can limit `uv build` to building a source distribution with `uv build --sdist`, a binary
distribution with `uv build --wheel`, or build both distributions from source with
`uv build --sdist --wheel`.

## Including a dependency lock

!!! warning "Preview feature"

    This feature is experimental and may change. Enable it with
    `--preview-features locked-tools`.

When a project uses `uv_build` and its dependencies use only PyPI sources, `uv build` includes a
`pylock.toml` in its source distribution and wheel. A subsequent `uv build` can carry the lock from
the source distribution into a wheel. The project itself is supplied by the wheel and is not listed
in `pylock.toml`. Workspace members may use a shared lock, but cannot have dependencies on other
workspace members. The lock does not include optional extras or dependency groups. Locks restricted
to specific environments are not supported.

Set `tool.uv.export-lock = true` to include a lock when using another build backend or package
index, or set it to `false` to disable export. `UV_EXPORT_LOCK` overrides this setting. Export
requires registry artifact URLs and hashes; Git and local dependencies are not supported. Automatic
export identifies PyPI sources by the index recorded in the lock, without verifying that PyPI lists
each artifact. It requires HTTPS artifact URLs without credentials, custom ports, query strings, or
fragments and skips export if URL-like values occur outside artifact and index URLs. The packaged
lock covers the project's base runtime dependencies; it does not include the full workspace
`uv.lock`. Source distributions can also contain `uv.lock` if the build backend includes it.
Automatic export also checks optional dependencies and dependency groups for non-PyPI sources. For
projects with dynamic dependency declarations, automatic export is skipped and explicit export
fails.

uv checks the workspace lock without network access before packaging it. When export is expected,
the build fails if the lock is missing, outdated, or cannot be verified. Run `uv lock` before
building. Automatic export uses the lock to determine eligible sources, so a stale lock that refers
to non-PyPI sources can cause export to be skipped. Set `tool.uv.export-lock = true` to require a
lock. When rebuilding from a source distribution, uv checks the packaged lock and the distribution
metadata before including the lock in the wheel. uv does not re-resolve the lock; if you edit it,
ensure it contains the intended dependencies. A source distribution without a lock builds a wheel
without one unless lock export is explicitly required.

The export is performed by `uv build`. Other frontends, including `python -m build`, do not include
the lock automatically.

## Build constraints

`uv build` accepts `--build-constraint`, which can be used to constrain the versions of any build
requirements during the build process. When coupled with `--require-hashes`, uv will enforce that
the requirements used to build the project match specific, known hashes, for reproducibility.

For example, given the following `constraints.txt`:

```text
setuptools==68.2.2 --hash=sha256:b454a35605876da60632df1a60f736524eb73cc47bbc9f3f1ef1b644de74fd2a
```

Running the following would build the project with the specified version of `setuptools`, and verify
that the downloaded `setuptools` distribution matches the specified hash:

```console
$ uv build --build-constraint constraints.txt --require-hashes
```

To require hashes for every build dependency, including transitive dependencies, use
`--require-build-hashes` instead.

### Project build dependency hashes

For example, to verify build dependencies during project resolution and installation, add hashes to
[`build-constraint-dependencies`](../../reference/settings.md#build-constraint-dependencies) in your
workspace's `pyproject.toml`:

```toml
[tool.uv]
build-constraint-dependencies = [
    { requirement = "setuptools==68.2.2", hashes = ["sha256:b454a35605876da60632df1a60f736524eb73cc47bbc9f3f1ef1b644de74fd2a"] },
    "wheel<1",
]
```

uv checks supplied hashes when it downloads pinned build dependencies. Constraints without hashes,
such as `wheel<1`, are also allowed. To require a hash for every build dependency, including
transitive dependencies, set
[`require-build-hashes = true`](../../reference/settings.md#require-build-hashes) under `[tool.uv]`
or pass `--require-build-hashes`.

You'll need to pin each build dependency to an exact version (or use a direct URL) and provide a
hash. Hashes can also come from URL fragments in `build-system.requires`, but not from requirements
returned by a build backend.

When build isolation is disabled, uv uses the installed build dependencies without checking their
hashes. uv also does not recheck already-installed packages or previously built wheels. The bundled
`uv_build` backend does not need a hash because it is part of the uv executable.

!!! note

    `--require-build-hashes` is experimental. Use `--preview-features build-dependency-hashes` to
    suppress the warning.

## Preventing publish to PyPI

If you have internal packages that you do not want to be published, you can mark them as private:

```toml
[project]
classifiers = ["Private :: Do Not Upload"]
```

This setting makes PyPI reject your uploaded package from publishing. It does not affect security or
privacy settings on alternative registries.

We also recommend only generating [per-project PyPI API tokens](https://pypi.org/help/#apitoken):
Without a PyPI token matching the project, it can't be accidentally published.
