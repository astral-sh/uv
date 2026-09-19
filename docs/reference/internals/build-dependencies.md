# Build dependency locking

The `build-dependency-locking` [preview feature](../../concepts/preview.md) records the isolated
Python environments used to build source packages selected from a project's runtime lock. This is
useful when a deployment needs to build packages without compatible wheels, or when changes to build
tools need to be reviewed independently of runtime dependency changes.

The contract is that a project operation which requires a source build must use an applicable locked
build environment. Missing source, operation, executor, stage, or artifact coverage is an error; uv
does not silently resolve a replacement build environment. A cached wheel built from a selected
source, or an installed instance of that source, must also have the appropriate build provenance.

This document describes the current implementation and its boundaries. The nested lockfile tables
are an internal format, not a separate stable interchange API.

## Why build dependencies need their own graphs

A project's runtime lock answers which packages can be installed together in the project
environment. It does not, by itself, answer which packages were used to build them. For example, two
runtime packages may need incompatible versions of `setuptools` to build. Those versions can coexist
in separate build environments even though they cannot coexist in one Python environment.

Adding every build requirement to the runtime graph would introduce conflicts between packages that
never need to be installed together. Recording only a flat list of downloaded or installed packages
would have the opposite problem: it would discard the dependency edges, roots, markers, and source
information needed to validate and replay a resolution.

uv therefore uses an independent, ordinary `uv-lock::Lock` for each build environment. Runtime
packages retain their runtime edges, and build-only packages are not added to the project
environment. Each nested graph is independently valid, including its dependency edges and artifact
selection. Different graphs may select different versions of the same package.

Several related pieces of information remain distinct:

| Information                 | Meaning                                                                                     |
| --------------------------- | ------------------------------------------------------------------------------------------- |
| Runtime lock                | The universal set of valid runtime dependency selections.                                   |
| Selected source             | A source artifact that may need to be built for a valid selection on the observed executor. |
| Declared requirements       | The requirements the frontend read from the source's build-system configuration.            |
| Backend requirements        | The additional requirements returned by the operation's backend hook.                       |
| Locked build graph          | A complete resolution, including the particular wheel chosen for each build package.        |
| Installed build environment | The packages actually reconciled into an isolated environment for one frontend invocation.  |

In particular, an installed package name and version are not evidence of which wheel supplied it,
and a package-wide list of hashes does not identify a particular archive.

## The two build stages

