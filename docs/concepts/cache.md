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

It's safe to run multiple uv commands concurrently, even against the same virtual environment. uv's
cache is designed to be thread-safe and append-only, and thus robust to multiple concurrent readers
and writers. uv applies a file-based lock to the target virtual environment when installing, to
avoid concurrent modifications across processes.

Note that it's _not_ safe to modify the uv cache (e.g., `uv cache clean`) while other uv commands
are running, and _never_ safe to modify the cache directly (e.g., by removing a file or directory).

If you're running into caching issues, uv includes a few escape hatches:

- To force uv to revalidate cached data for all dependencies, run `uv pip install --refresh ...`.
- To force uv to revalidate cached data for a specific dependency, run, e.g.,
  `uv pip install --refresh-package flask ...`.
- To force uv to ignore existing installed versions, run `uv pip install --reinstall ...`.

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
pre-built wheels from the cache but retains any wheels that were built from source. We recommend
running `uv cache prune --ci` at the end of your continuous integration job to ensure maximum cache
efficiency. For an example, see the
[GitHub integration guide](../guides/integration/github.md#caching).
