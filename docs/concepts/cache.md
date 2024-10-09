# Caching

## Dependency caching

uv uses aggressive caching to avoid re-downloading (and re-building dependencies) that have already
been accessed in prior runs.

The specifics of uv's caching semantics vary based on the nature of the dependency:

- **For registry dependencies** (like those downloaded from PyPI), uv respects HTTP caching headers.
- **For direct URL dependencies**, uv respects HTTP caching headers, and also caches based on the
  URL itself.
- **For Git dependencies**, uv caches based on the fully-resolved Git commit hash. As such,
  `uv pip compile` will pin Git dependencies to a specific commit hash when writing the resolved
  dependency set.
- **For local dependencies**, uv caches based on the last-modified time of the source archive (i.e.,
  the local `.whl` or `.tar.gz` file). For directories, uv caches based on the last-modified time of
  the `pyproject.toml`, `setup.py`, or `setup.cfg` file.

If you're running into caching issues, uv includes a few escape hatches:

- To force uv to revalidate cached data for all dependencies, pass `--refresh` to any command (e.g.,
  `uv sync --refresh` or `uv pip install --refresh ...`).
- To force uv to revalidate cached data for a specific dependency pass `--refresh-dependency` to any
  command (e.g., `uv sync --refresh-package flask` or `uv pip install --refresh-package flask ...`).
- To force uv to ignore existing installed versions, pass `--reinstall` to any installation command
  (e.g., `uv sync --reinstall` or `uv pip install --reinstall ...`).

## Dynamic metadata

By default, uv will _only_ rebuild and reinstall local directory dependencies (e.g., editables) if
the `pyproject.toml`, `setup.py`, or `setup.cfg` file in the directory root has changed. This is a
heuristic and, in some cases, may lead to fewer re-installs than desired.

To incorporate other information into the cache key for a given package, you can add cache key
entries under `tool.uv.cache-keys`, which can include both file paths and Git commit hashes.

For example, if a project uses [`setuptools-scm`](https://pypi.org/project/setuptools-scm/), and
should be rebuilt whenever the commit hash changes, you can add the following to the project's
`pyproject.toml`:

```toml title="pyproject.toml"
[tool.uv]
cache-keys = [{ git = true }]
```

Similarly, if a project reads from a `requirements.txt` to populate its dependencies, you can add
the following to the project's `pyproject.toml`:

```toml title="pyproject.toml"
[tool.uv]
cache-keys = [{ file = "requirements.txt" }]
```

Globs are supported, following the syntax of the
[`glob`](https://docs.rs/glob/0.3.1/glob/struct.Pattern.html) crate. For example, to invalidate the
cache whenever a `.toml` file in the project directory or any of its subdirectories is modified, use
the following:

```toml title="pyproject.toml"
[tool.uv]
cache-keys = [{ file = "**/*.toml" }]
```

!!! note

    The use of globs can be expensive, as uv may need to walk the filesystem to determine whether any files have changed.
    This may, in turn, requiring traversal of large or deeply nested directories.

As an escape hatch, if a project uses `dynamic` metadata that isn't covered by `tool.uv.cache-keys`,
you can instruct uv to _always_ rebuild and reinstall it by adding the project to the
`tool.uv.reinstall-package` list:

```toml title="pyproject.toml"
[tool.uv]
reinstall-package = ["my-package"]
```

This will force uv to rebuild and reinstall `my-package` on every run, regardless of whether the
package's `pyproject.toml`, `setup.py`, or `setup.cfg` file has changed.

## Cache safety

It's safe to run multiple uv commands concurrently, even against the same virtual environment. uv's
cache is designed to be thread-safe and append-only, and thus robust to multiple concurrent readers
and writers. uv applies a file-based lock to the target virtual environment when installing, to
avoid concurrent modifications across processes.

Note that it's _not_ safe to modify the uv cache (e.g., `uv cache clean`) while other uv commands
are running, and _never_ safe to modify the cache directly (e.g., by removing a file or directory).

## Clearing the cache

uv provides a few different mechanisms for removing entries from the cache:

- `uv cache clean` removes _all_ cache entries from the cache directory, clearing it out entirely.
- `uv cache clean ruff` removes all cache entries for the `ruff` package, useful for invalidating
  the cache for a single or finite set of packages.
- `uv cache prune` removes all _unused_ cache entries. For example, the cache directory may contain
  entries created in previous uv versions that are no longer necessary and can be safely removed.
  `uv cache prune` is safe to run periodically, to keep the cache directory clean.

## Caching in continuous integration

It's common to cache package installation artifacts in continuous integration environments (like
GitHub Actions or GitLab CI) to speed up subsequent runs.

By default, uv caches both the wheels that it builds from source and the pre-built wheels that it
downloads directly, to enable high-performance package installation.

However, in continuous integration environments, persisting pre-built wheels may be undesirable.
With uv, it turns out that it's often faster to _omit_ pre-built wheels from the cache (and instead
re-download them from the registry on each run). On the other hand, caching wheels that are built
from source tends to be worthwhile, since the wheel building process can be expensive, especially
for extension modules.

To support this caching strategy, uv provides a `uv cache prune --ci` command, which removes all
pre-built wheels and unzipped source distributions from the cache, but retains any wheels that were
built from source. We recommend running `uv cache prune --ci` at the end of your continuous
integration job to ensure maximum cache efficiency. For an example, see the
[GitHub integration guide](../guides/integration/github.md#caching).

## Cache directory

uv determines the cache directory according to, in order:

1. A temporary cache directory, if `--no-cache` was requested.
2. The specific cache directory specified via `--cache-dir`, `UV_CACHE_DIR`, or
   [`tool.uv.cache-dir`](../reference/settings.md#cache-dir).
3. A system-appropriate cache directory, e.g., `$XDG_CACHE_HOME/uv` or `$HOME/.cache/uv` on Unix and
   `%LOCALAPPDATA%\uv\cache` on Windows

!!! note

    uv _always_ requires a cache directory. When `--no-cache` is requested, uv will still use
    a temporary cache for sharing data within that single invocation.

    In most cases, `--refresh` should be used instead of `--no-cache` — as it will update the cache
    for subsequent operations but not read from the cache.

It is important for performance for the cache directory to be located on the same file system as the
Python environment uv is operating on. Otherwise, uv will not be able to link files from the cache
into the environment and will instead need to fallback to slow copy operations.

## Cache versioning

The uv cache is composed of a number of buckets (e.g., a bucket for wheels, a bucket for source
distributions, a bucket for Git repositories, and so on). Each bucket is versioned, such that if a
release contains a breaking change to the cache format, uv will not attempt to read from or write to
an incompatible cache bucket.

For example, uv 0.4.13 included a breaking change to the core metadata bucket. As such, the bucket
version was increased from v12 to v13.

Within a cache version, changes are guaranteed to be forwards-compatible, but _not_
backwards-compatible.

For example, uv 0.4.8 can read cache entries written by uv 0.4.7, but uv 0.4.7 cannot read cache
entries written by uv 0.4.8. As a result, it's safe to share a cache directory across multiple uv
versions, as long as those versions are strictly increasing over time, as is common in production
and development environments.

If you intend to use multiple uv versions on an ongoing basis, we recommend using separate caches
for each version, as (e.g.) a cache populated by uv 0.4.8 may not be usable by uv 0.4.7, despite the
cache _versions_ remaining unchanged between the releases.
