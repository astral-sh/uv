# RFC: Locked Build Dependency Resolutions

## Summary

Extend the uv lockfile so source builds replay their dependency selection from the lockfile.

The design keeps uv's existing package graph as the unit of reuse and adds staged build-resolution
roots:

- `[[package]]` records continue to describe distributions, artifacts, hashes, and dependency edges.
- `[[resolution]]` records identify the source package, build operation, target reachability, and
  the roots for one build stage.
- An optional `resolution-id` extends package identity only when a build closure conflicts with the
  otherwise reusable package graph.

Lock generation resolves a bootstrap environment, runs the applicable dependency-discovery hook, and
resolves the final build environment. Frozen replay follows the same sequence using the locked roots
and ordinary dependency traversal; it does not resolve build requirements again.

Build requirement discovery is treated as stable metadata for a source artifact and its build
inputs. Backends are assumed to return the same requirements across environments, apart from PEP 508
markers on those requirements. The lockfile does not serialize an executor identity.

This guarantees that build-dependency selection comes from the lockfile. It does not claim that
arbitrary backend execution is hermetic or that two builds produce byte-identical artifacts.

`RFC-BUILD-DEPS-LOCK.md` contains the solution-independent motivation and constraints.
`RFC-BUILD-RESOLUTION-ID.md` describes the package-identity extension in detail.

## Motivation

Python source builds run in isolated environments. A build backend declares initial requirements
through `[build-system].requires` and can return additional requirements from a dependency-discovery
hook.

PEP 517 defines distinct discovery hooks for wheel and sdist builds:

- `get_requires_for_build_wheel`
- `get_requires_for_build_sdist`

PEP 660 adds a distinct discovery hook for editable builds:

- `get_requires_for_build_editable`

The requirements returned for these operations can differ. The operation is therefore part of the
identity of a captured build resolution.

For frozen replay to reproduce dependency selection, every package installed into a build
environment must come from the lockfile. Resolving only runtime dependencies is not sufficient.

Build environments can reuse runtime packages, but they cannot always reuse the runtime closure. For
example:

```text
runtime:
  builder==1 -> helper==2

wheel(dep-a):
  builder==1 -> helper==1

wheel(dep-b):
  builder==1 -> helper==2
```

The same `builder==1` artifact is valid in all three environments. One package node cannot describe
both selected dependency closures without leaking build-only edges into runtime traversal. The
lockfile must be able to reuse compatible nodes and split only the nodes whose closures conflict.

Build resolutions are not runtime forks. Runtime forks are mutually exclusive branches of one
installation. Several isolated build resolutions can be constructed during the same sync and can
reuse the same locked package nodes.

## Guide-Level Explanation

Consider a source package whose bootstrap requirement is `builder==1` and whose wheel-discovery hook
adds `helper==1`. The lockfile records the two stages independently:

```toml
[[resolution]]
id = "build:dep-a:wheel:bootstrap:<digest>"
kind = "build"
operation = "wheel"
mode = "isolated"
stage = "bootstrap"
name = "dep-a"
roots = [
    { name = "builder", version = "1.0.0" },
]

[[resolution]]
id = "build:dep-a:wheel:build:<digest>"
kind = "build"
operation = "wheel"
mode = "isolated"
stage = "build"
name = "dep-a"
roots = [
    { name = "builder", version = "1.0.0" },
    { name = "helper", version = "1.0.0" },
]
```

The roots enter the ordinary package graph. When the selected closure is compatible with an existing
package node, it is shared:

```toml
[[package]]
name = "builder"
version = "1.0.0"
source = { registry = "https://pypi.org/simple" }
dependencies = [
    { name = "helper", version = "1.0.0" },
]
wheels = [
    { url = "https://example.invalid/builder-1.0.0-py3-none-any.whl", hash = "sha256:..." },
]
```

If another build requires the same `builder` artifact with a different selected `helper`, the writer
creates a scoped package node and points that build root at it:

```toml
[[resolution]]
id = "build:dep-b:wheel:build:<digest>"
kind = "build"
operation = "wheel"
mode = "isolated"
stage = "build"
name = "dep-b"
roots = [
    { name = "builder", version = "1.0.0", resolution-id = "build:dep-b:wheel:build:<digest>" },
    { name = "helper", version = "2.0.0" },
]

[[package]]
name = "builder"
version = "1.0.0"
source = { registry = "https://pypi.org/simple" }
resolution-id = "build:dep-b:wheel:build:<digest>"
dependencies = [
    { name = "helper", version = "2.0.0" },
]
wheels = [
    { url = "https://example.invalid/builder-1.0.0-py3-none-any.whl", hash = "sha256:..." },
]
```

The artifact is still reusable. `resolution-id` separates only the package node and dependency
closure that would otherwise conflict.

During frozen replay, uv installs the locked bootstrap closure, runs and validates the applicable
dependency-discovery hook, then installs the locked final closure before invoking the downstream
metadata or build hook. Hook-only dependencies are not installed before discovery runs.

## Reference-Level Explanation

### Build Operations And Stages

