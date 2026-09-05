# Cache internals

!!! tip

    This document focuses on uv's internal cache implementation: the data model, freshness checks,
    and concurrency protocol used by resolvers, installers, Python management, and tool execution.
    For user-facing cache behavior, see the [cache concept](../../concepts/cache.md) and
    [storage reference](../storage.md).

## Cache

uv's cache is the shared store for data that can be reused across commands: index responses, wheel
metadata, downloaded wheels, built wheels, extracted source trees, interpreter probes, managed
Python downloads, and cached tool environments.

The cache is disposable, but uv still treats it as structured state. A cache entry is only useful if
its bucket version is compatible, its freshness information says it can be reused for the current
command, and its contents can be read successfully. Missing, stale, incompatible, or malformed
entries become cache misses and are rebuilt or refetched instead of being treated as authoritative.

## Cache roots

Most commands use the configured cache directory. `--no-cache` uses the same cache machinery with a
temporary root that lives for the duration of the process. This lets the resolver, builder,
installer, and tool runner share artifacts within one invocation without preserving them for later
commands. If the invocation spawns a child process whose environment or unpacked artifacts live
under that temporary root, uv keeps the root alive until the child process exits.

Every cache root has a small scaffold. The root contains `CACHEDIR.TAG`, a `.gitignore`, and a
`.lock` file. The source distribution bucket also contains an empty `.gitignore` and a phony `.git`
file so build backends see cached source trees as isolated source trees, not as part of the
surrounding repository.

## Cache layout

The cache is partitioned into versioned buckets. A bucket name like `wheels-v6` or `simple-v21`
encodes the cache format version for that class of data. If uv changes a bucket in a
backwards-incompatible way, it bumps that bucket version rather than trying to interpret old entries
as new data. Old bucket directories can coexist with new ones and are ignored by current code until
`uv cache prune` removes them.

A small cache root has a shape like:

```text
<cache>/
├── CACHEDIR.TAG
├── .gitignore
├── .lock
├── simple-v21/
├── flat-index-v2/
├── wheels-v6/
├── sdists-v9/
├── git-v0/
├── archive-v0/
├── interpreter-v4/
├── builds-v0/
├── environments-v2/
├── python-v0/
└── binaries-v0/
```

### Cache buckets

The main buckets are:

- `simple`: Simple API responses from package indexes.
- `flat-index`: Flat index responses.
- `wheels`: downloaded registry and direct-URL wheels, plus pointers to their unpacked archives.
- `sdists`: source distributions, source distribution revisions, built wheels, and metadata.
- `git`: cloned Git repositories.
- `archive`: unpacked wheel directories used by the distribution buckets.
- `interpreter`: interpreter metadata and marker information.
- `builds`: temporary build environments and build work directories.
- `environments`: reusable virtual environments used for tools and some script/layered runs.
- `python`: managed Python downloads and unpacked Python installations.
- `binaries`: downloaded standalone tool binaries.

### Index metadata

Index buckets store parsed package index data rather than raw HTML or JSON responses. For the
Simple API, uv stores binary `rkyv` archives containing parsed project pages and index pages:
filenames, URLs, hashes, yanked state, upload-time metadata, `Requires-Python` constraints, and
project status. The format is larger than the older MessagePack representation, but warm-cache
resolution benefits from avoiding repeated parsing and from reading archived data on hot paths.

```text
simple-v21/
├── pypi/
│   ├── index.html.rkyv
│   └── typing-extensions.rkyv
└── index/<index-url-digest>/
    ├── index.html.rkyv
    └── typing-extensions.rkyv

flat-index-v2/
└── html/
    └── <flat-index-url-digest>.msgpack
```

Flat index entries use the same general idea: the response is transformed into the list of files uv
will feed to the resolver, then stored with the HTTP freshness information for the original
request. The filename extension is not the compatibility boundary; the bucket version is.

### Distribution cache

The wheel and source-distribution buckets are grouped first by source kind, then by the stable
identity uv uses for that source. Registry packages are separated by index and package name, direct
URLs and local paths are separated by URL digests, editable installs have their own namespace, and
Git distributions include the repository identity and commit. The on-disk keys avoid credentials,
harmless URL spelling differences, and platform-specific path details.

