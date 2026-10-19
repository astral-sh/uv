# Universal resolver internals

!!! tip

    This document focuses on the internal workings of uv's universal resolver. For using universal
    resolution, see the [resolution concept](../../concepts/resolution.md#universal-resolution)
    documentation. For the underlying PubGrub loop, see the [resolver internals](./resolver.md)
    documentation.

## Universal resolution

Most Python resolvers solve for one concrete marker environment: one Python version, one operating
system, one architecture, one Python implementation, and so on. uv's project lockfile is different.
It needs to contain one resolution that can be installed across all marker environments supported by
the project.

The central invariant of universal resolution is:

> For any concrete marker environment, and for any install target that does not enable declared
> conflicts, the universal resolution must install at most one version of each package.

This allows a universal resolution to contain multiple versions of the same package, as long as
their markers are disjoint. For example, the lockfile may contain one `numpy` version for
`python_version < "3.10"` and another for `python_version >= "3.10"`, but a Python 3.10 install must
only see the latter.

uv satisfies this invariant by treating universal resolution as a partitioning problem. A
platform-specific resolution has one concrete marker environment, so every dependency either applies
or does not apply. A universal resolution starts with a set of possible marker environments. When uv
finds requirements that only apply to different parts of that set, it can split the resolution into
separate branches, called forks.

Each fork is still solved with the normal version solver. The universal resolver is the layer around
that solver that decides when to partition the marker space, keeps each fork disjoint from the
others, and merges the solved forks back into one graph.

## Marker space

Universal resolution relies on marker algebra. uv needs to answer questions such as:

- Does this requirement overlap the current fork?
- Are these two requirements disjoint?
- What marker expression covers the environments not handled by this fork?
- Do the stored lockfile forks still cover the supported environments?

The important guarantee is that every active fork is both disjoint from its siblings and within the
supported marker space. The union of active forks should cover the environments that still need a
solution. When uv cannot prove that two requirements are disjoint, it must keep them in the same
fork and ask the version solver to find one version that satisfies the overlap.

Configured `environments` reduce the supported marker space before forking begins. uv may still
split that reduced space into narrower forks, but it should not solve branches that are disjoint
from all configured environments.

Disjointness is what makes a fork sound. If two requirements for the same package have overlapping
markers, uv must find one version that satisfies the overlap. If their markers are disjoint, uv can
solve them in separate forks:

```text
numpy<2 ; python_version < "3.10"
numpy>=2 ; python_version >= "3.10"
```

The two requirements above can fork. These two cannot:

```text
numpy<2 ; python_version >= "3.9"
numpy>=2 ; python_version >= "3.10"
```

The overlap at `python_version >= "3.10"` means the environment could require both constraints at
once, so PubGrub must resolve them together or fail.

The universal marker space is not only the PEP 508 environment marker space. Declared conflicts add
another dimension: whether a package, extra, or dependency group is active. PEP 508 has no native
syntax for "the `foo` group of package `bar` is enabled", but uv still needs to distinguish those
states while locking and later reject install targets that enable conflicting states together.

## Resolution flow

Universal resolution proceeds in phases that feed information into each other:

1. Establish the supported marker space from `requires-python`, `environments`, and any reusable
   `resolution-markers` from an existing lockfile.
2. Solve the root requirements in the current fork.
3. As package metadata is discovered, split forks when same-package dependencies are mutually
   exclusive.
4. During version selection, split forks when a candidate is only valid for part of the supported
   marker space.
5. Apply declared conflicts by resolving mutually exclusive dependency sets in separate conflict
   states.
6. Merge the solved forks into one graph and persist the marker information needed to reproduce the
   same partitioning later.

The phases are incremental. uv does not need to know the entire dependency graph before it starts
solving; it discovers new reasons to fork as metadata and candidate distributions are loaded.

## Dependency forks

Most forks are discovered after uv has selected a package version and read its dependency metadata.
The dependencies are grouped by package name. For each group, uv decides whether the dependencies
can remain in the same fork or whether they need separate forks.

uv avoids forking when there is only one dependency for a package name. It also avoids forking when
all dependencies for a package name have the same marker. This conservative rule keeps lockfiles
smaller and resolutions faster, but it means uv does not always detect possible non-sibling
transitive forks ahead of time.

When a dependency marker can introduce a fork, uv splits the current fork into:

- the part where the dependency marker is true
- the remaining part where the dependency marker is false

The remaining fork is important. It covers gaps in the marker universe, such as the Linux branch in
this example:

```text
flask>1 ; sys_platform == "darwin"
flask>2 ; sys_platform == "win32"
flask
```

The resolver needs forks for macOS, Windows, and the complement of both, otherwise dependencies that
apply outside macOS and Windows could disappear.

Forks are nested. If a fork is already constrained to `python_version >= "3.10"` and later splits on
`sys_platform == "darwin"`, the new fork marker is the intersection of both conditions. Dependencies
that do not overlap the new fork are removed from that fork before solving continues.

## Version forks

Some forks are created during version selection instead of dependency expansion. These forks happen
after uv knows more about a candidate distribution.

The most common version fork is based on `Requires-Python`. Suppose a project supports
`requires-python = ">=3.8"`, and the latest version of a dependency requires Python 3.10 or newer.
If uv used one version for the whole project, it would have to choose an older release that still
supports Python 3.8. With the default fork strategy, uv splits the Python requirement and solves one
fork below Python 3.10 and another at Python 3.10 or newer. This lets newer Python versions receive
newer dependency versions without dropping support for older Python versions.

Version forks are also used for artifact coverage. Packages that publish wheels but no source
distribution are only installable where a compatible wheel exists. If the user configures
`required-environments`, uv checks whether selected distributions cover those environments. Missing
coverage can force uv to fork and choose another version, or fail if no version can satisfy the
required environment.

Local versions can also fork. For packages such as PyTorch, a local version like `2.5.2+cpu` may
have different wheel coverage than the base version `2.5.2`. If the base version covers platforms
that the local version does not, uv can fork the resolution so each platform uses the appropriate
variant.

## Conflicts

Declared conflicts are a separate fork dimension. A conflict set says that multiple dependency sets,
such as two extras, two groups, or two workspace packages, are not installable together. During
locking, this lets uv resolve those dependency sets separately. During installation, uv rejects an
install target that enables two or more items from the same conflict set.

A conflict item is one of:

- a package's production dependencies
- a package extra
- a package dependency group

When a conflict set is relevant to a fork, uv creates one fork that excludes all items in the set,
then one fork for each item in the set. The per-item fork includes that item and excludes all the
others. For a conflict set `{a, b, c}`, this produces:

```text
none of a, b, c
a only
b only
c only
```

If earlier forks have already excluded enough items, uv can skip the full split and just filter out
dependencies that are no longer active.

Dependency groups can include other groups, so uv expands conflicts through transitive group
includes before resolution. This ensures a conflict with one group also applies to groups that
include it.

## Merging forks

Each fork produces a normal resolution graph. Merging combines those graphs into one lockfile graph
and annotates each edge with the marker conditions under which that dependency exists. An edge
marker is the intersection of the dependency marker and the fork marker that produced the edge.

After all forks are merged, uv computes marker reachability through the graph. This determines the
marker conditions under which each package is actually reachable from a root. Reachability matters
because a package can appear in a solved fork without being reachable in every environment that the
fork covered. The merged graph should only preserve dependencies that are reachable under at least
one concrete install target.

Conflict markers are simplified after reachability. For example, if a graph path activates
`foo[extra1]`, uv can infer that conflicting extra `foo[extra2]` is inactive along that path. Those
inferences simplify the conflict portion of the marker attached to the graph edge.

Finally, unreachable nodes are discarded. If multiple versions of the same package remain reachable,
their reachability markers must be disjoint, or they must be separated by a declared conflict. That
is the graph-level form of the central invariant: a concrete install target can evaluate the markers
and select at most one version of each package.

## Lockfile markers

Where a fork occurs can depend on preferences from an existing lockfile. A locked version might
avoid the exact package version that caused a previous fork, which can produce a different fork
shape on the next run. To avoid this, uv persists fork markers in the lockfile as
`resolution-markers`.

There are two related uses:

- Top-level `resolution-markers` record the fork markers used to solve the lockfile. On the next
  resolution, uv uses them as initial forks.
- Package-level `resolution-markers` are written when multiple versions of the same package appear
  in the lockfile. They describe which fork markers selected each version.

The lock also records `supported-markers` and `required-markers` when the user configures
`environments` or `required-environments`. Before reusing lockfile forks, uv checks that the stored
fork markers still cover the supported marker space and still overlap the current `requires-python`
range. If the supported environments, required environments, or Python support range no longer
match, uv performs a clean resolution and lets new forks be discovered.

These markers are generated lockfile state, not user configuration. They preserve the partitioning
uv used for the prior resolution so future lock operations do not accidentally change shape just
because dependency preferences now point to different versions.

Fork markers are canonicalized before they are persisted. When the PEP 508 portions are already
disjoint, uv stores only those simplified PEP 508 markers. When they overlap because conflict
markers are needed to distinguish forks, uv stores the combined marker state.

## Logs

Forking is visible in verbose logs. Useful messages include:

```text
Splitting resolution on ...
Solving split ... (requires-python: ...)
Distinct solution for split ...
```

These messages show the package that triggered a fork, the marker space of each fork, and the number
of distinct fork solutions that were merged into the final output.