The build frontend follows the staged environment model in
[PEP 517](https://peps.python.org/pep-0517/#build-environment). It first needs an environment in
which it can import the backend and ask for further requirements. The final build may then need a
different environment. [PEP 660](https://peps.python.org/pep-0660/#get-requires-for-build-editable)
defines the corresponding editable-build hooks.

For each source and operation, `LockedBuild` records:

1. The declared requirements and their **bootstrap** resolution.
2. The requirements observed from `get_requires_for_build_wheel` or
   `get_requires_for_build_editable`.
3. A **final** resolution rooted in the union of those two requirement sets, when the hook adds a
   requirement that is not already declared.

The final graph is optional when the bootstrap graph already supplies every declared hook
requirement. An absent final graph is not permission to resolve another one later. The frontend must
consume exactly the stages represented by the lock.

For example, a source might declare `helper>=1`, while its backend reports `helper<2`. Resolving the
declarations alone could select `helper==2`; the final resolution must select a version below 2.
Replaying only the final graph would run the discovery hook in a different environment. Keeping both
graphs lets uv reproduce the discovery environment, check the hook's answer, and then reconcile the
environment to the final graph. Reconciliation is exact: packages needed only by the bootstrap
resolution cannot remain as undeclared inputs to the final build.

`BuildStage` is deliberately separate from `BuildOperation`. A wheel build and an editable build can
report different requirements, and each operation can have both stages. Creating an sdist is not a
supported build operation in this preview; consuming a locked source archive to build a wheel is
supported.

## Identity and observations

`BuildSourceId` reuses the parent lock's package identity. For registry packages, the name, version,
and registry source identify the locked package. For non-registry sources, the precise URL, path, or
Git source is the identity; a version obtained from package metadata is not assumed to be available
to every build caller. A local directory has the same source identity whether it is installed
normally or editably, but those two operations have separate coverage.

`BuildExecutor` records the observed Python marker environment and interpreter ABI tag, when one is
available. Matching is exact. This is intentionally more conservative than assuming that two
interpreters with the same Python minor version will produce identical hook results. It also means
that changing an observed value, such as the Python patch version or platform release, can require
recapturing the build lock. The runtime graph remains universal; the build contract currently covers
one executor.

`BuildSourceInput` records either a SHA-256 digest of the source's `pyproject.toml` bytes or the
observed absence of that file. It does not invent an empty `pyproject.toml`, infer ownership from an
installed package, or claim to hash an entire local source tree. Source archives have a separate
artifact hash in the parent runtime lock. Precise Git identities continue to use the ordinary Git
source model.

Requirement observations are normalized relative to the lock root, sorted, and deduplicated. The
frontend reports declared and backend requirements separately, including an empty hook result. A
missing observation is an error rather than an inferred empty list.

## Capturing a build lock

Creation is explicit:

```console
$ uv lock --build-dependencies --preview-features build-dependency-locking
```

Enabling preview features alone does not create the contract or change the lockfile format.

Capture proceeds through the normal project resolver, distribution database, and PEP 517 frontend:

1. Resolve or validate the ordinary runtime lock.
2. Use `Lock::build_sources` to find sources reachable under any valid project, extra, or dependency
   group selection for this executor. The traversal retains dependency markers, transitive extra
   activation, and valid conflict choices. Wheel compatibility is then evaluated separately, so a
   package is not treated as a source build merely because it appears in the universal lock.
3. Materialize each selected source through the distribution database. Local directories receive
   both wheel and editable coverage. Sources that require executing a backend to obtain runtime
   metadata are rejected.
4. Run the normal frontend setup with a scoped `BuildContext`. It observes the declarations,
   resolves and installs the bootstrap environment, invokes the operation-specific requirement hook,
   and resolves and installs the final environment if needed. Capture discovers these environments;
   it does not need to build the project's wheel merely to enumerate its build dependencies.
5. Retain the full resolver output for each stage. Download or inspect the selected build wheels,
   measure their hashes, and construct ordinary nested lock graphs. Installed candidates and nested
   source builds are rejected.
6. Attach the validated build graphs to the runtime lock and publish the complete lockfile.

The capture context uses wheel-only build resolution. Every package in a nested graph must have
exactly one selected, hashed wheel and no source distribution. When an index supplies hashes, the
selected artifact must satisfy the applicable hash policy. When it supplies none, uv records a
digest measured from the downloaded bytes. For a selected runtime source archive, uv records the
measured digest on that exact archive in the parent lock; it does not borrow a digest from another
artifact of the same package.

An update prefers the previous version selections for the corresponding source, operation, and stage
when they remain valid. `--upgrade` and `--upgrade-package` apply to build dependencies too,
including version bounds supplied with `--upgrade-package`. These preferences influence a new
capture; they do not turn replay into a new resolution.

### Why runtime metadata must be static

The runtime graph is resolved before the independent build graphs are captured. If a source's
runtime dependencies are produced by executing a backend, that metadata could depend on the build
environment. Resolving runtime metadata with one environment and then locking a different build
environment would not establish that the resulting runtime graph is correct.

The preview therefore requires static runtime metadata from `pyproject.toml` or from suitable
`PKG-INFO` metadata. In particular, source-distribution metadata must meet the static-field rules
defined by [PEP 643](https://peps.python.org/pep-0643/#specification). Dynamic runtime metadata is
not handled by a fallback build or by assuming that the two environments are equivalent. Supporting
it requires a design that integrates runtime metadata discovery with build-environment capture and
revalidates the resulting runtime graph.

## Replaying the contract

Once a build lock exists, supported project operations enforce it even without the preview flag.
This includes project synchronization, project `uv run`, and `uv build --wheel` from a covered
source directory.

Before consulting installed-state or built-wheel-cache shortcuts, uv checks that selected source
builds have coverage for the current executor and operation. For local directories, it also checks
the current `pyproject.toml` observation. A missing entry or changed declaration is not repaired by
an unlocked build. Commands allowed to update the lockfile can recapture applicable changes;
`--locked` and `--frozen` do not authorize such a write.

When a new build is needed, the dispatcher creates a fresh scoped frontend context. The frontend
still reads the source and runs its requirement hook; the lock does not replace the backend's answer
with a stored answer. Instead, the scoped context checks that the observed declared and backend
requirements match the recorded requirements. Each resolution request is materialized from the
corresponding nested graph with required artifact hashes. The installer then reconciles the isolated
environment exactly, and the frontend proceeds with the build.

The scope tracks which observations and stages have been consumed. Unexpected additional
resolutions, repeated observations, missing stages, or changed requirements are errors. Nested
source builds cannot inherit the outer source's identity or stage, and the bundled backend's direct
build shortcut is not used to bypass a required contract.

Use the explicit capture command to rediscover requirements, or add `--check` to compare the result
without writing it:

```console
$ uv lock --build-dependencies --preview-features build-dependency-locking --check
```

## Validation, cache reuse, and provenance

`LockedBuilds` validates the complete collection before it can be attached to a parent lock. Each
source/operation pair is unique, every referenced source belongs to the parent runtime lock, and
covered source archives have hashes. A nested graph must be an ordinary version-1 lock without a
further build contract. Its bootstrap roots must equal the declared requirements; its final roots
must equal the declared and backend requirements together. The ordinary lock implementation remains
responsible for validating the dependency graph itself.

A content-derived `BuildLockFingerprint` covers the canonical build contract, including its
executor, source observations, requirements, and graphs. It is carried through the selected runtime
resolution and build context, included in built-wheel cache identity, and written into `BuildInfo`
installation provenance. The preparer checks that its resolution and distribution database agree
about the required fingerprint.

Consequently, a warm cache is not evidence that a wheel was built under the current contract. An
installed source package with no receipt, or with a different fingerprint, must be rebuilt or
reinstalled as appropriate. A changed contract cannot reuse an ordinary unscoped built wheel. When
no contract is required, the extra cache-key component is absent, so ordinary builds keep their
normal cache identity.

The fingerprint is deliberately scoped to the whole build contract rather than trying to infer which
changes are harmless to a particular cached build. This can invalidate more cached builds than a
per-source scheme, but keeps reuse and provenance checks consistent. Old cache entries remain
subject to uv's normal cache cleanup; removing a build lock does not relabel them or reconstruct
their build environments.

## Publication and compatibility

A required build contract uses lockfile version 2. A version-1 lock cannot hide a build contract,
and a version-2 lock must contain one. The version fence matters because an older reader that simply
ignored unknown tables could otherwise perform an unlocked build while appearing to honor the
project's lockfile.

Current-version writers acquire an operating-system file lock keyed by the destination lockfile
path, not the package cache directory. They hold it while reading, resolving, and publishing. Before
publication, uv checks that the original file contents have not changed externally. The new contents
are then written with atomic replacement, so readers see either the previous complete lock or the
new complete lock. A failed or interrupted capture does not publish a partial contract, and process
exit releases the writer lock.

An older uv process does not participate in this writer protocol. Before introducing a version-2
lock, coordinate with other lockfile writers and let already-running older commands finish. Older
releases reject the version-2 file. To remove the contract explicitly and return to the ordinary
version-1 format, run:

```console
$ uv lock --no-build-dependencies
```

Removal permits ordinary build resolution again. It does not teach older uv releases how to replay
the removed environments or make their behavior equivalent to the build-locked project.

## Implementation boundaries

The feature extends the existing build path instead of introducing another frontend or installer:

- `uv-lock` owns the build model, serialization, graph validation, runtime-source reachability, and
  materialization of a locked stage into a resolution.
- `uv-distribution` owns source materialization, static metadata inspection, and artifact hash
  verification. Capture uses the same source-specific paths as ordinary distribution operations.
- `uv-build-frontend` owns the PEP 517 and PEP 660 hook sequence. It reports requirement
  observations through `BuildContext` without knowing how they are persisted.
- `uv-dispatch` supplies the scoped capture or replay `BuildContext`, keeps the complete resolver
  output, rejects nested source builds, and requests strict build-environment installation.
- Project commands in `uv` own explicit creation and removal, consumer restrictions, and atomic
  lockfile publication.
- `uv-installer` and the distribution cache enforce the selected contract's fingerprint during
  preparation, cached-wheel lookup, and installed-state checks.

## Deliberate limits

The initial consumer is a project or workspace lock, not a machine-wide build policy. In particular,
independent installers and the `uv pip` interface do not become consumers of a project lock merely
because one exists.

The preview supports one observed executor, static runtime metadata, isolated wheel and editable
builds, and build dependencies available as compatible wheels. It rejects non-isolated builds,
config settings, extra build dependencies or variables, and disabling package sources. Directory
`uv build --wheel` must also use the recorded build constraints. These inputs can affect backend
observations, so accepting them without representing their effect would weaken replay.

Tool installation, script-lock creation, ephemeral `uv run --with` requirements, sdist creation,
`uv build` from an sdist, file listing, and exporting the build contract are not supported. Those
operations need their own source/operation coverage and consumer semantics before they can safely
participate. Recursive source builds would additionally need explicit nested identities, cycle
handling, and complete coverage rather than borrowing the outer build's context.

Finally, this is a dependency-environment lock, not a hermetic build-machine specification. A
backend can still inspect external tools, ambient environment variables, the network, and mutable
local source files. The executor record does not inventory a compiler or operating-system image, and
the `pyproject.toml` digest is not a source-tree snapshot. The feature does not promise
byte-identical wheels. Extending those guarantees would require representing and validating the
additional inputs, rather than adding compatibility guesses to replay.
