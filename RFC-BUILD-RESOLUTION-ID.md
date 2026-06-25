# RFC: Build Resolution IDs In Package Identity

## Summary

Add an optional `resolution-id` field to package identity in `uv.lock`.

The field is used only when the same package name, version, and source must appear more than once in
the lockfile because different build dependency resolutions require different dependency edges,
metadata, or artifact selection for that package.

This preserves the current package graph model for the common case:

- packages remain `[[package]]` records;
- dependency edges remain package-to-package edges;
- package artifacts and hashes stay in package records;
- runtime packages do not gain build-only edges unless those edges are compatible with the shared
  package node.

The new field acts as a narrow namespace for conflicting build-resolution nodes. It avoids encoding
build environments as runtime forks, and it avoids adding selector context to every dependency edge.

## Motivation

Build dependency locking has to represent isolated build environments that can coexist in one sync.
These environments are not runtime forks. Runtime forks are mutually exclusive branches of one
runtime installation. Build dependency resolutions are separate isolated closures that can all be
needed during the same operation.

At the same time, build dependency resolutions should reuse the main lockfile catalogue whenever
possible. If the runtime graph and a build environment both select the same package artifact with
compatible metadata and dependency edges, the lockfile should not duplicate that package entry.

The hard case is when reuse is valid at the artifact level but not at the dependency-closure level.

For example:

```text
runtime:
  setuptools==69.0.0 -> helper==2

wheel(dep-a):
  setuptools==69.0.0 -> helper==1

wheel(dep-b):
  setuptools==69.0.0 -> helper==2
```

All three contexts can use the same `setuptools==69.0.0` wheel. They cannot all use the same
`setuptools` package node if that node has one global set of dependency edges.

If the lockfile stores only one package node:

```toml
[[package]]
name = "setuptools"
version = "69.0.0"
dependencies = [
    { name = "helper", version = "1.0.0" },
    { name = "helper", version = "2.0.0" },
]
```

then the package graph no longer describes any single environment. Runtime replay can see build-only
edges. `wheel(dep-a)` can see runtime or `dep-b` edges. The selected closure is no longer
recoverable from ordinary dependency traversal.

A full multi-resolution graph solves this by putting resolution selectors on edges. That is general,
but it is also invasive. The `resolution-id` approach keeps ordinary dependency traversal and splits
package nodes only when a conflict makes sharing unsafe.

## Guide-Level Explanation

Package identity remains unchanged in the common case:

```toml
[[package]]
name = "setuptools"
version = "69.0.0"
source = { registry = "https://pypi.org/simple" }
dependencies = [
    { name = "helper", version = "2.0.0" },
]
```

If a build dependency resolution needs the same package with different dependency edges, the
lockfile can add a second package record with a `resolution-id`:

```toml
[[package]]
name = "setuptools"
version = "69.0.0"
source = { registry = "https://pypi.org/simple" }
resolution-id = "build:dep-a:wheel:<digest>"
dependencies = [
    { name = "helper", version = "1.0.0", resolution-id = "build:dep-a:wheel:<digest>" },
]
```

The `resolution-id` extends package identity. These two records are distinct package nodes:

```text
setuptools==69.0.0
setuptools==69.0.0 @ build:dep-a:wheel:<digest>
```

They can reference the same artifact:

```toml
wheels = [
    { url = "https://example.test/setuptools-69.0.0-py3-none-any.whl", hash = "sha256:..." },
]
```

but they do not share dependency edges.

Build roots enter the appropriate namespace:

```toml
build-dependencies = [
    { name = "setuptools", version = "69.0.0", resolution-id = "build:dep-a:wheel:<digest>" },
]
```

From there, replay can use ordinary package graph traversal. It does not need edge selectors for
this case:

```text
root: setuptools==69.0.0 @ build:dep-a
follow ordinary dependency edges
select helper==1 @ build:dep-a
```

When there is no conflict, the build root can point at the shared package node:

