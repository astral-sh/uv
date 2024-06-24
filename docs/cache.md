# Caching

## Dependency caching

uv uses aggressive caching to avoid re-downloading (and re-building dependencies) that have
already been accessed in prior runs.

The specifics of uv's caching semantics vary based on the nature of the dependency:

- **For registry dependencies** (like those downloaded from PyPI), uv respects HTTP caching headers.
- **For direct URL dependencies**, uv respects HTTP caching headers, and also caches based on
  the URL itself.
- **For Git dependencies**, uv caches based on the fully-resolved Git commit hash. As such,
  `uv pip compile` will pin Git dependencies to a specific commit hash when writing the resolved
  dependency set.
- **For local dependencies**, uv caches based on the last-modified time of the source archive (i.e.,
  the local `.whl` or `.tar.gz` file). For directories, uv caches based on the last-modified time of
  the `pyproject.toml`, `setup.py`, or `setup.cfg` file.

It's safe to run multiple `uv` commands concurrently, even against the same virtual environment.
uv's cache is designed to be thread-safe and append-only, and thus robust to multiple concurrent
readers and writers. uv applies a file-based lock to the target virtual environment when installing,
to avoid concurrent modifications across processes.

Note that it's _not_ safe to modify the uv cache directly (e.g., `uv cache clean`) while other `uv`
commands are running, and _never_ safe to modify the cache directly (e.g., by removing a file or
directory).

If you're running into caching issues, uv includes a few escape hatches:

- To force uv to revalidate cached data for all dependencies, run `uv pip install --refresh ...`.
- To force uv to revalidate cached data for a specific dependency, run, e.g., `uv pip install --refresh-package flask ...`.
- To force uv to ignore existing installed versions, run `uv pip install --reinstall ...`.
- To clear the global cache entirely, run `uv cache clean`.
