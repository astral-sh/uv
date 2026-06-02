# RFC: First-Class Locked Build Environments

## Summary

Add first-class build records to the uv lockfile.

A build record describes the environment required to perform one build
operation for one exact source artifact. It references ordinary locked
packages, but it is not part of the runtime dependency graph.

This proposal separates concepts with different semantics:

- `[[package]]` records describe reusable distributions, artifacts, hashes,
  and runtime dependency edges.
- `[[build]]` records describe build operations for source artifacts.
- `[[build.environment]]` records describe resolved environments captured for
  particular target-runtime and build-executor contexts.

The schema supports the `wheel`, `editable`, and `sdist` operations. Capture is
demand-driven: a command records the operations required by its replay
contract.

Lock generation performs build-requirement discovery and records the complete
resolved environment. Frozen replay installs that recorded environment
directly. It does not resolve build requirements or rerun dependency-discovery
hooks.

This guarantees that build-dependency selection comes from the lockfile. It
does not claim that arbitrary backend execution is hermetic or that two builds
produce byte-identical artifacts.

## Motivation

Python source builds run in isolated environments. A build backend declares
initial requirements through `[build-system].requires` and can return
additional requirements from a dependency-discovery hook.

PEP 517 defines distinct discovery hooks for wheel and sdist builds:

- `get_requires_for_build_wheel`
- `get_requires_for_build_sdist`

PEP 660 adds a distinct discovery hook for editable builds:

- `get_requires_for_build_editable`

The requirements returned for these operations can differ. The operation is
therefore part of the identity of a captured build environment.

For a frozen replay to be reproducible with respect to dependency selection,
every package installed into a build environment must come from the lockfile.
Resolving only runtime dependencies is not sufficient.

Build environments cannot be represented as additions to the runtime package
graph. Two source builds can select different valid closures for the same
package:

```text
dep-a wheel-build requirements: builder==1, helper==1
dep-b wheel-build requirements: builder==1, helper==2
```

If `builder==1` declares `helper>=1`, both isolated environments are valid:

```text
wheel(dep-a):
  builder==1
  helper==1

wheel(dep-b):
  builder==1
  helper==2
```

The runtime environment can select a third closure:

```text
runtime:
  builder==1
  helper==2
```

The distribution artifact for `builder==1` can be shared across all three
contexts. Its selected environment cannot be shared. Adding both build-time
edges to the global package record would incorrectly describe:

```text
builder==1 -> helper==1
builder==1 -> helper==2
```

That graph is not the runtime environment, `wheel(dep-a)`, or `wheel(dep-b)`.

A lockfile therefore needs to share package artifacts without merging
independently resolved build environments.

## Guide-Level Explanation

When locking a project, uv identifies the source artifacts and build operations
required by the command's reproducibility contract. For each operation, uv
determines the complete environment required to execute it.

For example, given:

```text
dep-a wheel-build requirements:
  builder==1
  helper==1
```

the lockfile contains ordinary package records for `builder` and `helper`, plus
a build record for the wheel operation on `dep-a`:

```toml
[[package]]
name = "builder"
version = "1.0.0"
source = { registry = "https://pypi.org/simple" }
wheels = [
    { url = "https://example.invalid/builder-1.0.0-py3-none-any.whl", hash = "sha256:..." },
]

[[package]]
name = "helper"
version = "1.0.0"
source = { registry = "https://pypi.org/simple" }
wheels = [
    { url = "https://example.invalid/helper-1.0.0-py3-none-any.whl", hash = "sha256:..." },
]

[[build]]
package = { name = "dep-a", version = "0.1.0", source = { directory = "dep-a" } }
artifact = { directory = "dep-a" }
kind = "wheel"
mode = "isolated"
inputs = { version = 1, hash = "sha256:..." }

[[build.environment]]
packages = [
    { name = "builder", version = "1.0.0" },
    { name = "helper", version = "1.0.0" },
]
```

The build record does not add dependency edges to the global `builder` package
record. It records the resolved members of one isolated environment.

If another source artifact requires `helper==2`, it gets a separate build
record:

```toml
[[build]]
package = { name = "dep-b", version = "0.1.0", source = { directory = "dep-b" } }
artifact = { directory = "dep-b" }
kind = "wheel"
mode = "isolated"
inputs = { version = 1, hash = "sha256:..." }

[[build.environment]]
packages = [
    { name = "builder", version = "1.0.0" },
    { name = "helper", version = "2.0.0" },
]
```

During frozen replay, uv selects the build record for the exact source artifact
and operation, installs the recorded package members into an isolated
environment, and invokes the downstream metadata or artifact-production hook.

The lockfile records the result of build-requirement discovery. Frozen replay
does not perform dependency resolution and does not rerun dependency-discovery
hooks.

## Reference-Level Explanation

### Terminology