```toml
build-dependencies = [
    { name = "setuptools", version = "69.0.0" },
]
```

This keeps the lockfile small when build dependency closures are compatible with the main runtime
graph or with each other.

## Reference-Level Explanation

### Package Identity

Today, a lockfile package identity is effectively:

```text
name + version + source
```

This RFC extends package identity to:

```text
name + version + source + optional resolution-id
```

When `resolution-id` is absent, the package record is the ordinary shared package node. Runtime
packages and compatible build packages use this form.

When `resolution-id` is present, the package record is scoped to one build dependency resolution. It
can duplicate the package name, version, and source of another package record without being a
duplicate package error.

### Resolution ID

A `resolution-id` names the isolated build dependency resolution that owns a scoped package node.

The ID is generated by uv, not authored by users. It should be stable for the build dependency
resolution identity, including:

- source package name and source artifact identity;
- build operation, such as `wheel`, `editable`, or `sdist`;
- requirement-discovery stage, if stages are represented separately;
- target reachability for the source package;
- build settings, extra build dependencies, sources, indexes, constraints, and frontend policy that
  affect build dependency selection.

The exact digest input is a format detail, but the ID has to distinguish build dependency
resolutions whose package graph closures are not interchangeable.

### When To Split A Package Node

The lock writer should reuse the unscoped package node when the build dependency resolution selects
a package with the same identity and compatible lockfile data.

Compatible means the shared package node has the same selected artifact set and the same dependency
edges needed by the build resolution, after markers and source identity are considered.

The writer creates a scoped package node when sharing would change the meaning of either
environment. Common reasons include:

- different dependency edges for the same package name, version, and source;
- different versions of a transitive dependency selected through the same parent package;
- different package metadata for a mutable or build-only local source package;
- different artifact selection for the same package identity in a build environment;
- dependency markers that are valid only within one build dependency resolution.

The writer does not need to split a package merely because it appears in a build dependency
environment. Splitting is for conflicts.

### Dependency References

Dependency references continue to use the existing package reference shape in the common case:

```toml
dependencies = [
    { name = "helper", version = "2.0.0" },
]
```

When the target package node is scoped, the dependency reference includes the same `resolution-id`:

```toml
dependencies = [
    { name = "helper", version = "1.0.0", resolution-id = "build:dep-a:wheel:<digest>" },
]
```

The `resolution-id` is part of the target package reference. It does not mean the dependency edge is
conditionally active. It means the edge points to a different package node.

This is the main distinction from edge selector designs. A selector says:

```text
this edge is active in resolution R
```

`resolution-id` says:

```text
this edge points to package node P in resolution namespace R
```

### Build Roots

Build dependency roots must identify the package node to traverse.

If the root package is unscoped, the root can omit `resolution-id`:

```toml
build-dependencies = [
    { name = "setuptools", version = "69.0.0" },
]
```

If the root package is scoped, the root includes `resolution-id`:

```toml
build-dependencies = [
    { name = "setuptools", version = "69.0.0", resolution-id = "build:dep-a:wheel:<digest>" },
]
```

The root list still needs surrounding build metadata somewhere in the lockfile: which source
artifact and build operation these roots satisfy, which target branches need the build, and which
inputs determine freshness. This RFC only defines the package identity mechanism used when those
roots enter the package graph.

### Artifact Reuse

Scoped package nodes can duplicate artifact metadata.

```toml
[[package]]
name = "setuptools"
version = "69.0.0"
source = { registry = "https://pypi.org/simple" }
wheels = [
    { url = "https://example.test/setuptools-69.0.0-py3-none-any.whl", hash = "sha256:..." },
]
dependencies = [
    { name = "helper", version = "2.0.0" },
]

[[package]]
name = "setuptools"
version = "69.0.0"
source = { registry = "https://pypi.org/simple" }
resolution-id = "build:dep-a:wheel:<digest>"
wheels = [
    { url = "https://example.test/setuptools-69.0.0-py3-none-any.whl", hash = "sha256:..." },
]
dependencies = [
    { name = "helper", version = "1.0.0", resolution-id = "build:dep-a:wheel:<digest>" },
]
```

