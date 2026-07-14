# RFC: Multiple Resolutions In One Lockfile Graph

This document explores the generalized edge-selector alternative. The narrower build-lock design
uses ordinary package traversal and `resolution-id` only for conflicting package closures; see
`RFC.md` and `RFC-BUILD-RESOLUTION-ID.md`.

## Summary

Generalize the uv lockfile so it can represent many distinct resolutions in one shared package
graph.

The lockfile already uses this idea for universal runtime resolution: packages are shared nodes,
dependency edges carry marker context, and an install selects the applicable branch of the graph.
This RFC extends that model from runtime forks to all independently resolved contexts, including
isolated build environments.

The core representation is:

- `[[package]]` entries remain the shared catalogue of package identities, artifacts, hashes,
  metadata, and dependency edges.
- `[[resolution]]` entries define runtime fork records and additional named resolution contexts,
  roots, replay mode, and freshness inputs.
- Dependency edges are active in the runtime graph by default. Edges carry typed selectors for
  runtime fork branches and for additional resolutions.

Frozen replay keeps the existing lockfile contract: select a resolution context, traverse the
applicable graph edges from its roots, and install the resulting closure.

## Motivation

The existing runtime lockfile graph is valuable because it preserves dependency provenance. A
package entry records its dependency edges, those edges carry markers, and installation follows the
graph. This is compact, explainable, and consistent with how dependency resolution works.

Locked build environments create the same shape of problem at a larger scope. They are independent
resolutions that can share package artifacts with the runtime graph and with each other:

```text
runtime:
  builder==1 -> helper==2

wheel(dep-a):
  builder==1 -> helper==1

wheel(dep-b):
  builder==1 -> helper==2
```

The artifact for `builder==1` is shared. The selected closure for `builder==1` is not.

A build-specific member-list design solves this by treating build replay as:

```text
install these selected packages
```

This RFC explores the alternative:

```text
select this resolution context and traverse the lockfile graph
```

The goal is to preserve the existing graph-based lockfile design while making the graph capable of
holding multiple independent resolutions.

## Guide-Level Explanation

The lockfile gains first-class resolution records and typed edge selectors.

The runtime graph remains the default graph. A dependency edge without an explicit non-runtime
selector is a runtime edge.

When the runtime resolver forks, each fork is represented as a runtime resolution record:

```toml
[[resolution]]
id = "runtime:project:linux-cpu:<digest>"
kind = "runtime"
target = {
    marker = "sys_platform == 'linux'",
    active = [{ package = "project", extra = "cpu" }],
    inactive = [{ package = "project", extra = "cu124" }],
}
roots = [{ name = "project" }]
```

Those records replace the current `resolution-markers` strings. Runtime edges still do not serialize
`contexts = ["runtime"]`; they use typed target selectors.

Each captured build environment is an additional resolution:

```toml
[[resolution]]
id = "build:dep-a:wheel:<digest>"
kind = "build"
operation = "wheel"
mode = "isolated"
package = { name = "dep-a", version = "0.1.0", source = { directory = "dep-a" } }
artifact = { directory = "dep-a" }
target = { marker = "sys_platform == 'linux'", active = [{ package = "project", extra = "foo" }] }
inputs = { version = 1, hash = "sha256:..." }
roots = [
    { name = "builder", version = "1.0.0" },
    { name = "helper", version = "1.0.0" },
]
```

Package entries still contain dependency edges:

```toml
[[package]]
name = "builder"
version = "1.0.0"
source = { registry = "https://pypi.org/simple" }
dependencies = [
    { name = "helper", version = "2.0.0" },
    { name = "helper", version = "1.0.0", selector = { resolution = "build:dep-a:wheel:<digest>" } },
]
```

To replay the runtime graph, uv starts at the runtime roots and follows edges whose target selectors
match the selected target.

To replay the build environment for `dep-a`, uv starts at that build resolution's roots and follows
edges whose resolution selector matches `build:dep-a:wheel:<digest>`.