A **package record** is an ordinary `[[package]]` entry. It identifies a
distribution and its installable artifacts. It contains runtime dependency
edges when the locked package has them.

A **source artifact** is an exact input that requires a build before it can be
installed, such as an sdist, source archive, Git checkout, or source directory.

A **build operation** is one of:

| Kind | Discovery hook | Downstream hooks |
| --- | --- | --- |
| `wheel` | `get_requires_for_build_wheel` | `prepare_metadata_for_build_wheel`, `build_wheel` |
| `editable` | `get_requires_for_build_editable` | `prepare_metadata_for_build_editable`, `build_editable` |
| `sdist` | `get_requires_for_build_sdist` | `build_sdist` |

Metadata preparation is not a separate operation kind. It uses the environment
captured for the corresponding wheel or editable operation.

A **build environment** is the complete set of distributions installed before
invoking the downstream hooks for one operation.

A **target-runtime context** is uv's runtime selection context for a branch of
the universal resolution. It includes ordinary environment markers and uv
fork context such as extras, groups, and conflict branches.

A **build-executor context** is the interpreter, host, tag, ABI, libc, and
frontend context used to execute backend hooks.

A **build variant** is a captured build environment for a particular
combination of:

- exact source artifact;
- build operation;
- target-runtime context;
- build-executor context;
- resolved `match-runtime` bindings, when present.

A **build record** is a `[[build]]` entry grouping variants for one source
artifact and operation.

### Package Records

Package records continue to serve as the globally deduplicated catalogue of
distributions and artifacts:

```toml
[[package]]
name = "helper"
version = "1.0.0"
source = { registry = "https://pypi.org/simple" }
wheels = [
    { url = "https://example.invalid/helper-1.0.0-py3-none-any.whl", hash = "sha256:..." },
]
```

Build-only packages use the same package representation as runtime packages.
Local wheel artifacts, including build-only local wheels, retain the same hash
requirements as equivalent runtime artifacts.

Package records do not contain build-only dependency edges. A package used in
both runtime and build contexts keeps its runtime edges in the package record
and appears as a member of each applicable build environment.

References from build records use the same package-reference semantics as
ordinary lockfile edges. They include source identity when needed and omit
fields only when the result remains unambiguous. Readers reject ambiguous
references.

### Source Artifact Identity

A package record does not always identify one exact source artifact.

For example, a registry package's source identifies its index, while an sdist
artifact has its own URL and hash. A build record therefore identifies both:

- the package record that owns the artifact;
- the exact source artifact used as the build input.

The proposed TOML shape is illustrative:

```toml
[[build]]
package = { name = "dep", version = "0.1.0", source = { registry = "https://pypi.org/simple" } }
artifact = { url = "https://example.invalid/dep-0.1.0.tar.gz", hash = "sha256:..." }
kind = "wheel"
mode = "isolated"
inputs = { version = 1, hash = "sha256:..." }
```

Artifact references are canonical and portable:

- immutable archives include their URL and hash;
- Git sources include an exact revision and subdirectory, when present;
- mutable directories include a portable path plus the freshness identity
  required by the selected mutable-source policy;
- generated artifacts include their provenance and content identity.

If the wire format omits `artifact`, the package record must have exactly one
unambiguous buildable source form and readers must validate that invariant.
The semantic identity still includes the exact artifact.

### Build Records

A build record identifies:

- a package record;
- an exact source artifact;
- an operation kind;
- an execution mode;
- common freshness inputs;
- one or more captured variants.

The proposed TOML shape is:

```toml
[[build]]
package = { name = "dep", version = "0.1.0", source = { directory = "dep" } }
artifact = { directory = "dep" }
kind = "wheel"
mode = "isolated"
inputs = { version = 1, hash = "sha256:..." }

[[build.environment]]
target = "sys_platform == 'win32'"
host = "sys_platform == 'darwin'"
context = { implementation = "cpython", python = "3.12", platform = "macos" }
packages = [
    { name = "builder", version = "1.0.0" },
]
```

The exact TOML spelling is an open design choice. The required semantic fields
are not.

The `target` selector identifies the runtime-resolution branches that require
this variant. It is a uv runtime fork context, not just a PEP 508 marker. It
must preserve the fork information that can affect source selection and
`match-runtime` bindings, including environment markers, extras, groups, and
conflict branches.

The `host` selector and `context` fields identify the build executor for which
the capture was produced. PEP 508 markers are useful selectors, but they do not
fully describe interpreter ABI, platform tags, libc policy, or other facts
that affect artifact compatibility and backend behavior.

Resolved `match-runtime` bindings are target-dependent. They belong to the
variant's target-runtime context and must not be encoded as build-host markers.
If the schema serializes bindings explicitly, they are diagnostic and
validation metadata, not replay authority.

Variants can be coalesced only when their recorded environments and relevant
inputs are identical.

### Captured Environment Membership