A staged build resolution currently captures wheel and editable operations:

| Operation  | Discovery hook                    | Downstream hooks                                        |
| ---------- | --------------------------------- | ------------------------------------------------------- |
| `wheel`    | `get_requires_for_build_wheel`    | `prepare_metadata_for_build_wheel`, `build_wheel`       |
| `editable` | `get_requires_for_build_editable` | `prepare_metadata_for_build_editable`, `build_editable` |

PEP 517 also defines `get_requires_for_build_sdist` and `build_sdist`. Capturing an `sdist`
operation, including a generated-sdist-to-wheel pipeline, is a possible extension and is not part of
the current project-sync contract.

Metadata preparation is not a separate operation. It uses the environment for the corresponding
wheel or editable operation.

An isolated operation has two stages:

```text
bootstrap  [build-system].requires and configured extra build requirements
build      bootstrap requirements plus the requirements returned by discovery
```

The stages remain distinct even when their selected roots happen to be equal. A discovery hook can
observe whether a hook-only dependency is already installed, so installing the final environment
before running the hook is not equivalent to the ordinary frontend sequence.

### Resolution Records

A build `[[resolution]]` record identifies:

- the source package being built;
- the build operation and stage;
- the isolated execution mode;
- target-runtime reachability, when needed;
- the locked direct roots for that stage.

The resolution ID is generated by uv and includes the source identity, operation, stage, and target
context necessary to keep independent captures distinct. It is not an executor selector.

The target context identifies runtime branches that can select the source package or affect
`match-runtime` bindings. It includes ordinary markers and uv fork context such as extras, groups,
and conflict branches. Forks determine when a build root is needed; they do not turn independent
build resolutions into mutually exclusive alternatives.

### Package Identity And Traversal

Package identity is extended from:

```text
name + version + source
```

to:

```text
name + version + source + optional resolution-id
```

An unscoped package node remains the shared representation. Runtime packages and compatible build
closures use that node. A scoped node is emitted only when sharing would change dependency edges,
metadata, or artifact selection for an otherwise identical package.

Build roots and dependency references include `resolution-id` only when they point at a scoped node.
The field is part of the target package identity; it is not a conditional edge selector. Frozen
replay can therefore use the existing dependency traversal model and retain ordinary dependency
provenance.

Build-only packages use the same package representation and artifact-hash requirements as runtime
packages. A runtime package must not gain incompatible build-only dependency edges merely because a
build resolution selected the same artifact.

### Lock Generation

For each source package and operation requiring capture, lock generation:

1. Lowers `[build-system].requires` using the same sources, indexes, and constraints as an ordinary
   build.
2. Selects the implicit default backend when the source omits `[build-system]`.
3. Adds configured extra build requirements, including resolved `match-runtime` bindings.
4. Resolves and installs the bootstrap environment.
5. Runs the applicable dependency-discovery hook.
6. Lowers hook-added requirements with the same source semantics.
7. Resolves the final build environment.
8. Records both stages and adds any build-only package nodes and dependency edges required for
   replay.

Directory, archive, direct-URL, Git, and registry source distributions use the same capture
contract. Static runtime metadata can avoid an unnecessary metadata build, but it cannot justify
omitting discovery when that source distribution can be selected for a build.

### Stable Build Metadata And Artifact-Tag Safety

Build requirement discovery follows the same practical contract uv uses for runtime metadata. For a
given source artifact, operation, configuration, extra build requirements, source and index
configuration, constraints, and frontend policy, a backend is assumed to report consistent build
requirements across environments.

Conditional requirements are still supported:

```text
helper ; python_version < "3.12"
```

The marker is stable metadata on the locked requirement or dependency edge and is evaluated against
the interpreter performing the build during replay. A backend that returns unrelated requirement
lists solely because the hook happened to run on another host is outside the universal-lock
contract, just as inconsistent wheel metadata across platforms is outside the runtime-lock contract.

Lock generation still checks that an artifact selected for installation into the bootstrap or final
environment is compatible with the interpreter performing capture. Platform, ABI, libc, and wheel
tags can therefore constrain _artifact selection during capture_. This is a transient safety check:
it prevents recording an environment that cannot even be installed to run the hook. It does not
create a serialized executor variant, participate in build-resolution identity, or make executor
identity a freshness input.

Target reachability and capture-time artifact compatibility answer different questions. A source
package selected only for a Windows target can be built on a Darwin host when the backend supports
that cross-build configuration. The target selector remains about why the source is needed; it is
not evaluated as a record of the host that captured it.

### Frozen Replay And Recursive Coverage

When frozen replay needs to build a source package, it:

1. Selects the applicable build and bootstrap resolution records for the source, operation, and
   target context.
2. Validates the records, package references, and artifact data.
3. Traverses both staged roots and recursively prepares any source-selected build dependencies.
4. Installs the locked bootstrap closure into the isolated environment.
5. Runs the dependency-discovery hook and validates its output against the locked final roots.
6. Installs the locked final closure.
7. Invokes the downstream metadata or artifact-production hook.

