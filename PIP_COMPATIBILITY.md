# Compatibility with `pip` and `pip-tools`

uv is designed as a drop-in replacement for common `pip` and `pip-tools` workflows.

Informally, the intent is such that existing `pip` and `pip-tools` users can switch to uv without
making meaningful changes to their packaging workflows; and, in most cases, swapping out
`pip install` for `uv pip install` should "just work".

However, uv is _not_ intended to be an _exact_ clone of `pip`, and the further you stray from
common `pip` workflows, the more likely you are to encounter differences in behavior. In some cases,
those differences may be known and intentional; in others, they may be the result of implementation
details; and in others, they may be bugs.

This document outlines the known differences between uv and `pip`, along with rationale,
workarounds, and a statement of intent for compatibility in the future.

## Configuration files and environment variables

uv does not read configuration files or environment variables that are specific to `pip`, like
`pip.conf` or `PIP_INDEX_URL`.

Reading configuration files and environment variables intended for other tools has a number of
drawbacks:

1. It requires bug-for-bug compatibility with the target tool, since users end up relying on bugs in
   the format, the parser, etc.
2. If the target tool _changes_ the format in some way, uv is then locked-in to changing it in
   equivalent ways.
3. If that configuration is versioned in some way, uv would need to know _which version_ of the
   target tool the user is expecting to use.
4. It prevents uv from introducing any settings or configuration that don't exist in the target
   tool, since otherwise `pip.conf` (or similar) would no longer be usable with `pip`.
5. It can lead user confusion, since uv would be reading settings that don't actually affect its
   behavior, and many users may _not_ expect uv to read configuration files intended for other
   tools.