Each `[[build.environment]]` entry records the complete set of locked package
members to install before invoking downstream hooks.

The ordinary lockfile dependency graph remains the right model for runtime
installation. Runtime installs start from requested roots, traverse dependency
edges, apply markers and forks, and construct one selected environment. That
edge graph is compact, explainable, and preserves dependency provenance.

Build replay has a different authority boundary. A build variant is the result
of a separate isolated resolution, and that resolution can select a different
closure for the same package than the runtime graph or another build variant.
The global package entry for `builder==1` cannot simultaneously own:

```text
runtime(builder) -> helper==2
wheel(dep-a):    builder -> helper==1
wheel(dep-b):    builder -> helper==2
```

If frozen replay uses ordinary package-edge traversal, those build-specific
edges must be represented somewhere. Putting them on the global package entry
leaks isolated build decisions into runtime traversal. Putting scope on every
edge works only if the source artifact, operation kind, target-runtime context,
and build-executor context are propagated through the entire transitive build
closure.

For frozen replay, the semantic output of resolution is the selected member
set. The replay contract is therefore:

```text
install exactly these locked packages into the isolated build environment
```

not:

```text
recover the build environment by traversing the global runtime graph
```

This does not reject dependency edges as useful data. Build-local edges remain
valid provenance and can support diagnostics, explanations, compression, and
resolver preferences. They belong under the build variant and cannot define
runtime edges.

The lockfile can retain normalized requirement provenance for diagnostics:

- bootstrap requirements;
- hook-added requirements;
- configured extra build requirements;
- resolved `match-runtime` bindings.

These fields are not replay authority. If retained, their role and
normalization must be explicit. The `packages` member list defines replay.

The lockfile can retain independent build-local dependency edges for
diagnostics or resolver preferences. Those edges are supplementary. They do
not change replay semantics and are not merged into the runtime graph.

### Lock Generation

For each source artifact and operation requiring capture, lock generation:

1. Lowers `[build-system].requires` using the same sources, indexes, and
   constraints as an ordinary build.
2. Selects the implicit default backend when the source omits
   `[build-system]`.
3. Adds configured extra build requirements, including resolved
   `match-runtime` bindings.
4. Resolves and installs that bootstrap environment.
5. Runs the applicable dependency-discovery hook:
   `get_requires_for_build_wheel`, `get_requires_for_build_editable`, or
   `get_requires_for_build_sdist`.
6. Lowers the hook-added requirements with the same source semantics.
7. Resolves the complete final environment.
8. Records the selected package members and freshness inputs in a build
   variant.

Directory, archive, direct URL, and Git sources use the same capture contract.
Static metadata extraction can avoid unnecessary metadata builds, but it must
not omit dependency discovery required to capture a complete build
environment.

Resolution work can be cached. Cached results must be projected into each
build variant with that variant's target-runtime and build-executor context.

### Demand-Driven Coverage

The schema supports all three operation kinds. Capture is demand-driven.

Project sync requires:

- `wheel` records for source artifacts installed non-editably;
- `editable` records for source artifacts installed editably.

Reproducible `uv build` support additionally requires:

- `sdist` records when producing source distributions;
- `wheel` records when producing wheels.

A project-sync-only version of the design captures `wheel` and `editable`, and
rejects an uncovered `sdist` operation rather than resolving outside the
lockfile.

Coverage is recursive. If an environment member is itself selected from a
source artifact, replay requires an applicable build record for that nested
artifact and operation.

### Generated Sdist Pipelines

A frontend can build an sdist and then build a wheel from the unpacked,
generated sdist.

That pipeline contains two operations with different exact inputs:

```text
sdist(source tree) -> generated archive
wheel(generated archive) -> wheel
```

Reproducible replay of this pipeline requires:

- a source-tree `sdist` record;
- a wheel record whose artifact identity refers to the generated sdist;
- a provenance and content-identity rule for that generated sdist.

This is required before locked build replay can cover an sdist-to-wheel
pipeline. It is not implied by package-level identity alone.

### Frozen Replay

When frozen replay needs to build a source artifact, it:

1. Looks up the record for the exact artifact and operation kind.
2. Selects the variant matching the target-runtime and build-executor
   contexts.
3. Validates the record's structural integrity and referenced artifacts.
4. Recursively builds source-selected environment members as required.
5. Installs the recorded package members into an isolated environment.
6. Invokes the downstream metadata or artifact-production hook.

Frozen replay must not resolve build requirements outside the lockfile.

Frozen replay must not rerun dependency-discovery hooks for a captured
environment. Discovery hooks determine environment membership and belong to
lock generation. Downstream metadata and artifact-production hooks consume the
captured final environment.

This is intentionally not a faithful replay of the ordinary frontend call
sequence. It is a dependency-environment replay contract.

Backend execution can still observe ambient environment variables, the
network, filesystem state, and time unless separately constrained. This RFC
does not claim byte-identical artifact reproduction or offline backend
execution.