Frozen replay must not resolve build requirements outside the lockfile. New or incompatible hook
requirements are rejected. Requirements omitted by a changed hook remain in the captured final
environment; the hook cannot remove packages that the lock selected.

Coverage is recursive across both stages. A source dependency selected only by the bootstrap stage
still requires its own captured build resolution. A missing staged record is different from a
captured-empty environment and must fail closed when the build is requested.

Backend execution can still observe ambient environment variables, the network, filesystem state,
and time unless separately constrained. Locked dependency selection does not promise byte-identical
artifacts or offline backend execution.

### Freshness And Validation

Freshness depends on inputs that can change discovered requirements or the selected closure,
including:

- source identity and mutable-source contents;
- `[build-system].requires`, backend selection, and backend path;
- operation and stage;
- configured extra build dependencies and `match-runtime` bindings;
- constraints, config settings, and build variables;
- source mappings, indexes, index strategy, `--find-links`, and `--no-sources`;
- target reachability and artifact-selection policy.

The interpreter that happened to perform capture is not a serialized freshness input. Artifact-tag
compatibility is checked while capturing and again when selecting an artifact for replay.

Structural and coverage validation reject:

- dangling or ambiguous package and build-root references;
- missing artifact hashes where hashes are required;
- duplicate or overlapping resolution records that select different closures;
- incompatible runtime and build edges accidentally merged into one package node;
- missing bootstrap or final stages for a requested build;
- missing recursive records for source-selected build dependencies;
- build cycles and artifact incompatibility during replay.

Frozen replay validates the requested graph without claiming that mutable inputs are current.
`--locked`, lock checks, and relocking additionally apply freshness rules.

### Compatibility

Build-resolution records and scoped package identities require a lockfile compatibility fence. An
older reader must not silently ignore the records and resolve build requirements live.

The build-lock representation is written under the newer lock schema and revision. Readers reject
unsupported schema versions and malformed records rather than degrading frozen replay; later
revisions remain readable when their additions are backwards-compatible. Changes to selector
semantics require a schema-version fence. Legacy preview locks can be rewritten when the feature is
enabled; disabling the feature does not make an incomplete frozen build graph valid.

## Rationale And Alternatives

### Treat Build Resolutions As Runtime Forks

Universal runtime forks describe mutually exclusive branches:

```text
Linux runtime:  helper==1
Darwin runtime: helper==2
```

Build resolutions are independent isolated operations:

```text
runtime
wheel(dep-a)
wheel(dep-b)
```

All three can be used during one sync, and they can reuse compatible package nodes. Encoding build
identity as synthetic fork markers or fake extras therefore gives it the wrong execution semantics
and risks leaking build-only edges into runtime traversal.

### Store Complete Environment Member Lists

The lockfile could store each build environment as a list of selected package members and install
that list without dependency traversal. This directly records the closure, but creates a separate
replay model and loses dependency-edge provenance unless another graph is stored for diagnostics.

Staged roots plus ordinary dependency traversal retain uv's existing graph semantics. Optional
`resolution-id` splits only the package nodes that cannot safely be shared.

### Add Resolution Selectors To Every Dependency Edge

A fully generalized multi-resolution graph could keep one package node and annotate each edge with
the resolutions in which it is active. That can deduplicate more aggressively, but requires every
traversal and validator to understand context selectors and propagate them through the complete
transitive closure.

`resolution-id` is the narrower choice: compatible nodes remain shared, conflicting nodes are
duplicated, and dependency traversal stays ordinary.

### Capture Only Static Build Requirements Or Collapse Stages

Locking only `[build-system].requires` is incomplete because PEP 517 and PEP 660 hooks can add
requirements. Installing the final closure before invoking discovery is also incorrect because the
hook can observe which packages are already installed.

The two-stage model preserves the ordinary frontend sequence while ensuring that both stages replay
only locked selections.

### Serialize Executor Variants

Capturing a separate build resolution for every interpreter, host, ABI, libc, or tag profile would
require executing arbitrary backend code in every possible environment and would make lock reuse and
freshness substantially more complex.

Uv instead assumes stable backend requirement metadata, preserves PEP 508 markers on conditional
requirements, and keeps artifact-tag checks local to capture and replay. No executor identity is
serialized.

## Prior Art

Uv's universal runtime resolution already separates shared package artifacts from context-dependent
selection. Locked build resolutions apply the same lesson without treating independent builds as
mutually exclusive forks.

PEP 517 defines wheel and sdist build hooks and their dependency-discovery hooks:
<https://peps.python.org/pep-0517/>.

PEP 660 defines editable build hooks and their dependency-discovery hook:
<https://peps.python.org/pep-0660/>.

## Unresolved Questions

1. What mutable-directory freshness policy balances correctness and relock cost?
2. Should the initial contract cover project sync only, or also reproducible `uv build` operations
   and generated-sdist pipelines?
3. How should exports to formats without scoped package identities represent a conflicting
   `resolution-id` closure?
4. Can artifact metadata shared by scoped package nodes be factored out without making the common
   lockfile harder to inspect?