Both replays use package dependency edges. Runtime replay selects target selectors. Build replay
selects a named resolution selector and evaluates that build resolution's target selector.

## Reference-Level Explanation

### Terminology

A **package node** is a `[[package]]` entry. It identifies a package distribution and its artifacts.

A **dependency edge** is an edge from one package node to another package node. Edges are guarded by
typed selectors.

A **selector** is a typed predicate that determines whether an edge is active for a traversal.
Selectors include target selectors and resolution selectors.

A **resolution context** is a graph traversal context. Runtime contexts are represented by runtime
fork records. Additional contexts are named and define roots, selectors, replay mode, and freshness
inputs.

A **runtime graph** is the dependency graph used for application installation.

A **build resolution** is a context used to construct an isolated build environment for one exact
source artifact and build operation.

A **target context** is uv's runtime selection context. It includes PEP 508 environment markers and
uv fork state such as extras, groups, and conflict branches.

### Resolution Records

A resolution record owns facts that belong to a whole resolution:

- identity;
- kind;
- roots;
- target context;
- operation kind, for build resolutions;
- exact source artifact, for build resolutions;
- replay mode;
- freshness inputs.

The proposed runtime fork shape is:

```toml
[[resolution]]
id = "runtime:project:linux-cpu:<digest>"
kind = "runtime"
target = {
    marker = "sys_platform == 'linux'",
    active = [{ package = "project", extra = "cpu" }],
    inactive = [{ package = "project", extra = "cu124" }],
}
roots = [{ name = "project" }]
```

Runtime resolution IDs are generated, not authored. The writer canonicalizes the runtime selector
and roots, then emits an ID of the form `runtime:<root-slug>:<target-slug>:<digest>`.

The readable slugs are deterministic projections of the same canonical input:

- `root-slug` is the normalized root package name for a singleton root set, or `multi-root` for a
  multi-root traversal;
- `target-slug` is a stable summary of simple target selector terms, such as `linux-cpu`, or `fork`
  when the selector is complex enough that a short summary would be misleading.

The digest remains the authoritative identity. The digest input includes:

- `kind = "runtime"`;
- the normalized target selector, including PEP 508 marker predicates and uv activation predicates;
- the root package identities for the runtime traversal;
- the lockfile schema version for the selector grammar.

The digest does not include package dependency edges. Edges reference the runtime resolution;
including them in the ID would make the graph self-referential and would churn IDs for unrelated
package metadata changes. Writers sort runtime records by canonical selector and digest before
serialization.

Readers use the digest to validate the record's canonical identity. The readable slugs make
lockfiles easier to inspect, but they do not change selection semantics. A lockfile with two runtime
records that have the same digest and different readable slugs is invalid.

The proposed build shape is:

```toml
[[resolution]]
id = "build:dep-a:wheel:<digest>"
kind = "build"
operation = "wheel"
mode = "isolated"
package = { name = "dep-a", version = "0.1.0", source = { directory = "dep-a" } }
artifact = { directory = "dep-a" }
target = { marker = "sys_platform == 'linux'", active = [{ package = "project", extra = "foo" }] }
inputs = { version = 1, hash = "sha256:..." }
roots = [{ name = "builder", version = "1.0.0" }]
```

Runtime records are emitted when the runtime graph forks. A singleton runtime graph uses the
existing package graph without an explicit runtime record.

### Context Selectors

The current lockfile uses marker strings on dependency edges. In universal resolution, those marker
strings carry more than ordinary PEP 508 environment predicates: uv encodes conflict branches for
extras and groups into the universal marker.

This RFC replaces that wire representation with typed selectors. A dependency edge has a selector,
not only a PEP 508 marker string.

For runtime edges, the selector refers to a runtime fork record. The fork record owns the PEP 508
environment marker and uv activation state:

```toml
[[resolution]]
id = "runtime:project:linux-cpu:<digest>"
kind = "runtime"
target = {
    marker = "sys_platform == 'linux'",
    active = [{ package = "project", extra = "cpu" }],
    inactive = [{ package = "project", extra = "cu124" }],
}

{
    name = "torch",
    version = "2.5.1+cpu",
    selector = { target = "runtime:project:linux-cpu:<digest>" },
}
```

For build edges, the selector includes the build resolution identity:

```toml
{
    name = "helper",
    version = "1.0.0",
    selector = { resolution = "build:dep-a:wheel:<digest>" },
}
```

An edge used in a target-specific build has both selectors:

```toml
{
    name = "helper",
    version = "1.0.0",
    selector = {
        resolution = "build:dep-a:wheel:<digest>",
        target = "runtime:project:linux:<digest>",
    },
}
```

The selector model is typed. Extras and groups are uv activation predicates, not fake PEP 508
extras. Build resolution identity is a graph traversal predicate, not a marker expression.

The target selector representation must preserve the boolean relationship between PEP 508 predicates
and uv activation predicates. A flat shape such as
`{ marker = ..., active = [...], inactive = [...] }` is sufficient only when the marker and
activation predicates form a single conjunction. It cannot represent cases where the activation
predicate varies by platform branch, for example:

```text
(platform_machine == "x86_64" and project[cu124])
or (project[cpu] and project[cu124])
```

In those cases, serializing `marker = ...` separately from `active = [...]` changes the graph.

The general target selector form is a disjunction of conjunctions:

```toml
target = { any-of = [
    { all-of = [
        { marker = "platform_machine == 'x86_64'" },
        { active = { package = "project", extra = "cu124" } },
    ] },
    { all-of = [
        { active = { package = "project", extra = "cpu" } },
        { active = { package = "project", extra = "cu124" } },
    ] },
] }
```

`any-of` is OR. `all-of` is AND. An empty `any-of` is false. An empty `all-of` is true. A term is
one of:

- `marker`, a single PEP 508 marker expression simplified against the lock's `requires-python`;
- `active`, a uv activation predicate for a project, extra, or dependency group;
- `inactive`, the negation of a uv activation predicate.

The flat shape is a shorthand for a single `all-of` clause. Writers use it for simple conjunctions
because it is shorter and easier to read. Writers use `any-of` when the target predicate contains
disjunctions or when PEP 508 marker predicates and uv activation predicates are correlated. A lock
writer must prove that a selector round-trips to the original [`UniversalMarker`] before emitting
the typed form.

### Replacing Universal Solver Tags

The production design replaces the current lockfile use of universal marker strings.

Today, runtime fork state is serialized as marker strings containing encoded extras:

```toml
resolution-markers = [
    "sys_platform == 'linux' and extra == 'extra-7-project-cpu'",
]

dependencies = [
    { name = "torch", marker = "extra == 'extra-7-project-cpu'" },
]
```

Those strings are not ordinary PEP 508 markers in practice. They require uv's private encoding for
projects, extras, groups, and conflict branches.

The proposed lockfile serializes that state as typed runtime fork records and edge selectors:

```toml
[[resolution]]
id = "runtime:project:linux-cpu:<digest>"
kind = "runtime"
target = {
    marker = "sys_platform == 'linux'",
    active = [{ package = "project", extra = "cpu" }],
}

dependencies = [
    { name = "torch", selector = { target = "runtime:project:linux-cpu:<digest>" } },
]
```

`UniversalMarker` remains useful inside the resolver for boolean algebra, reachability, and
simplification. It is no longer the lockfile wire format. The lockfile wire format is typed, so
branch identity is recoverable without decoding synthetic `extra` names.

### Package Records

Package records remain the shared catalogue of distributions and artifacts:

```toml
[[package]]
name = "helper"
version = "1.0.0"
source = { registry = "https://pypi.org/simple" }
wheels = [
    { url = "https://example.invalid/helper-1.0.0-py3-none-any.whl", hash = "sha256:..." },
]
```

Package records also remain the home for dependency edges:

```toml
[[package]]
name = "builder"
version = "1.0.0"
dependencies = [
    { name = "helper", version = "1.0.0", selector = { resolution = "build:dep-a:wheel:<digest-a>" } },
    { name = "helper", version = "2.0.0" },
    { name = "helper", version = "2.0.0", selector = { resolution = "build:dep-b:wheel:<digest-b>" } },
]
```

The same package node can participate in many resolutions. A dependency edge without a resolution
selector is active for runtime traversal. A dependency edge with a resolution selector is active for
the named resolution. If the same dependency edge is needed by runtime and by a named resolution,
the lockfile records two edges rather than serializing a synthetic runtime context.

Package records can also be active in only part of the universal runtime solution. This is package
membership, not a dependency edge. The same typed target selector language applies:

```toml
[[package]]
name = "urllib3"
version = "2.0.7"
source = { registry = "https://pypi.org/simple" }
selectors = [
    { target = { marker = "python_full_version < '3.8'" } },
]
```

When the package membership exactly matches a runtime fork, the package selector may reference that
runtime record by ID:

```toml
selectors = [{ target = "runtime:project:linux-cpu:<digest>" }]
```

When package membership is a projection across several runtime forks, such as all targets where
`sys_platform == 'darwin'` regardless of extra activation, the selector is serialized inline:

```toml
selectors = [{ target = { marker = "sys_platform == 'darwin'" } }]
```

This avoids reintroducing synthetic `resolution-markers` for package entries whose reachability is
broader or narrower than one generated runtime record.

### Graph Traversal

Replay is graph traversal under a selected context.

For the runtime graph, uv starts from the project roots and follows edges whose target selectors
match the selected target.

For a named resolution `R`, uv:

1. Loads `R.roots`.
2. Selects artifacts compatible with the concrete executor.
3. Traverses dependency edges active in `R`.
4. Installs the resulting closure.

For build replay, this constructs the isolated build environment:

```text
select resolution: build:dep-a:wheel:<digest>
start roots: builder==1, helper==1
follow context-active edges
install closure into build environment
run metadata/build hook
```

The build environment is not represented as a member list. It is represented as a named graph
traversal.

### Direct Requirements And Transitive Edges

Resolution roots represent direct requirements:

```toml
[[resolution]]
id = "build:dep-a:wheel:<digest>"
roots = [
    { name = "builder", version = "1.0.0" },
    { name = "helper", version = "1.0.0" },
]
```

Package dependency edges represent transitive dependencies:

```toml
[[package]]
name = "builder"
version = "1.0.0"
dependencies = [
    { name = "leaf", version = "1.0.0", selector = { resolution = "build:dep-a:wheel:<digest>" } },
]
```

This mirrors the existing distinction between root requirements and package metadata dependencies.

### Operation Kinds

Build resolution identity includes operation kind:

```text
wheel(dep)
editable(dep)
sdist(dep)
```

The same source artifact can require different build environments for these operations because the
backend has distinct dependency-discovery hooks:

- `get_requires_for_build_wheel`
- `get_requires_for_build_editable`
- `get_requires_for_build_sdist`

Metadata preparation is not a separate operation. It uses the corresponding `wheel` or `editable`
resolution.

### Build Requirement Capture Stages

PEP 517 build setup has two dependency environments for a given operation:

- `bootstrap`: the environment installed from `build-system.requires` plus uv-specified extra build
  dependencies before calling `get_requires_for_build_*`;
- `build`: the environment installed after adding requirements returned by the backend's
  `get_requires_for_build_*` hook.

Lock generation uses both environments so discovery follows the ordinary frontend sequence. When
their roots and complete closures agree, uv writes one compact, unstaged resolution:

```toml
[[resolution]]
id = "build:dep:wheel:build:<digest>"
kind = "build"
operation = "wheel"
mode = "isolated"
name = "dep"
roots = [
    { name = "setuptools", version = "69.2.0" },
    { name = "wheel", version = "0.43.0" },
]
```