Instead, uv supports its own environment variables, like `UV_INDEX_URL`. In the future, uv will
also support persistent configuration in its own configuration file format (e.g., `pyproject.toml`
or `uv.toml` or similar). For more, see [#651](https://github.com/astral-sh/uv/issues/651).

## Direct URL dependencies without package names

`pip` allows for direct URL dependencies to be provided on the command line or in `requirements.txt`
files without a package name, as in `pip install https://github.com/pallets/flask`, or:

```txt
# requirements.txt
git+https://github.com/pallets/flask
```

This is a common pattern in `pip` workflows, and is used to install a package from a direct URL
without incorporating the package name upfront.

`uv` supports direct URL dependencies from HTTP and VCS sources, but requires that the package name
be provided upfront, as in `uv install "flask @ git+https://github.com/pallets/flask"`, or:

```txt
# requirements.txt
flask @ git+https://github.com/pallets/flask
```

In the future, `uv` will support direct URL dependencies without package names. For more, see
[#313](https://github.com/astral-sh/uv/issues/313).

## Transitive direct URL dependencies

While uv does support direct URL dependencies (e.g., `black @ https://...`), it does not support
nested (or "transitive") direct URL dependencies, instead requiring that any direct URL dependencies
are declared upfront.

For example, if `black @ https://...` itself had a dependency on `toml @ https://...`, uv would
reject the transitive direct URL dependency on `toml` and require that `toml` be declared as a
dependency in the `pyproject.toml` file, like:

```toml
# pyproject.toml
dependencies = [
    "black @ https://...",
    "toml @ https://...",
]
```

This is a deliberate choice to avoid the correctness and security issues associated with allowing
transitive dependencies to introduce arbitrary URLs into the dependency graph.

For example:

- Your package depends on `package_a==1.0.0`.
- Your package depends on `package_b==1.0.0`.
- `package_b==1.0.0` depends on `package_a @ https://...`.

If `package_a @ https://...` happens to resolve to version `1.0.0`, `pip` would install `package_a`
from the direct URL. This is a security issue, since the direct URL could be controlled by an
attacker, and a correctness issue, since the direct URL could resolve to an entirely different
package with the same name and version.

In the future, uv may allow transitive URL dependencies in some form (e.g., with user opt-in).
For more, see [#1808](https://github.com/astral-sh/uv/issues/1808).

## Pre-release compatibility

By default, uv will accept pre-release versions during dependency resolution in two cases:

1. If the package is a direct dependency, and its version markers include a pre-release specifier
   (e.g., `flask>=2.0.0rc1`).
1. If _all_ published versions of a package are pre-releases.

If dependency resolution fails due to a transitive pre-release, uv will prompt the user to
re-run with `--prerelease=allow`, to allow pre-releases for all dependencies.

Alternatively, you can add the transitive dependency to your `requirements.in` file with
pre-release specifier (e.g., `flask>=2.0.0rc1`) to opt in to pre-release support for that specific
dependency.

In sum, uv needs to know upfront whether the resolver should accept pre-releases for a given
package. `pip`, meanwhile, _may_ respect pre-release identifiers in transitive dependencies
depending on the order in which the resolver encounters the relevant specifiers ([#1641](https://github.com/astral-sh/uv/issues/1641#issuecomment-1981402429)).

Pre-releases are [notoriously difficult](https://pubgrub-rs-guide.netlify.app/limitations/prerelease_versions)
to model, and are a frequent source of bugs in packaging tools. Even `pip`, which is viewed as a
reference implementation, has a number of open questions around pre-release handling ([#12469](https://github.com/pypa/pip/issues/12469),
[#12470](https://github.com/pypa/pip/issues/12470), [#40505](https://discuss.python.org/t/handling-of-pre-releases-when-backtracking/40505/20), etc.).
uv's pre-release handling is _intentionally_ limited and _intentionally_ requires user opt-in for
pre-releases, to ensure correctness.

In the future, uv _may_ support pre-release identifiers in transitive dependencies. However, it's
likely contingent on evolution in the Python packaging specifications. The existing PEPs [do not
cover "dependency resolution"](https://discuss.python.org/t/handling-of-pre-releases-when-backtracking/40505/17)
and are instead focused on behavior for a _single_ version specifier. As such, there are unresolved
questions around the correct and intended behavior for pre-releases in the packaging ecosystem more
broadly.

## Local version identifiers

uv does not implement spec-compliant handling of local version identifiers (e.g., `1.0.0+local`).
Though local version identifiers are rare in published packages (and, e.g., disallowed on PyPI),
they're common in the PyTorch ecosystem. uv's incorrect handling of local version identifiers
may lead to resolution failures in some cases.

In the future, uv intends to implement spec-compliant handling of local version identifiers.
For more, see [#1855](https://github.com/astral-sh/uv/issues/1855).

## Packages that exist on multiple indexes

In both uv and `pip`, users can specify multiple package indexes from which to search for
the available versions of a given package. However, uv and `pip` differ in how they handle
packages that exist on multiple indexes.

For example, imagine that a company publishes an internal version of `requests` on a private index
(`--extra-index-url`), but also allow installing packages from PyPI by default. In this case, the
private `requests` would conflict with the public [`requests`](https://pypi.org/project/requests/)
on PyPI.

When uv searches for a package across multiple indexes, it will iterate over the indexes in order
(preferring the `--extra-index-url` over the default index), and stop searching as soon as it
finds a match. This means that if a package exists on multiple indexes, uv will limit its
candidate versions to those present in the first index that contains the package.

`pip`, meanwhile, will combine the candidate versions from all indexes, and select the best
version from the combined set., though it makes [no guarantees around the order](https://github.com/pypa/pip/issues/5045#issuecomment-369521345)
in which it searches indexes, and expects that packages are unique up to name and version, even
across indexes.

uv's behavior is such that if a package exists on an internal index, it should always be installed
from the internal index, and never from PyPI. The intent is to prevent "dependency confusion"
attacks, in which an attacker publishes a malicious package on PyPI with the same name as an
internal package, thus causing the malicious package to be installed instead of the internal
package. See, for example, [the `torchtriton` attack](https://pytorch.org/blog/compromised-nightly-dependency/)
from December 2022.

In the future, uv will support pinning packages to dedicated indexes (see: [#171](https://github.com/astral-sh/uv/issues/171)).
Additionally, [PEP 708](https://peps.python.org/pep-0708/) is a provisional standard that aims to
address the "dependency confusion" issue across package registries and installers.

## Virtual environments by default

`uv pip install` and `uv pip sync` are designed to work with virtual environments by default.

Specifically, uv will always install packages into the currently active virtual environment, or
search for a virtual environment named `.venv` in the current directory or any parent directory
(even if it is not activated).

This differs from `pip`, which will install packages into a global environment if no virtual
environment is active, and will not search for inactive virtual environments.

In uv, you can install into non-virtual environments by providing a path to a Python executable
via the `--python /path/to/python` option, or via the `--system` flag, which installs into the
first Python interpreter found on the `PATH`, like `pip`.

In other words, uv inverts the default, requiring explicit opt-in to installing into the system
Python, which can lead to breakages and other complications, and should only be done in limited
circumstances.

For more, see ["Installing into arbitrary Python environments"](./README.md#installing-into-arbitrary-python-environments).

## Resolution strategy

For a given set of dependency specifiers, it's often the case that there is no single "correct"
set of packages to install. Instead, there are many valid sets of packages that satisfy the
specifiers.

Neither `pip` nor uv make any guarantees about the _exact_ set of packages that will be
installed; only that the resolution will be consistent, deterministic, and compliant with the
specifiers. As such, in some cases, `pip` and uv will yield different resolutions; however, both
resolutions _should_ be equally valid.

For example, consider:

```txt
# requirements.txt
starlette
fastapi
```

At time of writing, the most recent `starlette` version is `0.37.2`, and the most recent `fastapi`
version is `0.110.0`. However, `fastapi==0.110.0` also depends on `starlette`, and introduces an
upper bound: `starlette>=0.36.3,<0.37.0`.

If a resolver prioritizes including the most recent version of `starlette`, it would need to use
an older version of `fastapi` that excludes the upper bound on `starlette`. In practice, this
requires falling back to `fastapi==0.1.17`:

```txt
# This file was autogenerated by uv via the following command:
#    uv pip compile -
annotated-types==0.6.0
    # via pydantic
anyio==4.3.0
    # via starlette
fastapi==0.1.17
idna==3.6
    # via anyio
pydantic==2.6.3
    # via fastapi
pydantic-core==2.16.3
    # via pydantic
sniffio==1.3.1
    # via anyio
starlette==0.37.2
    # via fastapi
typing-extensions==4.10.0
    # via
    #   pydantic
    #   pydantic-core
```

Alternatively, if a resolver prioritizes including the most recent version of `fastapi`, it would
need to use an older version of `starlette` that satisfies the upper bound. In practice, this
requires falling back to `starlette==0.36.3`:

```txt
#    uv pip compile -
annotated-types==0.6.0
    # via pydantic
anyio==4.3.0
    # via starlette
fastapi==0.110.0
idna==3.6
    # via anyio
pydantic==2.6.3
    # via fastapi
pydantic-core==2.16.3
    # via pydantic
sniffio==1.3.1
    # via anyio
starlette==0.36.3
    # via fastapi
typing-extensions==4.10.0
    # via
    #   fastapi
    #   pydantic
    #   pydantic-core
```

When uv resolutions differ from `pip` in undesirable ways, it's often a sign that the specifiers
are too loose, and that the user should consider tightening them. For example, in the case of
`starlette` and `fastapi`, the user could require `fastapi>=0.110.0`.

## Hash-checking mode

While `uv` will include hashes via `uv pip compile --generate-hashes`, it does not support
hash-checking mode, which is a feature of `pip` that allows users to verify the integrity of
downloaded packages by checking their hashes against those provided in the `requirements.txt` file.

In the future, `uv` will support hash-checking mode. For more, see [#474](https://github.com/astral-sh/uv/issues/474).

## Strictness and spec enforcement

uv tends to be stricter than `pip`, and will often reject packages that `pip` would install.
For example, uv omits packages with invalid version specifiers in its metadata, which `pip` plans
to do in a [future release](https://github.com/pypa/pip/issues/12063).

In some cases, uv implements lenient behavior for popular packages that are known to have
specific spec compliance issues.

If uv rejects a package that `pip` would install due to a spec violation, the best course of
action is to first attempt to install a newer version of the package; and, if that fails, to report
the issue to the package maintainer.

## `pip` command-line options and subcommands

uv does not support the complete set of `pip`'s command-line options and subcommands, although it
does support a large subset.

Missing options and subcommands are prioritized based on user demand and the complexity of
the implementation, and tend to be tracked in individual issues. For example:

- [`--trusted-host`](https://github.com/astral-sh/uv/issues/1339)
- [`--user`](https://github.com/astral-sh/uv/issues/2077)
- [`--no-build-isolation`](https://github.com/astral-sh/uv/issues/1715)

If you encounter a missing option or subcommand, please search the issue tracker to see if it has
already been reported, and if not, consider opening a new issue. Feel free to upvote any existing
issues to convey your interest.

## Legacy features

`uv` does not support features that are considered legacy or deprecated in `pip`. For example,
`uv` does not support `.egg`-style distributions.

`uv` does not plan to support features that the `pip` maintainers explicitly recommend against,
like `--target`.
