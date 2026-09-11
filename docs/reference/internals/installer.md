# Installer internals

!!! tip

    This document focuses on uv's internal package installer: the path that turns a resolution or
    lockfile into an updated Python environment. For the standalone installer that installs the
    `uv` binary, see [the uv installer](../installer.md). For user-facing package installation
    behavior, see the [`uv pip`](../../pip/index.md) and
    [project](../../concepts/projects/index.md) documentation.

## Installer

The resolver decides which distributions should be present in an environment, but it does not write
files into that environment. Installation starts after resolution, when uv has a concrete set of
resolved distributions and a target Python environment. The shared installation path is used by
`uv pip install`, `uv pip sync`, and project environment updates such as `uv sync`.

At a high level, uv installs in the following steps:

- Index the target environment's `site-packages`.
- Build an installation plan by comparing the resolved distributions to the installed distributions
  and the wheel cache.
- If requested, report the plan as a dry run without changing the environment.
- Split the plan into build-isolated and non-build-isolated phases when needed.
- Prepare missing distributions by running download, build, and unpack work concurrently across
  distributions, then publishing the results into the cache.
- Uninstall distributions that will be replaced or removed.
- Link prepared and cached wheels into the environment.
- Optionally bytecode-compile the final `site-packages` tree.

The installer keeps three responsibilities separate. Planning decides what can stay installed, what
must be removed, and what must be prepared. Preparation obtains missing wheels and publishes them as
unpacked cache entries. Wheel installation and uninstallation are the only steps that mutate
`site-packages`. This separation lets pip-style and project-style commands share the same core
operation while still supporting dry runs, exact syncs, and different reinstall strategies.

## Site packages

Before planning, uv indexes the target environment. It reads every `site-packages` directory
reported by the interpreter and records installed `.dist-info`, `.egg-info`, and legacy editable
entries. Installed distributions are indexed by package name, and editable URL distributions are
also indexed by URL so unnamed URL requirements can be matched back to their installed package name.

The index is deliberately tolerant of broken or surprising environments. Python cannot import two
versions of the same distribution under one name, but environments can still contain duplicate
metadata directories. uv preserves that information, and the planner treats duplicate installed
versions as reinstall candidates rather than assuming one of them is authoritative. If a
`site-packages` directory does not exist, uv treats it as empty.

The same index is also used as a fast path for project operations. When an environment is allowed to
be merely sufficient, and there are no reinstall, upgrade, or source-tree changes, uv can check
whether the installed environment recursively satisfies the requested requirements before invoking
the resolver. This check reads installed metadata, follows transitive dependencies, evaluates
markers, and compares installed distributions with the same satisfaction logic used by the planner.

## Planning

Planning turns the resolved distributions and the installed environment into four practical
outcomes:

- Use an already-installed distribution.
- Install from an unpacked wheel that is already in uv's cache.
- Prepare a missing distribution by downloading or building it.
- Remove or reinstall an installed distribution.

The planner's first job is to avoid unnecessary work. For every resolved distribution, uv looks for
installed distributions with the same name and checks whether the installed distribution satisfies
the requested source, version, freshness, build settings, and platform tags. If it satisfies the
request, uv leaves it installed and moves on. Otherwise, the installed distribution becomes a
reinstall candidate.

The satisfaction check can be stricter for lockfile-driven project syncs than for pip-style install
operations. In the permissive strategy used by `uv pip` and some environment update paths, an
installed package can satisfy an implicit registry requirement even if it originally came from a
path or URL, as long as the version and other compatibility checks match. In the strict strategy
used by the main `uv sync` lockfile path, the installed source type must match the requirement
source so that the environment reflects the declared project state.

If a distribution is not already installed, uv checks the cache before scheduling remote work.
Registry wheels and source distributions are matched against the resolved index URL, filename,
version, required hashes, build options, build settings, extra build dependencies, extra build
variables, and the target tags. Direct URL and local path wheels are checked by their exact wheel
filename; local paths also carry freshness metadata so a changed source archive or directory can be
rebuilt. Git, path, directory, and URL source distributions are matched against cached wheels that
were built for the same source.

Any installed distribution left over after the resolved distributions have been processed is
extraneous. Exact sync operations remove these distributions. Sufficient install operations ignore
them. Seed packages such as `pip`, `setuptools`, `wheel`, and `uv` are preserved when appropriate so
that uv does not accidentally undo the seed state of an environment it did not create.

## Build isolation phases

After planning, uv may split the plan into two execution phases. Packages that can be built with
build isolation run first. Packages configured with build isolation disabled run second, with the
label "without build isolation" in the prepare summary. This allows packages such as `flash-attn` to
build against dependencies that were installed earlier in the same sync.

This split is a heuristic, not a general build-order solver. It works best when the build
dependencies for non-isolated builds are also ordinary project dependencies. If two packages both
need build isolation disabled and one must be built before the other, uv does not infer that order
from arbitrary build backend behavior.

The split has a subtle uninstall rule. If a second phase exists, uv delays reinstalling and removing
packages for that phase until the phase itself runs. This avoids uninstalling packages that may be
needed to build the non-isolated package. Extraneous packages are also kept until the second phase
when that phase exists, because an extraneous package can still be an undeclared build dependency
for a non-isolated build.

## Preparation

Preparation turns every distribution that still needs work into an unpacked wheel in uv's cache with
enough metadata to install it later. uv starts larger remote distributions first, then creates an
asynchronous preparation task for each distribution. A task owns the complete path for one cache
entry: obtain a wheel by download or source build, validate the wheel, unpack it into a temporary
cache location, and publish the resulting entry.