This is deliberately redundant. The lockfile already uses package records as the unit that owns
metadata and dependency edges. The scoped node can reuse the same artifact bytes through the same
URL and hash while owning different edges.

If duplication becomes too noisy, artifact factoring can be considered separately. It is not
required for the correctness model.

## Examples

### Compatible Build Closure

Runtime and build both select the same closure:

```text
runtime:
  setuptools==69.0.0 -> helper==2

wheel(dep):
  setuptools==69.0.0 -> helper==2
```

The build root can reuse the ordinary package node:

```toml
[[package]]
name = "setuptools"
version = "69.0.0"
dependencies = [
    { name = "helper", version = "2.0.0" },
]

build-dependencies = [
    { name = "setuptools", version = "69.0.0" },
]
```

No `resolution-id` is needed.

### Conflicting Build Closure

Runtime and build select the same package artifact with different transitive dependencies:

```text
runtime:
  setuptools==69.0.0 -> helper==2

wheel(dep):
  setuptools==69.0.0 -> helper==1
```

The build package node is scoped:

```toml
[[package]]
name = "setuptools"
version = "69.0.0"
dependencies = [
    { name = "helper", version = "2.0.0" },
]

[[package]]
name = "setuptools"
version = "69.0.0"
resolution-id = "build:dep:wheel:<digest>"
dependencies = [
    { name = "helper", version = "1.0.0", resolution-id = "build:dep:wheel:<digest>" },
]

build-dependencies = [
    { name = "setuptools", version = "69.0.0", resolution-id = "build:dep:wheel:<digest>" },
]
```

Runtime traversal sees `helper==2`. Build traversal starts at the scoped `setuptools` node and sees
`helper==1`.

### Two Build Resolutions Conflict With Each Other

Two source packages require the same `setuptools` artifact but different helper versions:

```text
wheel(dep-a):
  setuptools==69.0.0 -> helper==1

wheel(dep-b):
  setuptools==69.0.0 -> helper==2
```

The lockfile can keep two scoped package nodes:

```toml
[[package]]
name = "setuptools"
version = "69.0.0"
resolution-id = "build:dep-a:wheel:<digest-a>"
dependencies = [
    { name = "helper", version = "1.0.0", resolution-id = "build:dep-a:wheel:<digest-a>" },
]

[[package]]
name = "setuptools"
version = "69.0.0"
resolution-id = "build:dep-b:wheel:<digest-b>"
dependencies = [
    { name = "helper", version = "2.0.0", resolution-id = "build:dep-b:wheel:<digest-b>" },
]
```

The builds are not modeled as mutually exclusive forks. They are two coexisting isolated closures.

### Shared Transitive Package

Only the conflicting node needs to be scoped. If both build resolutions use the same `helper==2`
node with the same metadata and edges, that dependency can remain unscoped:

```toml
[[package]]
name = "helper"
version = "2.0.0"

[[package]]
name = "setuptools"
version = "69.0.0"
resolution-id = "build:dep-a:wheel:<digest-a>"
dependencies = [
    { name = "helper", version = "2.0.0" },
]

[[package]]
name = "setuptools"
version = "69.0.0"
resolution-id = "build:dep-b:wheel:<digest-b>"
dependencies = [
    { name = "helper", version = "2.0.0" },
]
```

The scoped parent nodes can point at an unscoped shared child when the child is compatible.

## Interaction With Runtime Forks

`resolution-id` is not a runtime fork marker.

Runtime forks describe mutually exclusive branches of one runtime install. `resolution-id` describes
a package node scoped to an isolated build dependency resolution. Several scoped build nodes can be
used during one sync.

