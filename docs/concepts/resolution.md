# Resolution

Dependency resolution is the process of taking your requirements and converting them to a list of
package versions that fulfil your requirements and the requirements of all included packages.

## Overview

Imagine you have the following dependency tree:

-   Your project depends on `foo>=1,<3` and `bar>=1,<3`.
-   `foo` has two versions, 1.0.0 and 2.0.0. `foo` 2.0.0 depends on `lib==2.0.0`, `foo` 1.0.0 has no
    dependencies.
-   `bar` has two versions, 1.0.0 and 2.0.0. `bar` 2.0.0 depends on `lib==1.0.0`, `bar` 1.0.0 has no
    dependencies.
-   `lib` has two versions, 1.0.0 and 2.0.0. Both versions have no dependencies.

We can't install both `foo` 2.0.0 and `bar` 2.0.0 because they conflict on the version of `lib`, so
the resolver will pick either `foo` 1.0.0 or `bar` 1.0.0. Both are valid solutions, at the resolvers
choice.

## Platform-specific and universal resolution

uv supports two modes of resolution: Platform-specific and universal (platform-independent).

Like `pip` and `pip-tools`, `uv pip compile` produces a resolution that's only known to be
compatible with the current operating system, architecture, Python version and Python interpreter.
`uv pip compile --universal` and the [project](../guides/projects.md) interface on the other hand
will solve to a host-agnostic universal resolution that can be used across platforms.

For universal resolution, you need to configure the minimum required python version. For
`uv pip compile --universal`, you can pass `--python-version`, otherwise the current Python version
will be treated as a lower bound. For example, `--universal --python-version 3.9` writes a universal
resolution for Python 3.9 and later. Project commands such as `uv sync` or `uv lock` read
`project.requires-python` from your `pyproject.toml`.

Setting the minimum Python version is important because all package versions we select have to be
compatible with the python range. For example, a universal resolution of `numpy<2` with
`--python-version 3.8` resolves to `numpy==1.24.4`, while `--python-version 3.9` resolves to
`numpy==1.26.4`, as `numpy` releases after 1.26.4 require at least Python 3.9. Note that we only
consider the lower bound of any Python requirement.

In platform-specific mode, the `uv pip` interface also supports resolving for specific alternate
platforms and Python versions with `--python-platform` and `--python-version`. For example, if
you're running Python 3.12 on macOS, but want to resolve for Linux with Python 3.10, you can run
`uv pip compile --python-platform linux --python-version 3.10 requirements.in` to produce a
`manylinux2014`-compatible resolution. In this mode, `--python-version` is the exact python version
to use, not a lower bound.

!!! note

    Python's environment markers expose far more information about the current machine
    than can be expressed by a simple `--python-platform` argument. For example, the `platform_version` marker
    on macOS includes the time at which the kernel was built, which can (in theory) be encoded in
    package requirements. uv's resolver makes a best-effort attempt to generate a resolution that is
    compatible with any machine running on the target `--python-platform`, which should be sufficient for
    most use cases, but may lose fidelity for complex package and platform combinations.

In universal mode, a package may be listed multiple times with different versions or URLs. In this
case, uv determined that we need different versions to be compatible different platforms, and the
markers decides on which platform we use which version. A universal resolution is often more
constrained than a platform-specific resolution, since we need to take the requirements for all
markers into account.

If an output file is used with `uv pip` or `uv.lock` exist with the project commands, we try to
resolve to the versions present there, considering them preferences in the resolution. The same
applies to version already installed to the active virtual environments. You can override this with
`--upgrade`.

## Resolution strategy

By default, uv tries to use the latest version of each package. For example,
`uv pip install flask>=2.0.0` will install the latest version of Flask (at time of writing:
`3.0.0`). If you have `flask>=2.0.0` as a dependency of your library, you will only test `flask`
3.0.0 this way, but not if you are actually still compatible with `flask` 2.0.0.

With `--resolution lowest`, uv will install the lowest possible version for all dependencies, both
direct and indirect (transitive). Alternatively, `--resolution lowest-direct` will opt for the
lowest compatible versions for all direct dependencies, while using the latest compatible versions
for all other dependencies. uv will always use the latest versions for build dependencies.

For libraries, we recommend separately running tests with `--resolution lowest` or
`--resolution lowest-direct` in continuous integration to ensure compatibility with the declared
lower bounds.

As an example, given the following `requirements.in` file:

```text title="requirements.in"
flask>=2.0.0
```

Running `uv pip compile requirements.in` would produce the following `requirements.txt` file:

```text title="requirements.txt"
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

```text title="requirements.in"
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

## Pre-release handling

By default, uv will accept pre-release versions during dependency resolution in two cases:

1. If the package is a direct dependency, and its version specifiers include a pre-release specifier
   (e.g., `flask>=2.0.0rc1`).
1. If _all_ published versions of a package are pre-releases.

If dependency resolution fails due to a transitive pre-release, uv will prompt the user to re-run
with `--prerelease allow`, to allow pre-releases for all dependencies.

Alternatively, you can add the transitive dependency to your `requirements.in` file with a
pre-release specifier (e.g., `flask>=2.0.0rc1`) to opt in to pre-release support for that specific
dependency.

Pre-releases are
[notoriously difficult](https://pubgrub-rs-guide.netlify.app/limitations/prerelease_versions) to
model, and are a frequent source of bugs in other packaging tools. uv's pre-release handling is
_intentionally_ limited and _intentionally_ requires user opt-in for pre-releases, to ensure
correctness.

For more, see [Pre-release compatibility](../pip/compatibility.md#pre-release-compatibility).

## Constraints

Like `pip`, uv supports constraints files (`--constraint constraints.txt`), which allows users to
narrow the set of acceptable versions for a given package. A constraint files is like a regular
requirements files, but it doesn't add packages, it only constrains their version range when they
are depended on by a regular requirement.

## Overrides

Sometimes, the requirements in one of your (transitive) dependencies are too strict, and you want to
install a version of a package that you know to work, but wouldn't be allowed regularly. Overrides
allow you to lie to the resolver, replacing all other requirements for that package with the
override. They break the usual rules of version resolving and should only be used as last resort
measure.

For example, if a transitive dependency declares `pydantic>=1.0,<2.0`, but the user knows that the
package is compatible with `pydantic>=2.0`, the user can override the declared dependency with
`pydantic>=2.0,<3` to allow the resolver to continue.

Overrides are passed to `uv pip` as `--override` with an overrides file with the same syntax as
requirements or constraints files. In `pyproject.toml`, you can set `tool.uv.override-dependencies`
to a list of requirements. If you provide multiple overrides for the same package, we apply them
simultaneously, while markers are applied as usual.

## Time-restricted reproducible resolutions

uv supports an `--exclude-newer` option to limit resolution to distributions published before a
specific date, allowing reproduction of installations regardless of new package releases. The date
may be specified as an [RFC 3339](https://www.rfc-editor.org/rfc/rfc3339.html) timestamp (e.g.,
`2006-12-02T02:07:43Z`) or UTC date in the same format (e.g., `2006-12-02`).

Note the package index must support the `upload-time` field as specified in
[`PEP 700`](https://peps.python.org/pep-0700/). If the field is not present for a given
distribution, the distribution will be treated as unavailable. PyPI provides `upload-time` for all
packages.

To ensure reproducibility, messages for unsatisfiable resolutions will not mention that
distributions were excluded due to the `--exclude-newer` flag — newer distributions will be treated
as if they do not exist.