Those tasks run at the same time, but there is no global barrier between "download or build" and
"unpack." As soon as one task has a wheel to read, it can start unpacking and publishing that wheel
while other tasks are still downloading wheels or building source distributions. Work is concurrent
across distributions and sequential within a single distribution's cache entry.

In-flight tracking uses the same end-to-end unit of work. If two requirements converge on the same
cache entry, the first task owns the download, build, and unpack work while the second waits for that
result. When the result is ready, uv checks that the prepared wheel still has the expected name and
version.

The download concurrency limit applies to network work, not the entire preparation task. Unpacking
is part of each distribution's preparation task.

For built distributions, uv validates `--no-binary` before using a wheel. For source distributions,
uv validates `--no-build` before building, with a special case that still permits editable source
distributions to be built when needed. After the wheel is obtained, uv verifies the required hash
policy before returning it to the installer.

For remote wheels, uv usually streams the HTTP response directly through the wheel extractor. The
same read path can report progress, compute required hashes, and unzip the wheel into a temporary
cache directory as bytes arrive. Source distribution downloads use the same idea: the response body
is hashed and unpacked into the source cache while it streams. If streaming wheel extraction is not
supported by the archive, or if the streaming body fails in a way that should be retried, uv falls
back to downloading the wheel to a temporary file and then unpacking that file.

Prepared wheels are cached only after they are unpacked and validated. As part of that process, uv
checks the wheel's `RECORD` against the actual files extracted from the archive and rewrites the
`RECORD` if needed. This ensures that every unpacked wheel in the cache has install metadata that
matches the files uv will later link into an environment, and prevents malformed `RECORD` data from
causing unsafe uninstall behavior.

## Uninstallation

Before installing replacements, uv uninstalls any distributions selected for reinstall and any
extraneous distributions selected for removal. Wheel uninstallation reads the installed
`.dist-info/RECORD` and removes the recorded files. It also removes empty parent directories and
`__pycache__` directories, even though bytecode files are not generally listed in `RECORD`.

The uninstaller rejects `RECORD` entries that escape the target installation scheme, such as paths
that traverse outside the environment. For virtual environments, the valid scheme includes the venv
root because wheel `.data/data` entries can write there. For system or custom schemes, uv checks
against the interpreter's scheme paths. Invalid entries are skipped with a warning instead of being
removed. This protects users from malformed or malicious wheel metadata during upgrades and syncs.

uv can also uninstall legacy `.egg-info` directories and legacy editable `.egg-link` installs. For
`.egg-info` directories, uv follows setuptools metadata such as `top_level.txt` and
`namespace_packages.txt`, while ignoring empty entries so a malformed file cannot resolve to the
entire `site-packages` directory. Distutils-installed distributions that only have an `.egg-info`
file do not include enough metadata to uninstall safely, so uv reports an error instead of guessing.

## Wheel installation

Installing a prepared wheel follows the wheel installation specification. uv reads the wheel's
`METADATA` to validate the package name and version against the resolved filename, unless the
filename check is explicitly disabled for malformed wheels. It reads the `WHEEL` file to determine
whether the wheel is purelib or platlib, then links the unpacked wheel tree into the matching
`site-packages` directory.

uv supports several link modes. The default is copy-on-write clone on macOS and Linux, where that
mode is most likely to be available, and hardlink on other platforms. Users can also request copy,
hardlink, or symlink mode. All modes can fall back to copying when the requested link operation is
not supported, for example when hardlinking across filesystems. Since copying is not atomic, uv uses
directory-level copy locks during wheel installation so concurrent installs cannot corrupt the same
destination directory. The `RECORD` file is always copied as a mutable file, because uv edits it
during installation.

After linking package files, uv creates console and GUI entry points, moves any `.data`
subdirectories to their scheme destinations, writes installer metadata, and writes a sorted `RECORD`
for the installed distribution. Installer metadata can include `REQUESTED`, `INSTALLER`,
`direct_url.json`, `uv_cache.json`, and `uv_build.json`. The uv-specific cache and build metadata
allow later installs to detect stale local paths and wheels built with outdated config settings,
extra build dependencies, or extra build variables. These metadata files can be disabled for
reproducible distribution packaging with `UV_NO_INSTALLER_METADATA`.

The installer can also detect some package conflicts while multiple wheels are being installed in
one operation. It tracks top-level paths written by each wheel and, when the preview feature is
enabled, warns if two packages provide overlapping files with different sizes. The check recurses
only into shared directories and intentionally uses file size rather than reading file contents, so
it catches common broken-package cases without turning the installer into an expensive filesystem
comparison.

## Bytecode compilation

If bytecode compilation is enabled, uv runs it after all install phases have completed. This is a
separate pass over the entire final `site-packages` tree, not just the packages installed by the
current operation. Running after installation avoids concurrent writes to `.pyc` files during wheel
installation and ensures previously installed packages are compiled too.

The compiler uses a worker pool of Python subprocesses. uv walks `site-packages`, sends `.py` files
through a bounded queue, and each worker calls Python's `compileall.compile_file`, matching pip's
underlying behavior. Compilation errors from individual files are muted like pip, but uv still
guards the worker protocol against broken interpreters and hangs. The per-file timeout defaults to
60 seconds and can be configured with `UV_COMPILE_BYTECODE_TIMEOUT`; setting it to `0` disables the
timeout.

uv does not add generated `.pyc` files to installed `RECORD` files. Instead, uninstallation removes
`__pycache__` directories when cleaning up package directories, following the expectation that
uninstallers remove bytecode even when it is not recorded.
