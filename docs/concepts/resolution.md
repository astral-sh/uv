# Resolution

Resolution is the process of taking a list of requirements and converting them to a list of package
versions that fulfill the requirements. Resolution requires recursively searching for compatible
versions of packages, ensuring that the requested requirements are fulfilled and that the
requirements of the requested packages are compatible.

## Dependencies

Most projects and packages have dependencies. Dependencies are other packages that are necessary in
order for the current package to work. A package defines its dependencies as _requirements_, roughly
a combination of a package name and acceptable versions. The dependencies defined by the current
project are called _direct dependencies_. The dependencies added by each dependency of the current
project are called _indirect_ or _transitive dependencies_.

!!! note

    See the [dependency specifiers
    page](https://packaging.python.org/en/latest/specifications/dependency-specifiers/)
    in the Python Packaging documentation for details about dependencies.

## Basic examples

To help demonstrate the resolution process, consider the following dependencies:

<!-- prettier-ignore -->
- The project depends on `foo` and `bar`.
- `foo` has one version, 1.0.0:
    - `foo 1.0.0` depends on `lib>=1.0.0`.
- `bar` has one version, 1.0.0:
    - `bar 1.0.0` depends on `lib>=2.0.0`.
- `lib` has two versions, 1.0.0 and 2.0.0. Both versions have no dependencies.

In this example, the resolver must find a set of package versions which satisfies the project
requirements. Since there is only one version of both `foo` and `bar`, those will be used. The
resolution must also include the transitive dependencies, so a version of `lib` must be chosen.
`foo 1.0.0` allows all available versions of `lib`, but `bar 1.0.0` requires `lib>=2.0.0` so
`lib 2.0.0` must be used.

In some resolutions, there may be more than one valid solution. Consider the following dependencies:

<!-- prettier-ignore -->
- The project depends on `foo` and `bar`.
- `foo` has two versions, 1.0.0 and 2.0.0:
    - `foo 1.0.0` has no dependencies.
    - `foo 2.0.0` depends on `lib==2.0.0`.
- `bar` has two versions, 1.0.0 and 2.0.0:
    - `bar 1.0.0` has no dependencies.
    - `bar 2.0.0` depends on `lib==1.0.0`
- `lib` has two versions, 1.0.0 and 2.0.0. Both versions have no dependencies.

In this example, some version of both `foo` and `bar` must be selected; however, determining which
version requires considering the dependencies of each version of `foo` and `bar`. `foo 2.0.0` and
`bar 2.0.0` cannot be installed together as they conflict on their required version of `lib`, so the
resolver must select either `foo 1.0.0` (along with `bar 2.0.0`) or `bar 1.0.0` (along with
`foo 1.0.0`). Both are valid solutions, and different resolution algorithms may yield either result.

## Platform markers

Markers allow attaching an expression to requirements that indicate when the dependency should be
used. For example `bar ; python_version < "3.9"` indicates that `bar` should only be installed on
Python 3.8 and earlier.

Markers are used to adjust a package's dependencies based on the current environment or platform.
For example, markers can be used to modify dependencies by operating system, CPU architecture,
Python version, Python implementation, and more.

!!! note

    See the [environment
    markers](https://packaging.python.org/en/latest/specifications/dependency-specifiers/#environment-markers)
    section in the Python Packaging documentation for more details about markers.

Markers are important for resolution because their values change the required dependencies.
Typically, Python package resolvers use the markers of the _current_ platform to determine which
dependencies to use since the package is often being _installed_ on the current platform. However,
for _locking_ dependencies this is problematic — the lockfile would only work for developers using
the same platform the lockfile was created on. To solve this problem, platform-independent, or
"universal" resolvers exist.

uv supports both [platform-specific](#platform-specific-resolution) and
[universal](#universal-resolution) resolution.

## Platform-specific resolution

By default, uv's pip interface, i.e., [`uv pip compile`](../pip/compile.md), produces a resolution
that is platform-specific, like `pip-tools`. There is no way to use platform-specific resolution in
the uv's project interface.

uv also supports resolving for specific, alternate platforms and Python versions with the
`--python-platform` and `--python-version` options. For example, if using Python 3.12 on macOS,
`uv pip compile --python-platform linux --python-version 3.10 requirements.in` can be used to
produce a resolution for Python 3.10 on Linux instead. Unlike universal resolution, during
platform-specific resolution, the provided `--python-version` is the exact python version to use,
not a lower bound.

!!! note

    Python's environment markers expose far more information about the current machine
    than can be expressed by a simple `--python-platform` argument. For example, the `platform_version` marker
    on macOS includes the time at which the kernel was built, which can (in theory) be encoded in
    package requirements. uv's resolver makes a best-effort attempt to generate a resolution that is
    compatible with any machine running on the target `--python-platform`, which should be sufficient for
    most use cases, but may lose fidelity for complex package and platform combinations.

## Universal resolution

uv's lockfile (`uv.lock`) is created with a universal resolution and is portable across platforms.
This ensures that dependencies are locked for everyone working on the project, regardless of
operating system, architecture, and Python version. The uv lockfile is created and modified by
[project](../concepts/projects/index.md) commands such as `uv lock`, `uv sync`, and `uv add`.

Universal resolution is also available in uv's pip interface, i.e.,
[`uv pip compile`](../pip/compile.md), with the `--universal` flag. The resulting requirements file
will contain markers to indicate which platform each dependency is relevant for.

During universal resolution, a package may be listed multiple times with different versions or URLs
if different versions are needed for different platforms — the markers determine which version will
be used. A universal resolution is often more constrained than a platform-specific resolution, since
we need to take the requirements for all markers into account.

During universal resolution, all required packages must be compatible with the _entire_ range of
`requires-python` declared in the `pyproject.toml`. For example, if a project's `requires-python` is
`>=3.8`, resolution will fail if all versions of given dependency require Python 3.9 or later, since
the dependency lacks a usable version for (e.g.) Python 3.8, the lower bound of the project's
supported range. In other words, the project's `requires-python` must be a subset of the
`requires-python` of all its dependencies.

When selecting the compatible version for a given dependency, uv will
([by default](#multi-version-resolution)) attempt to choose the latest compatible version for each
supported Python version. For example, if a project's `requires-python` is `>=3.8`, and the latest
version of a dependency requires Python 3.9 or later, while all prior versions supporting Python
3.8, the resolver will select the latest version for users running Python 3.9 or later, and previous
versions for users running Python 3.8.

When evaluating `requires-python` ranges for dependencies, uv only considers lower bounds and
ignores upper bounds entirely. For example, `>=3.8, <4` is treated as `>=3.8`. Respecting upper
bounds on `requires-python` often leads to formally correct but practically incorrect resolutions,
as, e.g., resolvers will backtrack to the first published version that omits the upper bound (see:
[`Requires-Python` upper limits](https://discuss.python.org/t/requires-python-upper-limits/12663)).

### Limited resolution environments

By default, the universal resolver attempts to solve for all platforms and Python versions.

If your project supports only a limited set of platforms or Python versions, you can constrain the
set of solved platforms via the `environments` setting, which accepts a list of
[PEP 508 environment markers](https://packaging.python.org/en/latest/specifications/dependency-specifiers/#environment-markers).
In other words, you can use the `environments` setting to _reduce_ the set of supported platforms.

For example, to constrain the lockfile to macOS and Linux, and avoid solving for Windows:

```toml title="pyproject.toml"
[tool.uv]
environments = [
    "sys_platform == 'darwin'",
    "sys_platform == 'linux'",
]
```

Or, to avoid solving for alternative Python implementations:

```toml title="pyproject.toml"
[tool.uv]
environments = [
    "implementation_name == 'cpython'"
]
```

Entries in the `environments` setting must be disjoint (i.e., they must not overlap). For example,
`sys_platform == 'darwin'` and `sys_platform == 'linux'` are disjoint, but
`sys_platform == 'darwin'` and `python_version >= '3.9'` are not, since both could be true at the
same time.

### Required environments

In the Python ecosystem, packages can be published as source distributions, built distributions
(wheels), or both; but to install a package, a built distribution is required. If a package lacks a
built distribution, or lacks a distribution for the current platform or Python version (built
distributions are often platform-specific), uv will attempt to build the package from source, then
install the resulting built distribution.

Some packages (like PyTorch) publish built distributions, but omit a source distribution. Such
packages are _only_ installable on platforms for which a built distribution is available. For
example, if a package publishes built distributions for Linux, but not macOS or Windows, then that
package will _only_ be installable on Linux.

Packages that lack source distributions cause problems for universal resolution, since there will
typically be at least one platform or Python version for which the package is not installable.

By default, uv requires each such package to include at least one wheel that is compatible with the
target Python version. The `required-environments` setting can be used to ensure that the resulting
resolution contains wheels for specific platforms, or fails if no such wheels are available. The
setting accepts a list of
[PEP 508 environment markers](https://packaging.python.org/en/latest/specifications/dependency-specifiers/#environment-markers).

While the `environments` setting _limits_ the set of environments that uv will consider when
resolving dependencies, `required-environments` _expands_ the set of platforms that uv _must_
support when resolving dependencies.

For example, `environments = ["sys_platform == 'darwin'"]` would limit uv to solving for macOS (and
ignoring Linux and Windows). On the other hand,
`required-environments = ["sys_platform == 'darwin'"]` would _require_ that any package without a
source distribution include a wheel for macOS in order to be installable (and would fail if no such
wheel is available).

In practice, `required-environments` can be useful for declaring explicit support for non-latest
platforms, since this often requires backtracking past the latest published versions of those
packages. For example, to guarantee that any built distribution-only packages includes support for
Intel macOS:

```toml title="pyproject.toml"
[tool.uv]
required-environments = [
    "sys_platform == 'darwin' and platform_machine == 'x86_64'"
]
```

## Dependency preferences

If resolution output file exists, i.e., a uv lockfile (`uv.lock`) or a requirements output file
(`requirements.txt`), uv will _prefer_ the dependency versions listed there. Similarly, if
installing a package into a virtual environment, uv will prefer the already installed version if
present. This means that locked or installed versions will not change unless an incompatible version
is requested or an upgrade is explicitly requested with `--upgrade`.

## Resolution strategy

By default, uv tries to use the latest version of each package. For example,
`uv pip install flask>=2.0.0` will install the latest version of Flask, e.g., 3.0.0. If
`flask>=2.0.0` is a dependency of the project, only `flask` 3.0.0 will be used. This is important,
for example, because running tests will not check that the project is actually compatible with its
stated lower bound of `flask` 2.0.0.

With `--resolution lowest`, uv will install the lowest possible version for all dependencies, both
direct and indirect (transitive). Alternatively, `--resolution lowest-direct` will use the lowest
compatible versions for all direct dependencies, while using the latest compatible versions for all
other dependencies. uv will always use the latest versions for build dependencies.

For example, given the following `requirements.in` file:

```python title="requirements.in"
flask>=2.0.0
```

Running `uv pip compile requirements.in` would produce the following `requirements.txt` file:

```python title="requirements.txt"
# This file was autogenerated by uv via the following command:
#    uv pip compile requirements.in
blinker==1.7.0
    # via flask
click==8.1.7
    # via flask
flask==3.0.0
itsdangerous==2.1.2
    # via flask
jinja2==3.1.2
    # via flask
markupsafe==2.1.3
    # via
    #   jinja2
    #   werkzeug
werkzeug==3.0.1
    # via flask
```

However, `uv pip compile --resolution lowest requirements.in` would instead produce:

```python title="requirements.in"
# This file was autogenerated by uv via the following command:
#    uv pip compile requirements.in --resolution lowest
click==7.1.2
    # via flask
flask==2.0.0
itsdangerous==2.0.0
    # via flask
jinja2==3.0.0
    # via flask
markupsafe==2.0.0
    # via jinja2
werkzeug==2.0.0
    # via flask
```

When publishing libraries, it is recommended to separately run tests with `--resolution lowest` or
`--resolution lowest-direct` in continuous integration to ensure compatibility with the declared
lower bounds.

## Pre-release handling

By default, uv will accept pre-release versions during dependency resolution in two cases:

1. If the package is a direct dependency, and its version specifiers include a pre-release specifier
   (e.g., `flask>=2.0.0rc1`).
1. If _all_ published versions of a package are pre-releases.

If dependency resolution fails due to a transitive pre-release, uv will prompt use of
`--prerelease allow` to allow pre-releases for all dependencies.

Alternatively, the transitive dependency can be added as a [constraint](#dependency-constraints) or
direct dependency (i.e. in `requirements.in` or `pyproject.toml`) with a pre-release version
specifier (e.g., `flask>=2.0.0rc1`) to opt-in to pre-release support for that specific dependency.

Pre-releases are
[notoriously difficult](https://pubgrub-rs-guide.netlify.app/limitations/prerelease_versions) to
model, and are a frequent source of bugs in other packaging tools. uv's pre-release handling is
_intentionally_ limited and requires user opt-in for pre-releases to ensure correctness.

For more details, see
[Pre-release compatibility](../pip/compatibility.md#pre-release-compatibility).

## Multi-version resolution

During universal resolution, a package may be listed multiple times with different versions or URLs
within the same lockfile, since different versions may be needed for different platforms or Python
versions.

The `--fork-strategy` setting can be used to control how uv trades off between (1) minimizing the
number of selected versions and (2) selecting the latest-possible version for each platform. The
former leads to greater consistency across platforms, while the latter leads to use of newer package
versions where possible.

By default (`--fork-strategy requires-python`), uv will optimize for selecting the latest version of
each package for each supported Python version, while minimizing the number of selected versions
across platforms.

For example, when resolving `numpy` with a Python requirement of `>=3.8`, uv would select the
following versions:

```txt
numpy==1.24.4 ; python_version == "3.8"
numpy==2.0.2 ; python_version == "3.9"
numpy==2.2.0 ; python_version >= "3.10"
```

This resolution reflects the fact that NumPy 2.2.0 and later require at least Python 3.10, while
earlier versions are compatible with Python 3.8 and 3.9.

Under `--fork-strategy fewest`, uv will instead minimize the number of selected versions for each
package, preferring older versions that are compatible with a wider range of supported Python
versions or platforms.

For example, when in the scenario above, uv would select `numpy==1.24.4` for all Python versions,
rather than upgrading to `numpy==2.0.2` for Python 3.9 and `numpy==2.2.0` for Python 3.10 and later.

## Dependency constraints

Like pip, uv supports constraint files (`--constraint constraints.txt`) which narrow the set of
acceptable versions for the given packages. Constraint files are similar to requirements files, but
being listed as a constraint alone will not cause a package to be included to the resolution.
Instead, constraints only take effect if a requested package is already pulled in as a direct or
transitive dependency. Constraints are useful for reducing the range of available versions for a
transitive dependency. They can also be used to keep a resolution in sync with some other set of
resolved versions, regardless of which packages are overlapping between the two.

## Dependency overrides

Dependency overrides allow bypassing unsuccessful or undesirable resolutions by overriding a
package's declared dependencies. Overrides are a useful last resort for cases in which you _know_
that a dependency is compatible with a certain version of a package, despite the metadata indicating
otherwise.

For example, if a transitive dependency declares the requirement `pydantic>=1.0,<2.0`, but _does_
work with `pydantic>=2.0`, the user can override the declared dependency by including
`pydantic>=1.0,<3` in the overrides, thereby allowing the resolver to choose a newer version of
`pydantic`.

Concretely, if `pydantic>=1.0,<3` is included as an override, uv will ignore all declared
requirements on `pydantic`, replacing them with the override. In the above example, the
`pydantic>=1.0,<2.0` requirement would be ignored completely, and would instead be replaced with
`pydantic>=1.0,<3`.

While constraints can only _reduce_ the set of acceptable versions for a package, overrides can
_expand_ the set of acceptable versions, providing an escape hatch for erroneous upper version
bounds. As with constraints, overrides do not add a dependency on the package and only take effect
if the package is requested in a direct or transitive dependency.

In a `pyproject.toml`, use `tool.uv.override-dependencies` to define a list of overrides. In the
pip-compatible interface, the `--override` option can be used to pass files with the same format as
constraints files.

If multiple overrides are provided for the same package, they must be differentiated with
[markers](#platform-markers). If a package has a dependency with a marker, it is replaced
unconditionally when using overrides — it does not matter if the marker evaluates to true or false.

## Dependency metadata

During resolution, uv needs to resolve the metadata for each package it encounters, in order to
determine its dependencies. This metadata is often available as a static file in the package index;
however, for packages that only provide source distributions, the metadata may not be available
upfront.

In such cases, uv has to build the package to determine its metadata (e.g., by invoking `setup.py`).
This can introduce a performance penalty during resolution. Further, it imposes the requirement that
the package can be built on all platforms, which may not be true.

For example, you may have a package that should only be built and installed on Linux, but doesn't
build successfully on macOS or Windows. While uv can construct a perfectly valid lockfile for this
scenario, doing so would require building the package, which would fail on non-Linux platforms.

The `tool.uv.dependency-metadata` table can be used to provide static metadata for such dependencies
upfront, thereby allowing uv to skip the build step and use the provided metadata instead.

For example, to provide metadata for `chumpy` upfront, include its `dependency-metadata` in the
`pyproject.toml`:

```toml
[[tool.uv.dependency-metadata]]
name = "chumpy"
version = "0.70"
requires-dist = ["numpy>=1.8.1", "scipy>=0.13.0", "six>=1.11.0"]
```

These declarations are intended for cases in which a package does _not_ declare static metadata
upfront, though they are also useful for packages that require disabling build isolation. In such
cases, it may be easier to declare the package metadata upfront, rather than creating a custom build
environment prior to resolving the package.

For example, you can declare the metadata for `flash-attn`, allowing uv to resolve without building
the package from source (which itself requires installing `torch`):

```toml
[project]
name = "project"
version = "0.1.0"
requires-python = ">=3.12"
dependencies = ["flash-attn"]

[tool.uv.sources]
flash-attn = { git = "https://github.com/Dao-AILab/flash-attention", tag = "v2.6.3" }

[[tool.uv.dependency-metadata]]
name = "flash-attn"
version = "2.6.3"
requires-dist = ["torch", "einops"]
```

Like dependency overrides, `tool.uv.dependency-metadata` can also be used for cases in which a
package's metadata is incorrect or incomplete, or when a package is not available in the package
index. While dependency overrides allow overriding the allowed versions of a package globally,
metadata overrides allow overriding the declared metadata of a _specific package_.

!!! note

    The `version` field in `tool.uv.dependency-metadata` is optional for registry-based
    dependencies (when omitted, uv will assume the metadata applies to all versions of the package),
    but _required_ for direct URL dependencies (like Git dependencies).

Entries in the `tool.uv.dependency-metadata` table follow the
[Metadata 2.3](https://packaging.python.org/en/latest/specifications/core-metadata/) specification,
though only `name`, `version`, `requires-dist`, `requires-python`, and `provides-extra` are read by
uv. The `version` field is also considered optional. If omitted, the metadata will be used for all
versions of the specified package.

## Lower bounds

By default, `uv add` adds lower bounds to dependencies and, when using uv to manage projects, uv
will warn if direct dependencies don't have lower bound.

Lower bounds are not critical in the "happy path", but they are important for cases where there are
dependency conflicts. For example, consider a project that requires two packages and those packages
have conflicting dependencies. The resolver needs to check all combinations of all versions within
the constraints for the two packages — if all of them conflict, an error is reported because the
dependencies are not satisfiable. If there are no lower bounds, the resolver can (and often will)
backtrack down to the oldest version of a package. This isn't only problematic because it's slow,
the old version of the package often fails to build, or the resolver can end up picking a version
that's old enough that it doesn't depend on the conflicting package, but also doesn't work with your
code.

Lower bounds are particularly critical when writing a library. It's important to declare the lowest
version for each dependency that your library works with, and to validate that the bounds are
correct — testing with
[`--resolution lowest` or `--resolution lowest-direct`](#resolution-strategy). Otherwise, a user may
receive an old, incompatible version of one of your library's dependencies and the library will fail
with an unexpected error.

## Reproducible resolutions

uv supports an `--exclude-newer` option to limit resolution to distributions published before a
specific date, allowing reproduction of installations regardless of new package releases. The date
may be specified as an [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339.html) timestamp (e.g.,
`2006-12-02T02:07:43Z`) or a local date in the same format (e.g., `2006-12-02`) in your system's
configured time zone.

Note the package index must support the `upload-time` field as specified in
[`PEP 700`](https://peps.python.org/pep-0700/). If the field is not present for a given
distribution, the distribution will be treated as unavailable. PyPI provides `upload-time` for all
packages.

To ensure reproducibility, messages for unsatisfiable resolutions will not mention that
distributions were excluded due to the `--exclude-newer` flag — newer distributions will be treated
as if they do not exist.

!!! note

    The `--exclude-newer` option is only applied to packages that are read from a registry (as opposed to, e.g., Git
    dependencies). Further, when using the `uv pip` interface, uv will not downgrade previously installed packages
    unless the `--reinstall` flag is provided, in which case uv will perform a new resolution.

## Source distribution

[PEP 625](https://peps.python.org/pep-0625/) specifies that packages must distribute source
distributions as gzip tarball (`.tar.gz`) archives. Prior to this specification, other archive
formats, which need to be supported for backward compatibility, were also allowed. uv supports
reading and extracting archives in the following formats:

- gzip tarball (`.tar.gz`, `.tgz`)
- bzip2 tarball (`.tar.bz2`, `.tbz`)
- xz tarball (`.tar.xz`, `.txz`)
- zstd tarball (`.tar.zst`)
- lzip tarball (`.tar.lz`)
- lzma tarball (`.tar.lzma`)
- zip (`.zip`)

## Learn more

For more details about the internals of the resolver, see the
[resolver reference](../reference/resolver-internals.md) documentation.

## Lockfile versioning

The `uv.lock` file uses a versioned schema. The schema version is included in the `version` field of
the lockfile.

Any given version of uv can read and write lockfiles with the same schema version, but will reject
lockfiles with a greater schema version. For example, if your uv version supports schema v1,
`uv lock` will error if it encounters an existing lockfile with schema v2.

uv versions that support schema v2 _may_ be able to read lockfiles with schema v1 if the schema
update was backwards-compatible. However, this is not guaranteed, and uv may exit with an error if
it encounters a lockfile with an outdated schema version.

The schema version is considered part of the public API, and so is only bumped in minor releases, as
a breaking change (see [Versioning](../reference/policies/versioning.md)). As such, all uv patch
versions within a given minor uv release are guaranteed to have full lockfile compatibility. In
other words, lockfiles may only be rejected across minor releases.