### Target and Build-Executor Contexts

Build variants distinguish target-runtime context from build-executor context.

Target-runtime context answers:

```text
For which runtime resolution branches is this source artifact and environment
selected?
```

Build-executor context answers:

```text
For which interpreter, host, and artifact-compatibility profile was this
environment captured?
```

These domains are not interchangeable. A source package selected only for a
Windows target can be built on a Darwin host when the backend supports that
cross-build configuration:

```toml
[[build]]
package = { name = "dep", version = "0.1.0", source = { directory = "dep" } }
artifact = { directory = "dep" }
kind = "wheel"
mode = "isolated"
inputs = { version = 1, hash = "sha256:..." }

[[build.environment]]
target = "sys_platform == 'win32'"
host = "sys_platform == 'darwin'"
context = { implementation = "cpython", python = "3.12", platform = "macos" }
packages = [
    { name = "builder", version = "1.0.0" },
    { name = "darwin-helper", version = "1.0.0" },
]
```

Evaluating `target` against the build host would incorrectly discard the
captured environment during a valid cross-target replay.

Output-target configuration can affect backend behavior and freshness. The
example does not imply that arbitrary backends support cross-compilation.

### Host Capture Boundary

Dependency-discovery hooks are arbitrary backend code executed on a concrete
interpreter and host. Running a hook on Darwin cannot discover requirements
returned only when the same hook runs on Linux.

A local lock operation records only build-executor contexts it actually
captures. It must not infer arbitrary host coverage from universal resolution
of marker-declared requirements.

Frozen replay fails when no captured variant covers the requested executor
context. A future multi-host workflow can run capture on multiple executors
and merge the resulting variants.

### Execution Modes

Build records use an explicit execution mode:

```text
isolated  Replay installs a captured environment before invoking the backend.
direct    Replay invokes a compatible direct backend without an external
          environment.
```

For an isolated record, one or more environment variants are required. A
captured environment can contain zero packages:

```toml
[[build]]
package = { name = "dep", version = "0.1.0", source = { directory = "dep" } }
artifact = { directory = "dep" }
kind = "wheel"
mode = "isolated"
inputs = { version = 1, hash = "sha256:..." }

[[build.environment]]
packages = []
```

This is distinct from a missing record. Absence of a satisfactory build record
means the lock does not cover the requested operation.

A direct record has no environments. Its freshness inputs include the direct
backend identity, operation kind, source identity, uv compatibility contract,
`--no-sources` policy, and relevant configured extras. Frozen replay
revalidates direct-build eligibility or rejects the record.

Policy skips are not replayable build records. Build policy determines whether
an absent record is acceptable for a command. Removing a policy restriction
can make additional records necessary.

### Non-Isolated Builds

`--no-build-isolation` and shared build environments do not have a closed
uv-managed environment to capture. The caller owns their contents.

The initial locked-build contract does not treat a shared environment as a
captured isolated environment. Strict frozen replay rejects build operations
that require a non-isolated environment unless a separate contract is added.

### Freshness

A captured build environment remains reusable only while relevant inputs
remain unchanged.

The design should define typed models for three categories.

**Capture inputs** can change backend discovery or the selected final
environment:

- exact source artifact identity and content hash, Git revision, or mutable
  source identity;
- `[build-system].requires`, `build-backend`, and `backend-path`;
- implicit backend selection;
- operation kind;
- configured extra build dependencies and resolved `match-runtime` bindings;
- build constraints;
- global and per-package config settings;
- extra build variables exposed to backend hooks;
- source mappings, index configuration, index strategy, and `--find-links`;
- `--no-sources`;
- build-executor interpreter, host, tag, ABI, and libc context;
- direct-build frontend and compatibility version, when applicable.

**Selection inputs** determine which source artifacts and operations require
records:

- runtime requirements and sources;
- target platform, tags, and Python range;
- `--no-binary` and `--no-binary-package`;
- index and artifact-selection policy;
- `exclude-newer`.

**Replay policy** determines whether an otherwise valid record is usable:

- `--no-build` and `--no-build-package`;
- build-isolation policy and per-package exceptions;
- direct-build eligibility policy.

The lockfile serializes inputs structurally, stores versioned digests, or does
both. In every case, digests derive from typed definitions so new inputs
participate in validation deliberately.

Mutable source identity requires an explicit policy. Options include a tree
digest, conservative recapture, or a rule that certain mutable sources cannot
be considered fresh without relocking.

### Validation

Structural validation rejects:

- dangling or ambiguous package references;
- missing source artifacts;
- missing hashes for artifacts that require hashes;
- illegal execution-mode field combinations;
- duplicate build records;
- overlapping build variants that do not select one unambiguous environment;
- missing recursive records for selected nested source artifacts;
- build cycles;
- artifact incompatibility with the selected build executor.