When the stages differ, uv writes paired bootstrap and build records. Frozen replay validates and
installs the bootstrap resolution, runs `get_requires_for_build_*`, validates and installs the final
resolution, and invokes the actual build hook. A valid PEP 517 hook can inspect whether a hook-only
dependency is already installed, so installing the final environment before discovery would change
backend behavior.

### Target Context And Backend Metadata

A build resolution has target context.

The target context identifies the runtime branch that selected the source artifact or
`match-runtime` binding:

```toml
target = {
    marker = "sys_platform == 'linux'",
    active = [{ package = "project", extra = "foo" }],
}
```

Build requirement discovery is treated as stable metadata for the selected source artifact and build
inputs, following the same model uv uses for wheel metadata in universal resolution. For a given
source artifact, build operation, configuration settings, extra build dependencies, sources,
indexes, constraints, and frontend policy, backend-reported build requirements are expected to be
the same regardless of the host that executed the backend hook.

Conditional build requirements are represented with markers inside the requirements and dependency
edges. Those markers are stable metadata and are evaluated against the build environment during
replay. Backend hooks returning different requirement lists on different hosts are outside the
universal lock contract, just as inconsistent wheel metadata across platforms is outside the normal
universal runtime lock contract.

The target context determines whether the resolution is needed. It is not a record of where backend
hooks happened to run while creating the lock.

### Source Artifact Identity

A build resolution references the exact source artifact being built:

```toml
package = { name = "dep", version = "0.1.0", source = { registry = "https://pypi.org/simple" } }
artifact = { url = "https://example.invalid/dep-0.1.0.tar.gz", hash = "sha256:..." }
```

The package reference identifies the package node. The artifact reference identifies the build
input. Registry package source is not enough, because the sdist URL and hash are artifact-level
identity.

### Generated Sdist Pipelines

A source tree can be built through an intermediate sdist:

```text
source tree --build_sdist--> generated archive --build_wheel--> wheel
```

Those are two different PEP 517 operations. The first operation builds an sdist from the source
tree. The second operation builds a wheel from the generated archive, not from the original source
tree. Each operation can have different build requirements, so each operation needs its own build
resolution:

```toml
[[resolution]]
id = "build:dep:sdist:<digest>"
kind = "build"
operation = "sdist"
package = { name = "dep", version = "0.1.0", source = { directory = "dep" } }
artifact = { directory = "dep" }
roots = [{ name = "sdist-helper", version = "1.0.0" }]

[[resolution]]
id = "build:dep:wheel:<digest>"
kind = "build"
operation = "wheel"
package = { name = "dep", version = "0.1.0", source = { directory = "dep" } }
artifact = {
    generated-by = "build:dep:sdist:<digest>",
    filename = "dep-0.1.0.tar.gz",
    hash = "sha256:...",
}
roots = [{ name = "wheel-helper", version = "1.0.0" }]
```

The `package` field identifies the logical package node shared by both operations. The `artifact`
field identifies the actual build input for that operation. For the wheel resolution, the build
input is the exact archive generated by the sdist resolution. Package name and version are not
enough, because the original directory and the generated archive are different inputs.

Replay therefore has to construct the sdist resolution before replaying the wheel resolution that
references it. If the generated archive hash changes, the wheel resolution no longer matches the
build input and must be refreshed.

### Nested Source Builds

If a build resolution selects a source artifact as a dependency, that source artifact has its own
build resolution.

```text
build(dep-a) roots include builder
builder is selected from source
build(builder) is another resolution
```

Replay recursively constructs the nested build resolution before installing the built artifact into
the parent build environment.

The graph must reject cycles in required build resolutions.

### Direct Builds

Direct builds are still resolution records, but they have no dependency roots:

```toml
[[resolution]]
id = "build:dep:wheel:direct"
kind = "build"
operation = "wheel"
mode = "direct"
artifact = { directory = "dep" }
inputs = { version = 1, hash = "sha256:..." }
roots = []
```

