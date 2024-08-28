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
- There are many possible solutions, but some are preferable than others. For example we generally
  prefer using the latest version of packages.
- Package's dependencies are complex, e.g., there are contiguous versions ranges — not arbitrary
  boolean inclusion/exclusions of versions, adjacent releases often have the same or similar
  requirements, etc.
- For most resolutions, the resolver doesn't need to backtrack, picking versions iteratively is
  sufficient. If there are version preferences from a previous resolution, barely any work needs to
  be done.
- When resolution fails, more information is needed than a message that there is no solution (as is
  seen in SAT solvers). Instead, the resolver should produce an understandable error trace that
  states which packages are involved in away to allows a user to remove the conflict.

uv uses [pubgrub-rs](https://github.com/pubgrub-rs/pubgrub), the Rust implementation of
[PubGrub](https://nex3.medium.com/pubgrub-2fb6470504f), an incremental version solver. PubGrub in uv
works in the following steps:

- Start with a partial solution that declares which packages versions have been selected and which
  are undecided. Initially, only a virtual root package is decided.
- The highest priority package is selected from the undecided packages. Package with URLs (including
  file, git, etc.) have the highest priority, then those with more exact specifiers (such as `==`),
  then those with less strict specifiers. Inside each category, packages are ordered by when they
  were first seen (i.e. order in a file), making the resolution deterministic.
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

## Forking

Python resolvers historically didn't support backtracking, and even with backtracking, resolution
was usually limited to single environment, which one specific architecture, operating system, Python
version, and Python implementation. Some packages use contradictory requirements for different
environments, for example:

```python
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

```python
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

## Requires-python

To ensure that a resolution with `requires-python = ">=3.9"` can actually be installed for the
included Python versions, uv requires that all dependencies have the same minimum Python version.
Package versions that declare a higher minimum Python version, e.g., `requires-python = ">=3.10"`,
are rejected, because a resolution with that version can't be installed on Python 3.9. For
simplicity and forward compatibility, only lower bounds in `requires-python` are respected. For
example, if a package declares `requires-python = ">=3.8,<4"`, the `<4` marker is not propagated to
the entire resolution.

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