Coverage validation rejects a requested operation when no record and variant
cover its exact artifact, kind, target-runtime context, and build-executor
context.

Freshness validation rejects records whose typed inputs no longer match.

These checks are command-sensitive:

- frozen replay performs structural and requested-coverage validation but does
  not claim that mutable lock inputs are current;
- `--locked`, lock checks, and ordinary relocking apply freshness rules as
  appropriate;
- commands that change selection inputs or replay policy reconsider affected
  records.

Validation of universal locks considers supported runtime branches, not only
the current host branch.

### Compatibility

The representation requires a lockfile compatibility fence.

Adding `[[build]]` while removing package-local build overlays is not safely
backward-compatible if an older reader silently ignores unknown top-level
records and resolves build requirements live.

The format must choose one of:

- increment the lockfile schema version so older readers reject the file;
- dual-write a faithful legacy representation during a transition;
- define and explicitly accept degraded behavior for older readers.

New readers also define how they treat legacy preview locks, whether feature
enablement rewrites them, and whether disabling the feature removes or
preserves first-class build records.

## Drawbacks

First-class build records add a new top-level lockfile concept. The same package
can appear in multiple environment member lists, increasing lockfile size.

The proposal changes frozen replay. A backend's dependency-discovery hook runs
during lock generation, not during frozen replay. Backends that rely on side
effects from dependency-discovery hooks during every build would not observe
those side effects under frozen replay. PEP 517 and PEP 660 define these hooks
as dependency discovery; relying on unrelated side effects is not part of this
replay contract.

Capturing build requirements can require backend execution during locking even
when project metadata is otherwise statically available. This is necessary to
record the complete build environment.

Build records do not make arbitrary backend code hermetic. Stronger
reproducibility requires additional controls outside this RFC.

## Rationale and Alternatives

The core design question is not whether build dependencies should be locked.
It is where the lockfile should store an isolated build resolution.

All viable designs need to express the same facts:

- one package artifact can be shared by multiple resolutions;
- one source artifact can have multiple build operations;
- one operation can have multiple target-runtime and build-executor variants;
- multiple build environments can be constructed during one sync;
- build environments must not mutate the runtime dependency graph.

The alternatives below explore different answers to where ownership of those
facts belongs.

### Treat Builds As Runtime Resolver Forks

Universal runtime resolution already retains multiple selections in one
lockfile. For example:

```text
Linux runtime:  helper==1
Darwin runtime: helper==2
```

Runtime forks and build records both allow multiple resolved selections to
refer to shared package artifacts. They differ in execution semantics.

A runtime install selects an applicable branch of one graph. A sync can
construct several build environments, each in isolation, in addition to
installing the runtime graph:

```text
runtime
wheel(dep-a)
wheel(dep-b)
```

The build environments are not mutually exclusive alternatives. They are
separate operations. Build ownership is therefore a first-class record rather
than another runtime fork marker.

This RFC still borrows the useful part of the resolver-fork model: shared
artifacts can be referenced from multiple selected contexts. It rejects the
part that does not apply: representing every context as a branch of one
runtime traversal.

### Generalize The Lockfile As A Multi-Resolution Graph

The strongest alternative is to generalize uv's existing graph model so the
lockfile can represent many distinct resolutions at once.

Today, uv already merges multiple runtime fork resolutions into one package
graph. Package records are global nodes. Dependency edges carry marker context.
Installing for one runtime means selecting the active context and traversing
the applicable edges.

The generalized design keeps that model and makes the context explicit:

```toml
[[resolution]]
id = "runtime"
kind = "runtime"
roots = [{ name = "project" }]

[[resolution]]
id = "build:dep-a:wheel:<digest>"
kind = "build"
package = { name = "dep-a", version = "0.1.0", source = { directory = "dep-a" } }
artifact = { directory = "dep-a" }
operation = "wheel"
mode = "isolated"
target = { marker = "sys_platform == 'linux'", conflicts = ["extra:foo"] }
host = { marker = "sys_platform == 'darwin'", python = "3.12" }
inputs = { version = 1, hash = "sha256:..." }
roots = [
    { name = "builder", version = "1.0.0" },
    { name = "helper", version = "1.0.0" },
]
```

Package entries remain the shared artifact catalogue and dependency graph.
Dependency edges are active in one or more resolution contexts:

```toml
[[package]]
name = "builder"
version = "1.0.0"
dependencies = [
    { name = "helper", version = "2.0.0", contexts = ["runtime"] },
    { name = "helper", version = "1.0.0", contexts = ["build:dep-a:wheel:<digest>"] },
]
```

This preserves the normal lockfile property that dependencies are represented
as edges. Frozen replay for a build would not install a stored member list; it
would select the build resolution context and traverse the context-active graph
from that resolution's roots.

Conceptually, this is a typed version of uv's existing `UniversalMarker` idea.
The current runtime graph uses markers to say when an edge is active. The
multi-resolution graph would replace "marker means runtime environment
predicate" with a broader context expression:

```text
edge is active in runtime fork X
edge is active in wheel(dep-a) captured on host Y for target branch Z
```

This is not the same as encoding builds as fake extras. The context is a
lockfile concept with typed fields. It can include PEP 508 markers, uv conflict
branches, build operation identity, exact source artifact identity,
target-runtime context, and build-executor context.

This design has strong advantages:

- it keeps the existing package/edge graph as the central representation;
- it preserves dependency provenance for build environments;
- it can share identical transitive edges across runtime and build contexts;
- it gives uv one model for runtime forks, build variants, and future
  independent resolutions;
- it avoids a special member-list replay path.

It also has real costs:

- the lockfile needs first-class `[[resolution]]` records for roots, replay
  mode, artifact identity, operation kind, freshness, and coverage;
- dependency edges need typed context selectors, not only PEP 508 markers;
- every transitive build edge must carry the correct context selector;
- validation must prove that each resolution context has complete edge
  coverage and does not accidentally traverse edges from another context;
- artifact selection must be context-aware when a package has both wheels and
  source artifacts;
- the marker/fork machinery must stop treating all non-runtime context as
  synthetic extras.

The design is attractive if the goal is "one lockfile graph can represent many
independent resolutions." It is also a larger conceptual change than
first-class build records with member lists. The member-list design introduces
a build-specific replay contract. The multi-resolution graph design
generalizes the lockfile's existing replay contract.

If selected, this design would replace `[[build.environment]].packages` with
`[[resolution]]` roots plus context-scoped dependency edges. Build records
would still exist as resolution metadata, but replay authority would remain in
the dependency graph.

### Encode Build Identity With Synthetic Markers

One alternative is to encode build ownership with synthetic marker
expressions:

```toml
{ name = "helper", version = "1.0.0", marker = "extra == 'build-dep-a'" }
```

This is close to how uv currently represents some universal conflict branches:
the marker expression carries lockfile context that is not a real Python
environment predicate.

The appeal is that existing marker algebra can distinguish runtime edges from
build-owned edges. It also lets the lockfile keep one graph and select a
subgraph by activating a synthetic build context.

However, this turns build identity into a fake environment property. It also
forces ordinary runtime edges to exclude build scopes whenever selections
differ:

```toml
{ name = "helper", version = "2.0.0", marker = "extra != 'build-dep-a'" }
```

Most importantly, synthetic marker forks imply alternative traversal branches.
Build environments are separately installed environments and can all be used
in one sync.

This alternative is rejected as a build-lock representation. A future cleanup
can replace synthetic conflict markers with a typed context expression, but
build records do not require that work and should not extend fake extras into
another domain.

### Store Build Requirements On Package Entries

Another alternative is to keep build data inside the package being built:

```toml
[[package]]
name = "dep-a"
version = "0.1.0"
source = { directory = "dep-a" }
build-dependencies = [
    { name = "builder", version = "1.0.0" },
    { name = "helper", version = "1.0.0" },
]
```

This is compact and easy to read for simple cases. It also puts information
near the package that owns the build.

The problem is that the package entry is already serving as the runtime
package record. Adding build requirements there does not by itself say which
operation they apply to, which exact source artifact was built, which target
runtime selected it, which build executor captured it, or whether the entries
are direct requirements or the final resolved environment.

The representation can be extended with more package-local fields:

```toml
[[package]]
name = "dep-a"
version = "0.1.0"

[[package.build]]
artifact = { directory = "dep-a" }
kind = "wheel"
target = "..."
host = "..."
packages = [{ name = "builder", version = "1.0.0" }]
```

At that point, the model has become first-class build records nested under
package records. The remaining distinction is placement in the file, not the
data model. A top-level `[[build]]` table keeps the build graph separate from
runtime package records and avoids implying that build dependencies are
runtime package edges.

### Combine Package Build Dependencies With Synthetic Build Markers

A hybrid alternative is to keep `build-dependencies` inside each package, but
use synthetic marker branches to isolate build variants:

```toml
[[package]]
name = "dep-a"
version = "0.1.0"
build-dependencies = [
    { name = "builder", version = "1.0.0", marker = "extra == 'build-dep-a-wheel'" },
]
```

This improves over unscoped package-local fields because it can express
multiple build variants. It still inherits the synthetic-marker problem:
ownership of a separate build operation is encoded as if it were an
environment predicate. It also splits the model across two mechanisms. The
package field says "this is a build dependency"; the marker field says "which
build this belongs to"; other fields still need to describe operation kind,
artifact identity, executor context, and freshness.

The design is therefore more complex than a direct build record without
removing the need for build-owned state.

### Annotate Dependency Edges With Build Scope

Another alternative is to annotate dependency edges with the source builds
that use them:

```toml
[[package]]
name = "builder"
version = "1.0.0"
dependencies = [
    { name = "helper", version = "2.0.0" },
    { name = "helper", version = "1.0.0", build = [
        { name = "dep-a", version = "0.1.0", source = { directory = "dep-a" } },
    ] },
]
```

This can represent the required information if traversal follows only edges in
the active build scope. It can also deduplicate repeated edges.

However, it is an inverted representation of per-build environments.
Build-root membership must be propagated through every selected transitive
edge:

```text
dep-a build -> builder -> helper -> leaf
```

Every edge in that chain needs to know that it belongs to `dep-a`'s wheel
build. If the same intermediate package appears in another build, the graph
has to carry another scope through the same nodes.

This solves part of the representation problem, but it does not provide a
natural home for execution mode, exact input artifact, operation kind,
target-runtime context, build-executor context, or freshness. Those facts
still belong to the build root.

Member lists under first-class build records express the replay contract
directly: this build environment contains these locked packages. Scoped edges
remain useful as optional provenance, compression, or diagnostics, but they are
not the primary model.

### Add A Top-Level `build = [...]` Scope Field To Edges

A related alternative is to keep the global package graph but add an explicit
scope field to dependency edges:

```toml
[[package]]
name = "builder"
version = "1.0.0"
dependencies = [
    { name = "helper", version = "1.0.0", build = [
        { package = "dep-a", artifact = "sdist", kind = "wheel" },
        { package = "dep-b", artifact = "sdist", kind = "wheel" },
    ] },
]
```

This avoids fake marker syntax and can deduplicate identical scoped edges
across build roots. It also makes it clear that the scope is lockfile context,
not a PEP 508 predicate.

The cost is that the lockfile records the build environment by distributing it
across all edges in the transitive closure. To replay a build, uv has to start
from the build root and recover membership by graph traversal. To validate a
build, uv has to ensure the scope is propagated consistently through the
entire closure. To explain or refresh a build, uv still needs root-owned
metadata somewhere.

That is the same parent-tree propagation problem as edge annotations. The
field is a better spelling than fake extras, but not a better ownership model.

### Use Top-Level Build Records Without Environment Membership

Another alternative is a top-level build table that records only roots and
lets ordinary package dependencies define the closure:

```toml
[[build]]
package = { name = "dep-a", version = "0.1.0" }
artifact = { directory = "dep-a" }
kind = "wheel"
dependencies = [{ name = "builder", version = "1.0.0" }]
```

This gives build operations their own identity while avoiding repeated member
lists.

It fails when a package has different selected closures in runtime and build
contexts, or in two build contexts. The ordinary package dependency graph
cannot simultaneously mean:

```text
runtime(builder) -> helper==2
wheel(dep-a): builder -> helper==1
wheel(dep-b): builder -> helper==2
```

Once build-specific transitive edges are needed, this alternative becomes
either scoped-edge propagation or per-build graphs. Direct environment
membership is the smaller replay contract.

### Store Complete Graphs Under Build Records

A first-class build record can retain a complete dependency graph rather than
a package member list:

```toml
[[build]]
package = { name = "dep-a", version = "0.1.0" }
kind = "wheel"

[[build.package]]
name = "builder"
version = "1.0.0"
dependencies = [{ name = "helper", version = "1.0.0" }]
```

This preserves resolver provenance and can improve diagnostics. It also maps
closely to an in-memory resolution graph.

The drawback is that frozen replay does not need dependency-edge traversal.
Resolution has already selected the closure, and replay only needs to install
the selected members. A graph is therefore more than the semantic contract
requires.

In the member-list design, member lists are the normative replay model.
Build-local graphs can be added later as non-authoritative provenance when
diagnostics, exports, or relock preferences need them.

### Use Top-Level Build Records As Parent-Tree Records

A related design treats each build entry as a record of the parent tree that
requires it:

```toml
[[build]]
required-by = [
    { name = "parent-a", version = "1.0.0" },
    { name = "parent-b", version = "1.0.0" },
]
package = { name = "builder", version = "1.0.0" }
kind = "wheel"
```

This mirrors how the need for a build is discovered: a selected parent pulls a
source artifact into some target region.

The build operation, however, is owned by the source artifact being built, not
by every parent that can reach it. Parents determine target reachability and
can contribute runtime bindings, but the captured environment is for
`wheel(builder)` or `editable(builder)`. Making the parent tree primary would
force parent reachability through the transitive build graph and make
deduplication harder when multiple parents require the same build variant.

The chosen model records parent effects as target-runtime context and
runtime-binding inputs on the build variant, while keeping the build record
owned by the source artifact and operation.

### Reuse A Single Build Environment Per Source Artifact

Another simplification is to key build environments only by source artifact:

```text
build(dep)
```

instead of:

```text
wheel(dep)
editable(dep)
sdist(dep)
```

or:

```text
wheel(dep, target-context, build-executor-context)
```

This is insufficient because build backends can return different requirements
for wheel, editable, and sdist operations. The same operation can also require
different packages for different target-runtime bindings or build executors.

The schema therefore treats operation kind and variant context as part of the
identity rather than as incidental metadata.

### Capture Only Static Build Requirements

Another alternative is to lock only `[build-system].requires` and skip
dependency-discovery hooks. This keeps locking fast for projects with static
metadata and avoids executing backend code during lock generation.

It is incomplete for PEP 517 and PEP 660. Backends can return additional
requirements from `get_requires_for_build_wheel`,
`get_requires_for_build_editable`, or `get_requires_for_build_sdist`. Frozen
replay would still need to resolve those requirements outside the lockfile, or
would fail when the backend expects them to be installed.

This alternative is rejected because the goal is to lock the complete build
dependency environment.

### Replaying Dependency-Discovery Hooks

Another alternative is to rerun dependency-discovery hooks during frozen replay
and validate their requirements against locked data.

That more closely preserves the ordinary frontend invocation sequence, but it
requires the lockfile to distinguish bootstrap requirements from hook-added
requirements and to reconstruct intermediate environments. Hook output can
also change between locking and replay, making replay depend on backend
execution rather than the captured lock.

Installing the captured final environment is simpler and more reproducible
with respect to dependency selection.

### Make Frozen Replay Faithful To Build Stages

A stricter alternative is to lock each stage separately and replay the ordinary
frontend sequence:

```text
install bootstrap requirements
run get_requires_for_build_wheel
install hook-added requirements
run prepare_metadata_for_build_wheel or build_wheel
```

This is closer to how a non-frozen build runs. It also avoids installing
hook-only dependencies before the discovery hook runs.

The cost is a more complex lock model. The lockfile must distinguish
bootstrap, hook-added, configured extra, and final requirements as replay
authority. It also still executes discovery hooks during frozen replay, so a
changed backend can produce changed hook output and force validation decisions
at replay time.

This RFC instead treats dependency discovery as lock-generation work. Frozen
replay installs the final captured environment and invokes downstream hooks.
Requirement provenance can still be stored for diagnostics, but stage replay
is not the primary contract.

### Capture Build Environments As A Separate Lockfile

Another alternative is to keep `uv.lock` focused on runtime resolution and
write build environments into a separate file.

This would isolate the new format and reduce the risk of confusing runtime and
build records. It also makes compatibility easier for older uv versions that
only understand runtime locks.

The drawback is that runtime artifact selection and build capture are not
independent. Whether a build record is required depends on runtime selection,
target reachability, `--no-binary`, `match-runtime`, source configuration, and
nested build dependencies. A separate file would need to duplicate or strongly
reference the runtime lock to stay coherent.

The build records therefore belong in the same lockfile, even though they are
not part of the runtime graph.

## Prior Art

Uv's universal runtime resolution already separates shared package artifacts
from context-dependent selection. This RFC applies the same high-level lesson:
shared artifacts do not imply one shared resolution context.

The proposal does not require build environments to use the same encoding as
runtime forks. Runtime forks select branches of one installation graph. Build
records describe separate isolated installation operations.

PEP 517 defines wheel and sdist build hooks and their dependency-discovery
hooks: <https://peps.python.org/pep-0517/>.

PEP 660 defines editable build hooks and their dependency-discovery hook:
<https://peps.python.org/pep-0660/>.

## Unresolved Questions

1. Should build replay be represented by captured member lists, or by a
   generalized multi-resolution graph that keeps dependency-edge traversal as
   replay authority?
2. What exact wire representation should encode target-runtime context,
   including uv's universal conflict context?
3. What typed build-executor profile is sufficient to validate host,
   interpreter, ABI, tag, and libc compatibility?
4. Should source artifacts always be explicit, or can the wire format omit an
   artifact reference under a validated singleton-source invariant?
5. What mutable-directory freshness policy balances correctness and relock
   cost?
6. Should capture inputs be serialized structurally, as versioned digests, or
   both?
7. Should build records retain optional build-local dependency edges and
   normalized requirement provenance for diagnostics and resolver preferences?
8. Should the first version cover project sync only, or also reproducible
   `uv build` operations and generated-sdist pipelines?
9. What workflow merges capture variants produced on multiple build
   executors?
10. Should a compatibility transition use a schema-version fence or a
   dual-written legacy representation?

## Future Possibilities

The runtime fork representation can benefit from a typed internal context
algebra rather than synthetic marker expressions. A generalized
multi-resolution graph would make that work central: runtime forks and build
variants would share one context selector model.

Optional build-local dependency graphs can improve diagnostics, explain why
a package is present in a captured environment, and provide resolver
preferences during relocking.

A multi-executor capture workflow can merge variants produced on different
hosts or interpreters after validating that their exact source artifacts and
common inputs match.