Replay validates the direct-build compatibility contract and invokes the direct backend. There is no
isolated dependency closure to traverse.

### Non-Isolated Builds

Non-isolated builds do not have a uv-managed dependency graph. The caller owns the environment
contents.

Strict replay rejects non-isolated build resolutions unless a separate non-reproducible policy is
defined. They cannot be represented as ordinary isolated graph traversals.

## Examples

### Shared Package, Different Build Closures

`dep-a` and `dep-b` both build with `builder==1`, but select different helper versions.

```toml
[[resolution]]
id = "build:dep-a:wheel:<digest-a>"
kind = "build"
operation = "wheel"
mode = "isolated"
package = { name = "dep-a", version = "0.1.0", source = { directory = "dep-a" } }
artifact = { directory = "dep-a" }
roots = [
    { name = "builder", version = "1.0.0" },
    { name = "helper", version = "1.0.0" },
]

[[resolution]]
id = "build:dep-b:wheel:<digest-b>"
kind = "build"
operation = "wheel"
mode = "isolated"
package = { name = "dep-b", version = "0.1.0", source = { directory = "dep-b" } }
artifact = { directory = "dep-b" }
roots = [
    { name = "builder", version = "1.0.0" },
    { name = "helper", version = "2.0.0" },
]

[[package]]
name = "builder"
version = "1.0.0"
dependencies = [
    { name = "helper", version = "1.0.0", selector = { resolution = "build:dep-a:wheel:<digest-a>" } },
    { name = "helper", version = "2.0.0", selector = { resolution = "build:dep-b:wheel:<digest-b>" } },
]
```

`builder==1` is one package node. Its outgoing edges vary by resolution context.

### Runtime And Build Share A Package

The runtime environment uses `builder==1 -> helper==2`. A build environment uses
`builder==1 -> helper==1`.

```toml
[[resolution]]
id = "build:dep-a:wheel:<digest>"
kind = "build"
operation = "wheel"
mode = "isolated"
package = { name = "dep-a", version = "0.1.0", source = { directory = "dep-a" } }
artifact = { directory = "dep-a" }
roots = [{ name = "builder", version = "1.0.0" }]

[[package]]
name = "builder"
version = "1.0.0"
dependencies = [
    { name = "helper", version = "2.0.0" },
    { name = "helper", version = "1.0.0", selector = { resolution = "build:dep-a:wheel:<digest>" } },
]
```

Runtime traversal cannot see the build-only edge. Build traversal cannot see the runtime-only edge.

### Mixed Marker And Fork Selectors

A package can be selected by a marker expression that mixes ordinary PEP 508 predicates with uv fork
state. For example, a PyTorch-style lock can select a CPU wheel on one platform branch and a CUDA
wheel on another, while the fallback branches require both extras to be active.

The existing wire format has to encode uv fork state as fake extras inside the marker string:

```toml
[[package]]
name = "torchvision"
version = "0.20.1"
source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cpu" }
marker = "(python_full_version < '3.13' and platform_machine == 'aarch64' and platform_python_implementation == 'CPython' and sys_platform == 'linux' and extra == 'extra-7-project-cpu') or (python_full_version >= '3.13' and extra == 'extra-7-project-cpu' and extra == 'extra-7-project-cu124') or (platform_machine != 'aarch64' and extra == 'extra-7-project-cpu' and extra == 'extra-7-project-cu124') or (platform_python_implementation != 'CPython' and extra == 'extra-7-project-cpu' and extra == 'extra-7-project-cu124') or (sys_platform == 'darwin' and extra == 'extra-7-project-cpu') or (sys_platform != 'linux' and extra == 'extra-7-project-cpu' and extra == 'extra-7-project-cu124')"
```

The typed selector form keeps the same boolean structure, but separates marker predicates from uv
activation predicates:

```toml
[[package]]
name = "torchvision"
version = "0.20.1"
source = { registry = "https://astral-sh.github.io/pytorch-mirror/whl/cpu" }
selectors = [{ target = { any-of = [
    { all-of = [
        { marker = "python_full_version < '3.13' and platform_machine == 'aarch64' and platform_python_implementation == 'CPython' and sys_platform == 'linux'" },
        { active = { package = "project", extra = "cpu" } },
    ] },
    { all-of = [
        { marker = "python_full_version >= '3.13'" },
        { active = { package = "project", extra = "cpu" } },
        { active = { package = "project", extra = "cu124" } },
    ] },
    { all-of = [
        { marker = "platform_machine != 'aarch64'" },
        { active = { package = "project", extra = "cpu" } },
        { active = { package = "project", extra = "cu124" } },
    ] },
    { all-of = [
        { marker = "platform_python_implementation != 'CPython'" },
        { active = { package = "project", extra = "cpu" } },
        { active = { package = "project", extra = "cu124" } },
    ] },
    { all-of = [
        { marker = "sys_platform == 'darwin'" },
        { active = { package = "project", extra = "cpu" } },
    ] },
    { all-of = [
        { marker = "sys_platform != 'linux'" },
        { active = { package = "project", extra = "cpu" } },
        { active = { package = "project", extra = "cu124" } },
    ] },
] } }]
```

This cannot be represented as one flat marker plus one flat active-extra set. The active extras are
correlated with the marker branches: some branches require only `cpu`, while others require both
`cpu` and `cu124`.

### Target-Specific Match-Runtime

`dep` is selected on two target branches. Its build requirement matches a runtime package that
differs by target branch.

```toml
[[resolution]]
id = "build:dep:wheel:<linux-digest>"
kind = "build"
operation = "wheel"
target = { marker = "sys_platform == 'linux'" }
roots = [{ name = "runtime-helper", version = "1.0.0" }]

[[resolution]]
id = "build:dep:wheel:<darwin-digest>"
kind = "build"
operation = "wheel"
target = { marker = "sys_platform == 'darwin'" }
roots = [{ name = "runtime-helper", version = "2.0.0" }]
```

The target context is part of the resolution identity because it determines the resolved
`match-runtime` binding.

## Relationship To The Build-Lock Implementation

This RFC describes the generalized edge-selector alternative, not the build-lock implementation. The
implementation takes the narrower approach in `RFC.md` and `RFC-BUILD-RESOLUTION-ID.md`:

- build-resolution records identify the direct staged requirements;
- ordinary dependency edges describe each selected closure;
- an optional `resolution-id` extends package identity only when two closures need different edges
  for the same package artifact.

For example, if the runtime graph and a build graph select different transitive dependencies for
`builder==1`, the build graph receives a scoped package node instead of adding selectors to the
shared node's edges:

```toml
[[package]]
name = "builder"
version = "1.0.0"
dependencies = [{ name = "leaf", version = "2.0.0" }]

[[package]]
name = "builder"
version = "1.0.0"
resolution-id = "build:dep:wheel:build:<digest>"
dependencies = [{ name = "leaf", version = "1.0.0" }]
```

The build resolution references the scoped node from its roots. Replay then follows ordinary
dependency edges, with no second edge-activation authority and no serialized executor context.
Compatible nodes remain shared; only conflicting closures are split.

The generalized selector model remains useful for exploring future independent resolution contexts,
but it should not be confused with the current build-lock wire format or replay contract.

## Validation

An implementation of the generalized selector model would need to validate both graph structure and
resolution coverage.

Structural validation would reject:

- dangling package references;
- ambiguous package references;
- missing source artifacts;
- missing hashes for artifacts that require hashes;
- duplicate resolution IDs;
- incompatible selector or resolution-record shapes;
- runtime resolution records declaring build-only fields such as operation, replay mode, source
  package, or roots;
- build resolution records missing an operation, replay mode, or source package;
- build resolution roots that disagree with the source package's `build-dependencies`;
- dependency edges referencing unknown resolution selectors, referencing a non-build resolution as a
  build traversal context, or declaring a build context that does not contain the package carrying
  the edge;
