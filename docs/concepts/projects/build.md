# Building distributions

To distribute your project to others (e.g., to upload it to an index like PyPI), you'll need to
build it into a distributable format.

Python projects are typically distributed as both source distributions (sdists) and binary
distributions (wheels). The former is typically a `.tar.gz` or `.zip` file containing the project's
source code along with some additional metadata, while the latter is a `.whl` file containing
pre-built artifacts that can be installed directly.

!!! important

    When using `uv build`, uv acts as a [build frontend](https://peps.python.org/pep-0517/#terminology-and-goals)
    and invokes the build backend defined in [`[build-system]`](./config.md#build-systems). The
    backend determines the distribution filenames and normally determines which files are included.
    uv may also include a [dependency lock](#including-a-dependency-lock) when the `locked-tools`
    preview feature is enabled. Information about build configuration can be found in the respective
    tool's documentation.

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

When a project uses `uv_build` and its dependencies use only PyPI sources, `uv build` automatically
exports its base runtime dependencies from `uv.lock` to a `pylock.toml` in the source distribution
and wheel. The project itself, optional extras, and dependency groups are not included in the
packaged lock. A workspace member can use the shared `uv.lock`, but cannot depend on another
workspace member.

Set `tool.uv.export-lock = true` to require a packaged lock with any build backend or package index,
or set it to `false` to disable export. `UV_EXPORT_LOCK` overrides this setting. Explicit export
also supports direct archives, Git dependencies, and local paths, although these sources cannot be
used for [locked tool installation](../tools.md#using-a-packaged-lock). Distribution files must have
hashes. Locks restricted to specific environments are not supported. For projects with dynamic
dependency declarations, automatic export is skipped and explicit export fails.

Automatic export identifies PyPI sources by the index recorded in the local lock and trusts its
artifact URLs; it does not query PyPI to verify them. It also checks optional dependencies and
dependency groups for non-PyPI sources, even though they are not included in the packaged lock.

Run `uv lock` before building. When export applies, the build fails if `uv.lock` is missing, stale,
or unusable. Automatic export uses the lock to determine eligible sources, so a stale lock that
refers to non-PyPI sources can cause export to be skipped. Explicitly enabling export requires a
lock instead. When building a wheel from a source distribution, uv can carry its packaged lock into
the wheel after checking the lock and distribution metadata. uv does not re-resolve the lock. A
source distribution without a lock builds a wheel without one unless export is explicitly required.

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
