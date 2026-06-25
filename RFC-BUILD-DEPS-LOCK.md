# RFC: Locking Build Dependencies In The Current Lockfile Model

## Summary

This document describes the problem space for locking Python build dependencies in `uv.lock`.

It intentionally does not propose a wire format or implementation approach. The goal is to make the
motivation and constraints clear before choosing how the current lockfile model should represent
build dependency resolution.

The current lockfile is centered on package records, artifacts, dependency edges, markers, sources,
hashes, and universal runtime fork state. Locking build dependencies has to fit into that world
without losing the lockfile's existing properties:

- runtime installs are replayed from the lockfile;
- selected artifacts and hashes are explicit;
- package records can be shared across universal runtime branches;
- marker and fork state determine which package records and edges are active;
- frozen operations do not perform dependency resolution outside the lock.

Build dependencies stress this model because source builds have isolated dependency environments
that are selected by source artifacts, target reachability, build policy, backend metadata, user
build settings, and nested builds. These environments can share package artifacts with the runtime
graph without sharing the same resolved dependency closure.

## Motivation

`uv.lock` already records enough information to replay runtime dependency selection. A package
record describes a package identity, source, artifacts, hashes, metadata, and dependency edges.
Markers and universal fork state decide which package records and dependency edges apply to a target
install.

That model works well for runtime dependencies because the runtime dependency graph is the thing
being installed. Even when universal resolution creates multiple target branches, the lockfile still
describes one runtime install surface: select the applicable branch and install the selected
packages.

Source builds introduce another dependency surface. Before uv can install a source distribution, it
may need to create an isolated build environment and install build requirements into that
environment. Those requirements are not runtime requirements of the project. They are requirements
of the build operation needed to produce metadata or an artifact.

PEP 517 splits build requirements into backend-declared requirements and hook-discovered
requirements. A source tree can declare initial requirements in `[build-system].requires`, and a
backend can report additional requirements from dependency-discovery hooks:

- `get_requires_for_build_wheel`
- `get_requires_for_build_sdist`
- `get_requires_for_build_editable`

PEP 660 adds editable build hooks and a separate editable dependency-discovery hook. The operation
matters: the requirements for building a wheel, building an sdist, and building an editable wheel
can differ.

Build requirements can also be implicit. A source distribution without a `[build-system]` table uses
the default backend semantics. A lockfile that captures explicit build requirements but forgets
implicit backend requirements can still force frozen sync to resolve build dependencies outside the
lock.

The reason to lock build dependencies is the same reason to lock runtime dependencies: frozen replay
should not discover a different dependency selection from the network, local indexes, mutable source
configuration, or newly published packages. If a source build is required during a frozen operation,
every package installed into its build environment should be accounted for by the lockfile.

At the same time, locking build dependencies is not the same as promising hermetic or byte-for-byte
reproducible builds. A backend can run arbitrary code, inspect the source tree, and produce
different artifacts for reasons outside dependency selection. The lockfile problem discussed here is
narrower: the packages installed into build environments should be selected by the lock, not
resolved afresh during frozen replay.

This follows the same practical contract uv already uses for runtime metadata. Universal locking
assumes that package metadata is stable enough to lock from the observed metadata. Build requirement
discovery should be treated as stable metadata for a given source artifact, build operation,
configuration settings, extra build dependencies, source/index configuration, constraints, and
frontend policy. If a backend reports different build requirements merely because the hook ran on a
different host, that package is outside the normal universal lock model in the same way that
inconsistent wheel metadata across platforms is outside the normal runtime lock model.

Markers inside build requirements are still meaningful. A stable build requirement can be
conditional:

```text
helper ; python_version < "3.12"
```

The marker is part of the locked metadata. Replay evaluates it in the appropriate build environment.
That is different from a backend returning an entirely different requirement list on each host.

## Current Lockfile Framing

The current lockfile is primarily a package graph.

A `[[package]]` record identifies a package and stores information such as:

- package name and version;
- source identity;
- source distribution and wheel artifacts;
- hashes for artifacts that need them;
- runtime metadata and dependency edges;
- markers that describe when the package or edge is selected;
- source configuration needed to replay direct, path, Git, or registry selections.

Universal runtime resolution adds fork state on top of that package graph. The current wire format
can serialize `resolution-markers`, and package or dependency markers can include uv-specific fork
encodings such as synthetic extras. These are not ordinary user extras; they encode resolver branch
state inside a marker-shaped string.

The lockfile already has to distinguish package identity from artifact identity. A registry package
can have multiple wheels and an sdist. A direct URL, path, archive, or Git source can identify a
concrete artifact or source tree. A package name and version alone are not sufficient to determine
which artifact is installed or built.

Build dependency locking adds another use for package records. The packages installed into build
environments are still ordinary packages with ordinary artifacts and hashes. They should not be
duplicated as an unrelated package catalogue. The hard part is not storing package artifacts; the
hard part is describing which package selections belong to which isolated build environment.

## Where Build Dependencies Enter

A build environment is needed only when a selected artifact must be built from source. If uv
installs a wheel, the source distribution's build requirements do not matter for that install.

This distinction matters for packages that have both wheels and source distributions. The lockfile
can know about both artifacts, but a frozen sync should not reconstruct or validate a build
environment for an sdist that was not selected. Build dependency locking has to follow artifact
selection, not just package existence.

For example, a lockfile can contain both artifacts:

```toml
[[package]]
name = "dep"
version = "1.0.0"
source = { registry = "https://example.test/simple" }
sdist = { url = "https://example.test/dep-1.0.0.tar.gz", hash = "sha256:..." }
wheels = [
    { url = "https://example.test/dep-1.0.0-py3-none-any.whl", hash = "sha256:..." },
]
```

If the install selects the wheel, then the sdist's build requirements are not part of that replay.
If `--no-binary dep` later forces the sdist, build dependency information for the sdist becomes
relevant.

Build policy also changes whether build dependencies are needed:

- `--no-build` disables all source builds;
- `--no-build-package` disables builds for specific packages;
- `--no-binary` can force source selection where a wheel would otherwise be used;
- target platform selection can make a wheel available on one target and unavailable on another.

The same lockfile can therefore contain enough package information for many targets and policies,
while only some combinations actually require build dependency replay.

## Why Runtime Dependencies Are Not Enough

Build dependencies are not runtime dependencies of the project. They are installed into isolated
build environments and can resolve differently from the runtime graph.

For example:

```text
runtime:
  builder==1
  helper==2

wheel(dep-a):
  builder==1
  helper==1

wheel(dep-b):
  builder==1
  helper==2
```

All three environments can legitimately use the same `builder==1` artifact. They do not necessarily
use the same dependency closure.

This matters because the current package graph naturally encourages sharing. If the lockfile stores
one global `builder==1` package record, adding build-only dependency edges to that record can
accidentally change what runtime traversal sees. Conversely, if uv refuses to add build-only edges
to an existing runtime package record, a build environment can become incomplete.

The ambiguous current-lockfile-shaped version looks like this:

```toml
[[package]]
name = "builder"
version = "1.0.0"
dependencies = [
    { name = "helper", version = "1.0.0" },
    { name = "helper", version = "2.0.0" },
]
```

That record no longer says which `helper` edge belongs to runtime replay, which belongs to
`wheel(dep-a)`, and which belongs to `wheel(dep-b)`. The problem is not that `builder==1` cannot be
shared. The problem is that sharing the package artifact does not imply sharing all outgoing
dependency edges.

The problem is not limited to conflicting versions. A build-only edge can be needed only under a
marker branch that is not active for the runtime package's own branch. If a package is shared
between runtime and build contexts, the lockfile has to preserve which edges belong to which
context.

## Target Reachability And Universal Forks