- scoped build edge data whose package entries or edge selectors are not owned by the source
  package's build resolution;
- target selectors that reference unknown packages, undeclared extras, groups, projects, or conflict
  branches;
- resolution selectors that refer to incompatible package artifacts;
- cycles in required nested build resolutions.

Coverage validation would reject:

- a requested runtime branch with no matching runtime resolution record;
- a requested build operation with no matching build resolution;
- a build resolution whose graph traversal reaches a source artifact without a matching nested build
  resolution.

Freshness validation would compare resolution inputs against current inputs:

- source artifact identity;
- build backend identity;
- operation kind;
- target context;
- build configuration;
- sources, indexes, constraints, and artifact-selection policy.

## Drawbacks

The design is broader than build dependency locking. It changes the lockfile from "one runtime graph
with marker strings" to "one graph with typed resolution and target selectors."

Every dependency edge needs selector semantics. The default runtime selector is compact, but uv
still has to type, validate, simplify, and serialize target selectors and resolution selectors.

Build resolution metadata still needs a first-class home. Even with graph replay, a build resolution
must record source artifact identity, operation kind, target context, direct mode, and freshness
inputs.

The model also increases validation complexity. uv must prove that each named resolution has
complete edge coverage and cannot accidentally traverse edges from another resolution.

Replacing the current universal marker wire format is a migration cost. The internal
`UniversalMarker` algebra remains a useful implementation detail for solving and simplification, but
`extra == 'extra-7-project-cpu'` style values are no longer serialized as the lockfile
representation of uv fork state.

## Rationale And Alternatives

### Captured Build Member Lists

The competing design records the selected packages for each build environment:

```toml
[[build.environment]]
packages = [
    { name = "builder", version = "1.0.0" },
    { name = "helper", version = "1.0.0" },
]
```

This is simple replay: install exactly those packages.

The drawback is that it creates a build-specific replay contract different from the existing runtime
lockfile contract. Dependency provenance becomes optional metadata instead of the authority used for
replay.

### Synthetic Extras

Another alternative is to keep encoding resolution context and uv activation state with fake extras:

```toml
{ name = "helper", version = "1.0.0", marker = "extra == 'build-dep-a-wheel'" }
```

This reuses marker algebra, but it treats build context, project activation, extras, groups, and
conflict branches as PEP 508 extras. The multi-resolution graph keeps selectors typed and explicit.

### Separate Build Lockfile

Build resolutions can live in a separate lockfile. That keeps the runtime lockfile smaller and
avoids changing its graph model.

The problem is that runtime selection determines which build resolutions are required.
`--no-binary`, target branches, sources, `match-runtime`, and nested source dependencies all couple
runtime and build resolution. A separate file would need strong references back into `uv.lock`.

### Per-Build Graphs

Another design stores a full graph under each build record. That keeps build edges local and avoids
resolution selectors on global edges.

It duplicates graph structure when many resolutions share packages and edges. The multi-resolution
graph stores shared package nodes once and lets context selectors distinguish selected edges.

## Unresolved Questions

1. Should resolution selectors be serialized only as named IDs, or should the lockfile also support
   structural predicates for compression?
2. Should `any-of` / `all-of` selectors be compressed when several records share common terms, or
   should the lockfile always preserve the expanded DNF form?
3. Which internal boundary owns conversion between `UniversalMarker` and the typed lockfile selector
   representation?
4. How should the lockfile compress edges shared by many resolution records?
5. How should direct build resolutions and non-isolated build policies appear in the graph model?
6. What compatibility fence prevents older uv versions from ignoring `[[resolution]]` records or
   resolution selectors and resolving build dependencies outside the lockfile?

## Future Possibilities

A typed selector model can support other isolated resolution contexts, such as tool bootstrap
environments or plugin environments, without adding a new lockfile subsystem for each one.