For a registry wheel that has been seen by the resolver and then downloaded for installation, the
cache may contain:

```text
wheels-v6/
└── pypi/
    └── typing-extensions/
        ├── 4.15.0-py3-none-any.msgpack
        ├── 4.15.0-py3-none-any.http
        └── 4.15.0-py3-none-any -> archive-v0/<archive-id>/

archive-v0/
└── <archive-id>/
    ├── typing_extensions.py
    └── typing_extensions-4.15.0.dist-info/
```

Downloaded or built wheels are installed from unpacked directories, not directly from wheel files.
To make directory entries replaceable, uv stores unpacked wheels under unique IDs in `archive-v0`.
The wheel and source-distribution buckets point at those archive entries. On Unix, the pointers are
symlinks. On Windows, where directory symlinks are often unavailable, uv writes a small structured
file containing the archive ID and archive bucket version.

That indirection is what makes publishing a cached wheel atomic. uv can unpack a wheel into a
temporary directory, validate it, move it into `archive-v0`, and then make it visible by atomically
replacing the pointer in `wheels-v6` or `sdists-v9`. Archive pointers also include the original
filename and hashes, so uv can validate the archive it is about to reuse.

Alternative indexes, direct URLs, local paths, editables, and Git sources use parallel namespaces:

```text
wheels-v6/
├── index/<index-url-digest>/<package>/
├── url/<url-digest>/<package>/
├── path/<path-url-digest>/<package>/
└── editable/<path-url-digest>/<package>/

sdists-v9/
└── git/<repository-digest>/<commit>/<package>/
```

Source distributions add a revision layer. Version alone is not enough to name the cached source
tree: direct URLs can change behind the same URL, local paths can change without changing their
declared version, and build-relevant source state can differ between runs. uv therefore resolves a
source distribution to a revision first, then stores metadata, the unpacked source tree, and any
built wheels below that revision.

```text
sdists-v9/
└── pypi/
    └── watchdog/
        └── 6.0.0/
            ├── .lock
            ├── revision.http
            └── <revision-id>/
                ├── metadata.msgpack
                ├── src/
                ├── watchdog-6.0.0-cp314-cp314-macosx_11_0_arm64.whl
                └── watchdog-6.0.0-cp314-cp314-macosx_11_0_arm64 -> archive-v0/<archive-id>/
```

The `revision.http` and `revision.rev` files point to the active source revision. HTTP revisions
wrap a MessagePack revision pointer in the HTTP cache format. Local revisions are MessagePack
records that include both the revision pointer and the local source-state metadata that made the
revision fresh. Within a revision, built wheels may be sharded further by build inputs such as
configuration settings, extra build dependencies, and extra build variables.

### Other cache buckets

Other buckets are organized around the resource being reused rather than the package name:

```text
interpreter-v4/<host-digest>/<interpreter-digest>.msgpack
environments-v2/<interpreter-digest>/<resolution-digest> -> archive-v0/<archive-id>/
binaries-v0/<tool>/<version>/<platform>/<executable>
```

Interpreter entries are binary MessagePack files containing the marker and `sysconfig` information
uv would otherwise have to query by starting Python. Environment entries point at archived virtual
environments, and downloaded standalone tool binaries are grouped by tool name, version, and
platform.

## HTTP-backed entries

Network-backed metadata and artifacts go through a caching layer over uv's HTTP client. The cache
follows HTTP semantics where they are useful, but stores the transformed data uv wants to reuse
rather than the raw response body. A package index response can become parsed package metadata; a
wheel or source archive response can become a pointer to an unpacked archive or source revision.

HTTP cache files are binary because they are optimized for uv's fast path, not for direct
inspection. MessagePack keeps small structured pointers compact, while `rkyv` is used where uv
benefits from reading archived data without reconstructing the full owned representation first. The
cache format is:

```text
<payload bytes><archived HTTP cache policy><8-byte little-endian policy length>
```

The policy is stored at the end so uv can split it from the payload when reading the file. On a
later request, the archived policy decides whether the entry is fresh, whether uv should revalidate
it with validators such as `ETag` or `Last-Modified`, or whether it has to fetch a new response. If
a server returns `304 Not Modified`, uv refreshes the policy while reusing the cached payload.
Broken cache payloads are removed and healed by making a fresh request.