Build dependencies are needed only when the source package is selected for a target branch. In a
universal lock, that branch can be a complex combination of PEP 508 markers and uv fork state.

For example, a package can be selected only for Python 3.12 and newer:

```text
dep ; python_version >= "3.12"
```

If `dep` has build requirements that only support Python 3.12 and newer, the project can still
support older Python versions as long as `dep` is not selected there. A build dependency solve for
`dep` should be constrained by the branch where `dep` is reachable, not by the entire project
`requires-python` range.

Fork state can appear inside larger marker expressions. Current lockfiles can encode uv fork state
as synthetic extras inside marker strings:

```text
(sys_platform == "darwin" and extra == "extra-7-project-cpu")
or (sys_platform != "linux" and extra == "extra-7-project-cpu" and extra == "extra-7-project-cu124")
```

That marker is not merely a PEP 508 expression over user extras. It is a combined expression over
runtime environment predicates and uv resolver branch state. Build dependency locking has to
preserve this combined reachability when deciding whether a source package's build environment is
needed.

The important constraint is correlation. Some marker branches can require one fork activation, while
other branches require a different combination. A representation that flattens this into "marker X
and active extras Y" can change package membership.

For example, this current-lockfile-style marker carries both platform predicates and uv fork state:

```toml
[[package]]
name = "torchvision"
version = "0.20.1"
source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cpu" }
marker = "(sys_platform == 'darwin' and extra == 'extra-7-project-cpu') or (sys_platform != 'linux' and extra == 'extra-7-project-cpu' and extra == 'extra-7-project-cu124')"
```

The first branch requires the `cpu` fork state. The second branch requires both `cpu` and `cu124`. A
build dependency that is reachable through this package has to inherit that branch structure, not
just a flattened set of extras.

## Build Requirement Discovery Stages

PEP 517 build setup has a temporal shape:

```text
install [build-system].requires
call get_requires_for_build_*
install hook-returned requirements
call the metadata or artifact hook
```

The initial environment used to call `get_requires_for_build_*` is not always the same as the final
environment used to build. Hook-returned requirements can add packages, tighten requirements, or
supersede earlier selections.

This creates two separate correctness concerns:

- frozen replay must not resolve hook-returned requirements outside the lock;
- frozen replay must not install hook-only requirements before calling the hook if doing so changes
  the environment ordering that was used when the lock was created.

The lockfile problem is therefore not just "what packages are eventually in the build environment?"
It also includes how to validate that hook-discovered requirements still match the locked metadata
without accidentally changing the build setup sequence.

For example:

```toml
[build-system]
requires = ["seed>=1"]
build-backend = "backend"
```

and:

```text
get_requires_for_build_wheel() -> ["seed<2", "helper"]
```

The initial hook environment is allowed to install a different `seed` than the final build
environment. A lock that stores only the final package set cannot by itself explain the environment
used to call the discovery hook. A lock that stores only the initial requirements can miss `helper`.

## Static Metadata And Dynamic Hooks

Some source distributions have enough static metadata that uv can avoid executing a build backend
for runtime metadata. That does not automatically mean the build dependency environment is fully
known.

A static source can still have:

- `[build-system].requires`;
- implicit default backend requirements;
- backend hook requirements from `get_requires_for_build_*`;
- `tool.uv.sources` or index configuration that affects build requirements;
- extra build dependencies configured by the user.

If build dependency locking uses only static project metadata, it can miss hook-discovered
requirements. If it reads raw PEP 508 strings without applying the same source lowering as an actual
build, it can resolve from the wrong index or miss local path/Git sources.

The lockfile needs to reflect the effective build requirements uv would use for the actual build,
including source lowering and uv-specific build configuration.

## Extra Build Dependencies And Match-Runtime

uv supports extra build dependencies configured by the user. Some of these can use `match-runtime`,
meaning the build requirement is constrained by a package selected in the runtime graph.

This ties build dependency locking back to runtime resolution. A build environment cannot always be
determined from source metadata alone; it can depend on the runtime package selected for the same
target branch.

