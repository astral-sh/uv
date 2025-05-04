# Caching

## Dependency caching

uv uses aggressive caching to avoid re-downloading (and re-building) dependencies that have already
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
- To force uv to revalidate cached data for a specific dependency pass `--refresh-package` to any
  command (e.g., `uv sync --refresh-package flask` or `uv pip install --refresh-package flask ...`).
- To force uv to ignore existing installed versions, pass `--reinstall` to any installation command
  (e.g., `uv sync --reinstall` or `uv pip install --reinstall ...`).

As a special case, uv will always rebuild and reinstall any local directory dependencies passed
explicitly on the command-line (e.g., `uv pip install .`).

## Dynamic metadata

By default, uv will _only_ rebuild and reinstall local directory dependencies (e.g., editables) if
the `pyproject.toml`, `setup.py`, or `setup.cfg` file in the directory root has changed, or if a
`src` directory is added or removed. This is a heuristic and, in some cases, may lead to fewer
re-installs than desired.

To incorporate additional information into the cache key for a given package, you can add cache key
entries under [`tool.uv.cache-keys`](https://docs.astral.sh/uv/reference/settings/#cache-keys),
which covers both file paths and Git commit hashes. Setting
[`tool.uv.cache-keys`](https://docs.astral.sh/uv/reference/settings/#cache-keys) will replace
defaults, so any necessary files (like `pyproject.toml`) should still be included in the
user-defined cache keys.

For example, if a project specifies dependencies in `pyproject.toml` but uses
[`setuptools-scm`](https://pypi.org/project/setuptools-scm/) to manage its version, and should thus
be rebuilt whenever the commit hash or dependencies change, you can add the following to the
project's `pyproject.toml`:

```toml title="pyproject.toml"
[tool.uv]
cache-keys = [{ file = "pyproject.toml" }, { git = { commit = true } }]
```

If your dynamic metadata incorporates information from the set of Git tags, you can expand the cache
key to include the tags:

```toml title="pyproject.toml"
[tool.uv]
cache-keys = [{ file = "pyproject.toml" }, { git = { commit = true, tags = true } }]
```

Similarly, if a project reads from a `requirements.txt` to populate its dependencies, you can add
the following to the project's `pyproject.toml`:

```toml title="pyproject.toml"
[tool.uv]
cache-keys = [{ file = "pyproject.toml" }, { file = "requirements.txt" }]
```

Globs are supported for `file` keys, following the syntax of the
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

Similarly, if a project relies on an environment variable, you can add the following to the
project's `pyproject.toml` to invalidate the cache whenever the environment variable changes:

```toml title="pyproject.toml"
[tool.uv]
cache-keys = [{ file = "pyproject.toml" }, { env = "MY_ENV_VAR" }]
```

Finally, to invalidate a project whenever a specific directory (like `src`) is created or removed,
add the following to the project's `pyproject.toml`:

```toml title="pyproject.toml"
[tool.uv]
cache-keys = [{ file = "pyproject.toml" }, { dir = "src" }]
```

Note that the `dir` key will only track changes to the directory itself, and not arbitrary changes
within the directory.

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
version was increased from v12 to v13. Within a cache version, changes are guaranteed to be both
forwards- and backwards-compatible.

Since changes in the cache format are accompanied by changes in the cache version, multiple versions
of uv can safely read and write to the same cache directory. However, if the cache version changed
between a given pair of uv releases, then those releases may not be able to share the same
underlying cache entries.

For example, it's safe to use a single shared cache for uv 0.4.12 and uv 0.4.13, though the cache
itself may contain duplicate entries in the core metadata bucket due to the change in cache version.