## Freshness

Freshness answers the question "should uv trust an existing cache entry for this command?" By
default, uv uses entries according to their normal source-specific rules. `--refresh` changes the
cutoff time so matching entries older than the command start are treated as stale, and
`--refresh-package` applies that same idea to specific package names or local source paths. Offline
mode allows stale HTTP entries when they are the only available data.

For remote entries, a stale cache entry becomes an HTTP revalidation request. For local entries,
freshness comes from source-state metadata recorded when uv built or installed the distribution. By
default, uv watches `pyproject.toml`, `setup.py`, `setup.cfg`, and the presence of the `src`
directory for local directory dependencies. Users can replace that default set with
`tool.uv.cache-keys`, which supports file paths, globs, directory existence, Git commit and tag
state, and environment variables.

When installer metadata is enabled, uv writes two uv-specific metadata files into the installed
`.dist-info` directory. `uv_cache.json` records the source-state cache key, which lets the installer
avoid rebuilding a local project when its relevant inputs have not changed. `uv_build.json` records
the build inputs that can affect the produced wheel, including effective `--config-settings`, extra
build dependencies, and extra build variables. On a later install, uv compares the current build
inputs to `uv_build.json`; if they differ, the installed wheel is considered out of date even when
the local source cache key still matches.

A `uv_build.json` file is only written when there are build inputs to remember. For example,
formatted for readability, a wheel built with package-specific config settings, an extra build
requirement, and an extra build variable could include:

```json
{
  "config_settings": {
    "--global-option": ["build_ext", "--inplace"]
  },
  "extra_build_requires": [
    {
      "requirement": {
        "name": "cython",
        "specifier": ">=3.0"
      },
      "match_runtime": false
    }
  ],
  "extra_build_variables": {
    "CUDA_HOME": "/usr/local/cuda"
  }
}
```

Each top-level field is omitted when it is empty. That lets uv distinguish "this wheel was built
with no extra build inputs" from "this installed wheel was built under a different set of inputs."

## Concurrency

uv's cache is designed to be append-oriented. Writers produce data in temporary files or
directories, validate it, and then publish it with atomic writes, renames, or pointer replacement.
This protects readers from seeing partially written cache entries.

There are three important lock scopes:

- The root `.lock` is acquired in shared mode by normal uv processes. `uv cache clean` and
  `uv cache prune` acquire it in exclusive mode before deleting cache data.
- Source distribution revisions and builds are serialized at the shard level, so two processes do
  not try to publish the same derived source tree or built wheel at the same time.
- Individual cache entries can take an exclusive writer lock when the underlying operation requires
  it, such as wheel cache writes on Windows.

The root lock makes `uv cache clean` and `uv cache prune` safe to run alongside normal uv commands
by blocking destructive operations until other uv processes release their shared locks. If shared
locks are unsupported by a platform or filesystem, uv warns and continues with reduced
parallel-process safety.

## Pruning and cleaning

`uv cache clean` removes either the entire cache or package-specific entries. Package-specific
cleaning walks the buckets that are keyed by package name or package metadata, then removes archive
entries that no longer have any remaining pointer. Buckets that cannot be mapped to a package, such
as the Git and interpreter buckets, are left alone by package-specific cleaning.

`uv cache prune` is a reachability pass. It removes unknown top-level buckets, old versioned
buckets, cached environments, source distribution revisions that are no longer referenced by their
current `revision.http` or `revision.rev` pointer, and archive entries that are no longer referenced
from the `wheels` or `sdists` buckets. `uv cache prune --ci` additionally removes pre-built wheel
cache entries and unzipped source distribution contents while retaining wheels built from source,
since those are usually the most expensive artifacts to recreate in continuous integration.

Pruning is conservative around active or malformed cache state. If a pointer is missing, invalid,
or points at an archive that no longer exists, readers treat it as a cache miss. If a cache payload
cannot be deserialized, uv removes or ignores it and falls back to recomputing the data rather than
treating the cache as authoritative.
