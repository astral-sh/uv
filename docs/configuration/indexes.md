# Package indexes

By default, uv uses the [Python Package Index (PyPI)](https://pypi.org) for dependency resolution
and package installation. However, uv can be configured to use other package indexes, including
private indexes, via the `[[tool.uv.index]]` configuration option (and `--index`, its analogous
command-line option).

## Defining an index

To include an additional index when resolving dependencies, add a `[[tool.uv.index]]` entry to your
`pyproject.toml`:

```toml
[[tool.uv.index]]
# Optional, explicit name for the index.
name = "pytorch"
# Required URL for the index. Expects a repository compliant with PEP 503 (the simple repository API).
url = "https://download.pytorch.org/whl/cpu"
```

Indexes are prioritized in the order in which they’re defined, such that the first index listed in
the configuration file is the first index consulted when resolving dependencies, with indexes
provided via the command line taking precedence over those in the configuration file.

By default, uv includes the Python Package Index (PyPI) as the "default" index, i.e., the index used
when a package is not found on any other index. To exclude PyPI from the list of indexes, set
`default = true` on another index entry (or use the `--default-index` command-line option):

```toml
[[tool.uv.index]]
name = "pytorch"
url = "https://download.pytorch.org/whl/cpu"
default = true
```

The default index is always treated as lowest priority, regardless of its position in the list of
indexes.

## Pinning a package to an index

A package can be pinned to a specific index by specifying the index in its `tool.uv.sources` entry.
For example, to ensure that `torch` is _always_ installed from the `pytorch` index, add the
following to your `pyproject.toml`:

```toml
[tool.uv.sources]
torch = { index = "pytorch" }

[[tool.uv.index]]
name = "pytorch"
url = "https://download.pytorch.org/whl/cpu"
```

An index can be marked as `explicit = true` to prevent packages from being installed from that index
unless explicitly pinned to it. For example, to ensure that `torch` is _only_ installed from the
`pytorch` index, add the following to your `pyproject.toml`:

```toml
[tool.uv.sources]
torch = { index = "pytorch" }

[[tool.uv.index]]
name = "pytorch"
url = "https://download.pytorch.org/whl/cpu"
explicit = true
```

Named indexes referenced via `tool.uv.sources` must be defined within the project's `pyproject.toml`
file; indexes provided via the command-line, environment variables, or user-level configuration will
not be recognized.

## Searching across multiple indexes

By default, uv will stop at the first index on which a given package is available, and limit
resolutions to those present on that first index (`first-match`).

For example, if an internal index is specified via `[[tool.uv.index]]`, uv's behavior is such that
if a package exists on that internal index, it will _always_ be installed from that internal index,
and never from PyPI. The intent is to prevent "dependency confusion" attacks, in which an attacker
publishes a malicious package on PyPI with the same name as an internal package, thus causing the
malicious package to be installed instead of the internal package. See, for example,
[the `torchtriton` attack](https://pytorch.org/blog/compromised-nightly-dependency/) from
December 2022.

Users can opt in to alternate index behaviors via the`--index-strategy` command-line option, or the
`UV_INDEX_STRATEGY` environment variable, which supports the following values:

- `first-match` (default): Search for each package across all indexes, limiting the candidate
  versions to those present in the first index that contains the package.
- `unsafe-first-match`: Search for each package across all indexes, but prefer the first index with
  a compatible version, even if newer versions are available on other indexes.
- `unsafe-best-match`: Search for each package across all indexes, and select the best version from
  the combined set of candidate versions.

While `unsafe-best-match` is the closest to pip's behavior, it exposes users to the risk of
"dependency confusion" attacks.

## `--index-url` and `--extra-index-url`

In addition to the `[[tool.uv.index]]` configuration option, uv supports pip-style `--index-url` and
`--extra-index-url` command-line options for compatibility, where `--index-url` defines the default
index and `--extra-index-url` defines additional indexes.

These options can be used in conjunction with the `[[tool.uv.index]]` configuration option, and use
the same prioritization rules:

- The default index is always treated as lowest priority, whether defined via the legacy
  `--index-url` argument, the recommended `--default-index` argument, or a `[[tool.uv.index]]` entry
  with `default = true`.
- Indexes are consulted in the order in which they’re defined, either via the legacy
  `--extra-index-url` argument, the recommended `--index` argument, or `[[tool.uv.index]]` entries.

In effect, `--index-url` and `--extra-index-url` can be thought of as unnamed `[[tool.uv.index]]`
entries, with `default = true` enabled for the former.