For example, a source package can require a build helper that must match the runtime-selected
`anyio`. In a universal lock, the selected runtime `anyio` can differ by marker branch. The build
dependency environment must follow the same branch.

One branch might select:

```text
runtime linux:
  anyio==4.8.0

build child on linux:
  anyio==4.8.0
```

while another selects:

```text
runtime older-python:
  anyio==3.7.1

build child on older-python:
  anyio==3.7.1
```

The build dependency is not independent; it is derived from the runtime selection in the same target
branch.

This is a problem statement, not a representation choice. The important point is that build
dependency locking cannot be completely independent of the runtime resolution when user
configuration explicitly connects them.

## Source Artifact Identity And Freshness

Build dependencies are requirements of a build operation for a specific source artifact. Package
name and version are not enough.

Different source artifacts for the same package version can have different:

- `pyproject.toml` contents;
- `[build-system]` requirements;
- implicit or explicit backend behavior;
- `tool.uv.sources` entries;
- generated metadata;
- included files that affect backend logic.

The lockfile already records artifact identity for installable distributions. Build dependency
locking has to apply the same discipline to source artifacts. For registry packages, the selected
sdist URL and hash matter. For direct URLs, path archives, directories, and Git sources, the direct
source identity and freshness policy matter.

Mutable local directories are especially sensitive. A directory can keep the same package name and
version while changing its build requirements or source configuration. A frozen or locked operation
has to detect when the existing build dependency information no longer describes the current source
input.

For example, the package identity can stay constant while the effective build source changes:

```toml
[project]
name = "dep"
version = "1.0.0"

[build-system]
requires = ["seed"]

[tool.uv.sources]
seed = { path = "../seed-v1" }
```

Changing only the source mapping changes the build dependency input:

```toml
[tool.uv.sources]
seed = { path = "../seed-v2" }
```

The raw requirement name is still `seed`, but the effective resolved source is different.

Explicitly empty build requirements also carry information. A source with an explicit
`[build-system] requires = []` is not the same as a source with no `[build-system]` table if the
latter would use an implicit default backend. The lockfile has to preserve enough state to notice
that transition.

## Nested Build Dependencies

A build dependency can itself be selected from source. In that case, building the parent source
requires constructing a build environment for the nested source dependency.

For example:

```text
dep builds with builder
builder is selected as an sdist
builder has its own build requirements
```

Nested build requirements are needed only when the selected artifact for the build dependency is a
source artifact. If the build dependency is installed from a wheel, an unselected fallback sdist's
build requirements should not be replayed.

This mirrors the top-level artifact-selection rule: build environments follow selected source
artifacts, not every source artifact mentioned in the lock.

Nested builds also introduce cycle and coverage concerns. If replaying one build environment
requires another source build, the lockfile needs enough information to know whether that nested
build environment is also locked and whether the chain terminates.

## Generated Sdist Pipelines

A source tree can be built through an intermediate sdist:

```text
source tree --build_sdist--> generated archive --build_wheel--> wheel
```

These are two build operations with different hooks and potentially different build requirements.
The wheel build operates on the generated archive, not the original directory. If the generated
archive changes, the wheel build input has changed even if the package name and version have not.

The current lockfile framing already distinguishes package identity from artifact identity.
Generated sdist pipelines require applying that distinction to build inputs as well.

## Artifact Hashes And Build-Only Packages

Build-only packages can come from the same sources as runtime packages: registry artifacts, direct
URLs, local wheels, path archives, directories, and Git sources.

If a build requirement is lowered to a local wheel path, the resulting package record still needs
the hash information required for path wheel dependencies. A package being build-only does not relax
the lockfile's artifact integrity rules.

Build-only local source packages can also have runtime metadata of their own. Even if they are never
installed into the project runtime environment, their metadata can be needed to validate their
build-environment dependency edges or to reconstruct nested build environments.

## Empty And Skipped Build Graphs

