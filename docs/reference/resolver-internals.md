# Resolution internals

This page explains some of the internal workings of uv, its resolver and the lockfile. For using uv,
see [Resolution](../concepts/resolution.md).

## Dependency resolution with PubGrub

If you look into a textbook, it will tell you that finding a set of version to install from a given
set of requirements is equivalent to the
[SAT problem](https://en.wikipedia.org/wiki/Boolean_satisfiability_problem) and thereby NP-complete,
i.e., in the worst case you have to try all possible combinations of all versions of all packages
and there are no general fast algorithms. In practice, this is fairly misleading for a number of
reasons:

- The slowest part of uv is loading package and version metadata, even if it's cached.
- Certain solution are more preferable than others, for example we generally want to use latest
  versions.
- Requirements follow lots of patterns: We use continuous versions ranges and not arbitrary boolean
  inclusion/exclusions of versions, adjacent release have the same or similar requirements, etc.
- For the majority of resolutions, we wouldn't even need to backtrack, just picking versions
  iteratively is sufficient. If we have preferences from a previous resolution we often barely need
  to anything at all.
- We don't just need either a solution or a message that there is no solution (like for SAT), we
  need an understandable error trace that tell you which packages are involved in away to allows you
  to remove the conflict.

uv uses [pubgrub-rs](https://github.com/pubgrub-rs/pubgrub), the Rust implementation of
[PubGrub](https://nex3.medium.com/pubgrub-2fb6470504f), an incremental version solver. PubGrub in uv
works in the following steps:

- We have a partial solution that tells us for which packages we already picked versions and for
  which we still need to decide.
- From the undecided packages we pick the one with the highest priority. Package with URLs
  (including file, git, etc.) have the highest priority, then those with more exact specifiers (such
  as `==`), then those with less strict specifiers. Inside each category, we order packages by when
  we first saw them, making the resolution deterministic.
- For that package with the highest priority, pick a version that works with all specifiers from the
  packages with versions in the partial solution and that is not yet marked as incompatible. We
  prefer versions from a lockfile (`uv.lock` or `-o requirements.txt`) and installed versions, then
  we go from highest to lowest (unless you changed the resolution mode). You can see this happening
  by the `Selecting ...` messages in `uv lock -v`.
- Add all requirements of this version to pubgrub. Start prefetching their metadata in the
  background.
- Now we either we repeat this process with the next package or we have a conflict. Let's say we
  pick picked, among other packages, `a` 2 and then `b` 2, and those have requirements `a 2 -> c 1`
  and `b 2 -> c 2`. When trying to pick a version for `c`, we see there is no version we can pick.
  Using its internal incompatibilities store, PubGrub traces this back to `a 2` and `b 2` and adds
  an incompatibility for `{a 2, b 2}`, meaning when either is picked we can't select the other. We
  restore the state with `a` 2 before picking `b` 2 with the new learned incompatibility and pick a
  new version for `b`.

Eventually, we either have picked compatible versions for all packages and get a successful
resolution, or we get an incompatibility for the virtual root package, that is whatever versions of
the root dependencies and their transitive dependencies we'd pick, we'll always get a conflict. From
the incompatibilities in PubGrub, we can trace which packages were involved and format an error
message. For more details on the PubGrub algorithm, see
[Internals of the PubGrub algorithm](https://pubgrub-rs-guide.pages.dev/internals/intro).

## Forking

Python historically didn't have backtracking version resolution, and even with version resolution,
it was usually limited to single environment, which one specific architecture, operating system,
python version and python implementation. Some packages use contradictory requirements for different
environments, something like:

```text
numpy>=2,<3 ; python_version >= "3.11"
numpy>=1.16,<2 ; python_version < "3.11"
```

Since Python only allows one version package, just version resolution would error here. Inspired by
[poetry](https://github.com/python-poetry/poetry), we instead use forking: Whenever there are
multiple requirements with different for one package name in the requirements of a package, we split
the resolution around these requirements. In this case, we take our partial solution and then once
solve the rest for `python_version >= "3.11"` and once for `python_version < "3.11"`. If some
markers overlap or are missing a part of the marker space, we add additional forks. There can be
more than 2 forks per package and we nest forks. You can see this in the log of `uv lock -v` by
looking for `Splitting resolution on ...`, `Solving split ... (requires-python: ...)` and
`Split ... resolution took ...`.

One problem is that where and how we split is dependent on the order we see packages, which is in
turn dependent on the preference you get e.g. from `uv.lock`. So it can happen that we solve your
requirements with specific forks, write this to the lockfile, and when you call `uv lock` again,
we'd do a different resolution even if nothing changed because the preferences cause us to use
different fork points. To avoid this we write the `environment-markers` of each fork and each
package that diverges between forks to the lockfile. When doing a new resolution, we start with the
forks from the lockfile and use fork-dependent preference (from the `environment-markers` on each
package) to keep the resolution stable. When requirements change, we may introduce new forks from
the saved forks. We also merge forks with identical packages to keep the number of forks low.

## Requires-python

To ensure that a resolution with `requires-python = ">=3.9"` can actually be installed for all those
python versions, uv requires that all dependency support at least that python version. We reject
package versions that declare e.g. `requires-python = ">=3.10"` because we already know that a
resolution with that version can't be installed on Python 3.9, while the user explicitly requested
including 3.9. For simplicity and forward compatibility, we do however only consider lower bounds
for requires-python. If a dependency declares `requires-python = ">=3.8,<4"`, we don't want to
propagate that `<4` marker.

## Wheel tags

While our resolution is universal with respect to requirement markers, this doesn't extend to wheel
tags. Wheel tags can encode Python version, Python interpreter, operating system and architecture,
e.g. `torch-2.4.0-cp312-cp312-manylinux2014_aarch64.whl` is only compatible with CPython 3.12 on
arm64 Linux with glibc >= 2.17 (the manylinux2014 policy), while `tqdm-4.66.4-py3-none-any.whl`
works with all Python 3 versions and interpreters on any operating system and architecture. Most
projects have a (universally compatible) source distribution we can fall back to when we try to
install a package version and there is no compatible wheel, but some, such as `torch`, don't have a
source distribution. In this case an installation on e.g. Python 3.13 or an uncommon operating
system or architecture will fail with a message about a missing matching wheel.