Build dependency solves should still use runtime fork reachability from the main resolution. If a
source package is selected only on Linux, its build dependency resolution is only needed on Linux.
Runtime fork markers determine when the build root is needed; they do not make the build closure a
mutually exclusive runtime branch.

The main runtime resolution can be used as preferences for build dependency solves. This improves
reuse:

```text
runtime selected helper==2
build allows helper>=1
prefer helper==2
```

If the resulting build closure is compatible with the unscoped package graph, no scoped nodes are
needed. If the build closure diverges, `resolution-id` scopes only the packages that need different
graph data.

## Validation

The lockfile parser and freshness checks need to enforce these invariants:

- package identity includes `resolution-id`;
- unscoped duplicate package identities remain invalid;
- scoped duplicates are allowed only when `resolution-id` differs;
- dependency references with `resolution-id` must resolve to an existing scoped package node;
- dependency references without `resolution-id` must resolve to the unscoped package node;
- build roots must identify an existing package node, scoped or unscoped;
- a scoped package node must be reachable from a build dependency root or from another scoped
  package node in the same build dependency resolution;
- scoped package nodes must not be installed into the runtime environment unless they are explicitly
  referenced by a runtime package reference, which should normally be rejected;
- artifact hashes and source identity rules are the same for scoped and unscoped package nodes;
- freshness validation must include the inputs that define the `resolution-id`.

## Benefits

This design is narrower than a full multi-resolution graph:

- it keeps dependency edges ordinary;
- it avoids adding selectors to every edge;
- it preserves artifact reuse when closures are compatible;
- it duplicates package nodes only when graph data conflicts;
- it makes build resolutions coexist without pretending they are runtime forks.

It also fits the current lockfile framing. Package records remain the unit of metadata, artifacts,
hashes, and dependency edges. `resolution-id` only extends the identity of package records that
cannot safely be shared.

## Drawbacks

The same artifact metadata can be duplicated across scoped package nodes. Large projects with many
conflicting build resolutions can therefore produce larger lockfiles.

The lockfile still needs a place to record build-operation metadata: source artifact identity,
operation kind, target reachability, build inputs, and freshness. `resolution-id` solves package
graph ambiguity; it does not by itself describe why a build resolution exists or when it is needed.

The writer has to decide when a package node is compatible enough to reuse. Over-splitting is
correct but noisy. Under-splitting corrupts replay.

The scoped package graph can contain duplicate package names and versions, which makes lockfile
inspection more complex. Tooling must display `resolution-id` when it matters.

## Alternatives

### Always Duplicate Build Packages

The writer can put every build dependency package into a scoped namespace. That is simpler to reason
about, but it loses reuse in the common compatible case and makes lockfiles larger than necessary.

### Edge Selectors

A full multi-resolution graph can keep one package node and attach resolution selectors to
dependency edges.

That preserves package-node deduplication more aggressively, but it requires every graph traversal
to understand edge selectors. The `resolution-id` design instead preserves ordinary graph traversal
by splitting nodes only when needed.

### Member Lists

The lockfile can store each build dependency environment as an explicit list of selected package
members and avoid graph traversal for build replay.

That directly records the closure, but it creates a build-specific replay model and loses
dependency-edge provenance unless extra graph data is stored for diagnostics.

### Treat Build Environments As Runtime Forks

Build environments can be encoded as synthetic fork markers or extras.

This is the wrong semantic model. Runtime forks are mutually exclusive branches. Build dependency
resolutions are independent isolated closures that can coexist in one operation.

## Unresolved Questions

1. Where should build-operation metadata live if `resolution-id` is only a package identity
   extension?
2. Should `resolution-id` be allowed on runtime package references at all, or should scoped package
   nodes be build-only by construction?
3. How should the writer determine package-node compatibility without creating surprising lockfile
   churn?
4. Should scoped nodes duplicate artifact metadata, or should a later format factor artifacts out of
   package records?
5. How should `resolution-id` interact with package metadata exported to formats that do not
   understand scoped package identities?