An empty build dependency environment can be complete. For example, every declared build requirement
might be false in the supported environment set, or a source can explicitly declare no build
requirements.

The lockfile has to distinguish:

- no build dependency information was captured;
- build dependency capture was skipped because builds were disabled;
- capture completed and the resulting environment was empty.

Those states can all look superficially similar in a package entry:

```toml
[[package]]
name = "dep"
version = "1.0.0"
# no build dependency data

[[package]]
name = "dep"
version = "1.0.0"
build-dependencies = []
```

The second form can mean "capture completed and nothing is needed" or "builds were disabled and no
environment was captured" unless the lockfile records enough surrounding state to distinguish them.

These states behave differently when build policy changes. A lock created with
`--no-build-package dep` can legitimately lack build dependency information for `dep`; after
removing that option, uv may need that information. An unchanged empty capture, however, should not
force repeated relocking.

## Direct Builds And Virtual Projects

Some sources can be handled by direct build paths that do not require a normal isolated build
dependency environment. Other project entries are virtual and are never built or installed as
packages.

Build dependency locking must not infer "has build-system metadata" as "must lock a build
environment" without considering whether the source is actually built by the command. Otherwise
enabling build dependency locking can make valid projects fail by resolving build requirements for
packages that are not part of the build/install replay contract.

## Lock Freshness

Build dependency information can go stale for reasons that do not affect the runtime dependency
graph directly:

- `[build-system].requires` changes;
- implicit backend behavior changes by adding or removing `[build-system]`;
- backend hook requirements change;
- `tool.uv.sources` for a build requirement changes;
- index configuration changes;
- constraints or build constraints change;
- extra build dependencies change;
- `match-runtime` bindings change because runtime resolution changes;
- config settings or frontend policy changes;
- mutable source contents change.

Freshness validation has to compare the inputs that determine build dependency selection. If it
compares only package name and version, or only raw requirement names, it can accept stale build
environments.

## Current Lockfile Pressure Points

The current lockfile model has several useful properties that build dependency locking should
preserve:

- package artifacts are globally de-duplicated;
- package records contain provenance and hashes;
- runtime dependency edges are explainable;
- universal runtime forks can share package records;
- frozen replay can reject locks that are missing required information.

Build dependency locking adds pressure to each property:

- a package artifact can be shared while its selected closure differs by build environment;
- build-only packages still need full artifact provenance;
- build-only dependency edges can corrupt runtime traversal if they are treated as ordinary runtime
  edges;
- universal target reachability determines when a build environment is needed;
- skipped, empty, stale, and incomplete build captures must be distinguishable.

These pressures are the reason the representation question is difficult. The problem is not merely
adding a list named `build-dependencies`. The lockfile needs to describe source-build dependency
selection without collapsing isolated build environments into the runtime package graph, while still
sharing the package and artifact catalogue that the current lockfile is built around.

## Constraints For Future Design Work

This document does not choose a design, but it identifies constraints that a design has to satisfy:

- capture build dependencies only for source artifacts that can actually be selected for build;
- preserve artifact identity for source inputs and build-only artifacts;
- account for implicit backend requirements;
- account for hook-discovered requirements;
- preserve source lowering through `tool.uv.sources`, indexes, constraints, and direct source
  configuration;
- represent target reachability, including uv universal fork state;
- support build requirements connected to runtime packages via `match-runtime`;
- keep build-only dependency information from corrupting runtime traversal;
- support packages shared by runtime and one or more build environments;
- support multiple isolated build environments that share a package artifact but select different
  transitive closures;
- distinguish missing, skipped, explicitly empty, and captured-empty build dependency information;
- detect stale build dependency information for mutable source inputs and changed build settings;
- handle nested source builds without replaying unselected fallback sdists;
- preserve artifact hash requirements for build-only wheels and archives;
- reject malformed locks with dangling or incomplete build dependency references.

The solution space includes several possible representations, but the motivation above is
independent of which representation is chosen.
