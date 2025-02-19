# Resolver internals

!!! tip

    This document focuses on the internal workings of uv's resolver. For using uv, see the
    [resolution concept](../concepts/resolution.md) documentation.

## Resolver

As defined in a textbook, resolution, or finding a set of version to install from a given set of
requirements, is equivalent to the
[SAT problem](https://en.wikipedia.org/wiki/Boolean_satisfiability_problem) and thereby NP-complete:
in the worst case you have to try all possible combinations of all versions of all packages and
there are no general, fast algorithms. In practice, this is misleading for a number of reasons:

- The slowest part of resolution in uv is loading package and version metadata, even if it's cached.
- There are many possible solutions, but some are preferable to others. For example, we generally
  prefer using the latest version of packages.
- Package dependencies are complex, e.g., there are contiguous versions ranges — not arbitrary
  boolean inclusion/exclusions of versions, adjacent releases often have the same or similar
  requirements, etc.
- For most resolutions, the resolver doesn't need to backtrack, picking versions iteratively is
  sufficient. If there are version preferences from a previous resolution, barely any work needs to
  be done.
- When resolution fails, more information is needed than a message that there is no solution (as is
  seen in SAT solvers). Instead, the resolver should produce an understandable error trace that
  states which packages are involved in away to allows a user to remove the conflict.
- The most important heuristic for performance and user experience is determining the order in which
  decisions are made through prioritization.

uv uses [pubgrub-rs](https://github.com/pubgrub-rs/pubgrub), the Rust implementation of
[PubGrub](https://nex3.medium.com/pubgrub-2fb6470504f), an incremental version solver. PubGrub in uv
works in the following steps:

- Start with a partial solution that declares which packages versions have been selected and which
  are undecided. Initially, only a virtual root package is decided.
- The highest priority package is selected from the undecided packages. Roughly, packages with URLs
  (including file, git, etc.) have the highest priority, then those with more exact specifiers (such
  as `==`), then those with less strict specifiers. Inside each category, packages are ordered by
  when they were first seen (i.e. order in a file), making the resolution deterministic.
- A version is picked for the selected package. The version must works with all specifiers from the
  requirements in the partial solution and must not be previously marked as incompatible. The
  resolver prefers versions from a lockfile (`uv.lock` or `-o requirements.txt`) and those installed
  in the current environment. Versions are checked from highest to lowest (unless using an
  alternative [resolution strategy](../concepts/resolution.md#resolution-strategy)).
- All requirements of the selected package version are added to the undecided packages. uv
  prefetches their metadata in the background to improve performance.
- The process is either repeated with the next package unless a conflict is detected, in which the
  resolver will backtrack. For example, the partial solution contains, among other packages, `a 2`
  then `b 2` with the requirements `a 2 -> c 1` and `b 2 -> c 2`. No compatible version of `c` can
  be found. PubGrub can determine this was caused by `a 2` and `b 2` and add the incompatibility
  `{a 2, b 2}`, meaning that when either is picked, the other cannot be selected. The partial
  solution is restored to `a 2` with the tracked incompatibility and the resolver attempts to pick a
  new version for `b`.

Eventually, the resolver either picks compatible versions for all packages (a successful resolution)
or there is an incompatibility including the virtual "root" package which defines the versions
requested by the user. An incompatibility with the root package indicates that whatever versions of
the root dependencies and their transitive dependencies are picked, there will always be a conflict.
From the incompatibilities tracked in PubGrub, an error message is constructed to enumerate the
involved packages.

!!! tip

    For more details on the PubGrub algorithm, see [Internals of the PubGrub
    algorithm](https://pubgrub-rs-guide.pages.dev/internals/intro).

In addition to PubGrub's base algorithm, we also use a heuristic that backtracks and switches the
order of two packages if they have been conflicting too much.

## Forking

Python resolvers historically didn't support backtracking, and even with backtracking, resolution
was usually limited to single environment, which one specific architecture, operating system, Python
version, and Python implementation. Some packages use contradictory requirements for different
environments, for example:

```
numpy>=2,<3 ; python_version >= "3.11"
numpy>=1.16,<2 ; python_version < "3.11"
```

Since Python only allows one version of each package, a naive resolver would error here. Inspired by
[Poetry](https://github.com/python-poetry/poetry), uv uses a forking resolver: whenever there are
multiple requirements for a package with different markers, the resolution is split.

In the above example, the partial solution would be split into two resolutions, one for
`python_version >= "3.11"` and one for `python_version < "3.11"`.

If markers overlap or are missing a part of the marker space, the resolver splits additional times —
there can be many forks per package. For example, given:

```
flask > 1 ; sys_platform == 'darwin'
flask > 2 ; sys_platform == 'win32'
flask
```

A fork would be created for `sys_platform == 'darwin'`, for `sys_platform == 'win32'`, and for
`sys_platform != 'darwin' and sys_platform != 'win32'`.

Forks can be nested, e.g., each fork is dependent on any previous forks that occurred. Forks with
identical packages are merged to keep the number of forks low.

!!! tip

    Forking can be observed in the logs of `uv lock -v` by looking for
    `Splitting resolution on ...`, `Solving split ... (requires-python: ...)` and `Split ... resolution
    took ...`.

One difficulty in a forking resolver is that where splits occur is dependent on the order packages
are seen, which is in turn dependent on the preferences, e.g., from `uv.lock`. So it is possible for
the resolver to solve the requirements with specific forks, write this to the lockfile, and when the
resolver is invoked again, a different solution is found because the preferences result in different
fork points. To avoid this, the `resolution-markers` of each fork and each package that diverges
between forks is written to the lockfile. When performing a new resolution, the forks from the
lockfile are used to ensure the resolution is stable. When requirements change, new forks may be
added to the saved forks.

## Wheel tags

While uv's resolution is universal with respect to environment markers, this doesn't extend to wheel
tags. Wheel tags can encode the Python version, Python implementation, operating system, and
architecture. For example, `torch-2.4.0-cp312-cp312-manylinux2014_aarch64.whl` is only compatible
with CPython 3.12 on arm64 Linux with `glibc>=2.17` (per the `manylinux2014` policy), while
`tqdm-4.66.4-py3-none-any.whl` works with all Python 3 versions and interpreters on any operating
system and architecture. Most projects have a universally compatible source distribution that can be
used when attempted to install a package that has no compatible wheel, but some packages, such as
`torch`, don't publish a source distribution. In this case an installation on, e.g., Python 3.13, an
uncommon operating system, or architecture, will fail and complain that there is no matching wheel.

## Marker and wheel tag filtering

In every fork, we know what markers are possible. In non-universal resolution, we know their exact
values. In universal mode, we know at least a constraint for the python requirement, e.g.,
`requires-python = ">=3.12"` means that `importlib_metadata; python_version < "3.10"` can be
discarded because it can never be installed. If additionally `tool.uv.environments` is set, we can
filter out requirements with markers disjoint with those environments. Inside each fork, we can
additionally filter by the fork markers.

There is some redundancy in the marker expressions, where the value of one marker field implies the
value of another field. Internally, we normalize `python_version` and `python_full_version` as well
as known values of `platform_system` and `sys_platform` to a shared canonical representation, so
they can match against each other.

When we selected a version with a local tag (e.g.,`1.2.3+localtag`) and the wheels don't cover
support for Windows, Linux and macOS, and there is a base version without tag (e.g.,`1.2.3`) with
support for a missing platform, we fork trying to extend the platform support by using both the
version with local tag and without local tag depending on the platform. This helps with packages
that use the local tag for different hardware accelerators such as torch. While there is no 1:1
mapping between wheel tags and markers, we can do a mapping for well-known platforms, including
Windows, Linux and macOS.

## Requires-python

To ensure that a resolution with `requires-python = ">=3.9"` can actually be installed for the
included Python versions, uv requires that all dependencies have the same minimum Python version.
Package versions that declare a higher minimum Python version, e.g., `requires-python = ">=3.10"`,
are rejected, because a resolution with that version can't be installed on Python 3.9. For
simplicity and forward compatibility, only lower bounds in `requires-python` are respected. For
example, if a package declares `requires-python = ">=3.8,<4"`, the `<4` marker is not propagated to
the entire resolution.

This default is a problem for packages that use the version-dependent C API of CPython, such as
numpy. Each numpy release support 4 Python minor versions, e.g., numpy 2.0.0 has wheels for CPython
3.9 through 3.12 and declares `requires-python = ">=3.9"`, while numpy 2.1.0 has wheels for CPython
3.10 through 3.13 and declares `requires-python = ">=3.10"`. The means that when we resolve a
`numpy>=2,<3` requirement in a project with `requires-python = ">=3.9"`, we resolve numpy 2.0.0 and
the lockfile doesn't install on Python 3.13 or newer. To alleviate this, whenever we reject a
version due to a too high Python requirement, we fork on that Python version. This behavior is
controlled by `--fork-strategy`. In the example case, upon encountering numpy 2.1.0 we fork into
Python versions `>=3.9,<3.10` and `>=3.10` and resolve two different numpy versions:

```
numpy==2.0.0; python_version >= "3.9" and python_version < "3.10"
numpy==2.1.0; python_version >= "3.10"
```

## Prioritization

Prioritization is important for both performance and for better resolutions.

If we try many versions we have to later discard, resolution is slow, both because we have to read
metadata we didn't need and because we have to track a lot of (conflict) information for this
discarded subtree.

There are expectations about which solution uv should choose, even if the version constraints allow
multiple solutions. Generally, a desirable solution prioritizes use the highest versions for direct
dependencies over those for indirect dependencies, it avoids backtracking to very old versions and
can be installed on a target machine.

Internally, uv represent each package with a given package name as a number of virtual packages, for
example, one package for each activated extra, for dependency groups, or for having a marker. While
PubGrub needs to choose a version for each virtual package, uv's prioritization works on the package
name level.

Whenever we encounter a requirement on a package, we match it to a priority. The root package and
URL requirements have the highest priority, then singleton requirements with the `==` operator, as
their version can be directly determined, then highly conflicting packages (next paragraph), and
finally all other packages. Inside each category, packages are sorted by when they were first
encountered, creating a breadth first search that prioritizes direct dependencies including
workspace dependencies over transitive dependencies.

A common problem is that we have a package A with a higher priority than package B, and B is only
compatible with older versions of A. We decide the latest version for package A. Each time we decide
a version for B, it is immediately discarded due to the conflict with A. We have to try all possible
versions of B, until we have either exhausted the possible range (slow), pick a very old version
that doesn't depend on A, but most likely isn't compatible with the project either (bad) or fail to
build a very old version (bad). Once we see such conflict happen five time, we set A and B to
special highly-conflicting priority levels, and set them so that B is decided before A. We then
manually backtrack to a state before deciding A, in the next iteration now deciding B instead of A.
See [#8157](https://github.com/astral-sh/uv/issues/8157) and
[#9843](https://github.com/astral-sh/uv/pull/9843) for a more detailed description with real world
examples.
