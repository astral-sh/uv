# What changed when uv locked build dependencies

Creating a lock took an extra **0.44 seconds** on the median successful pair, about twice the
baseline time. Once the lock and cache were populated, all ten offline freshness checks finished in
**54–100 milliseconds** across five named projects. These are timings from debug builds on a shared
machine, not release benchmarks.

## All 15,000 inventory entries

Each pinned input ran with both binaries inside a container. **12,951** succeeded with both; **148**
succeeded only with the baseline; **0** succeeded only with the candidate; **1,901** failed on both
sides or had no pinned input. Failures do not contribute a zero time or zero size to the comparison.

| Measure across 12,951 successful pairs                           |  Median | 90th percentile |
| ---------------------------------------------------------------- | ------: | --------------: |
| Additional lock time                                             | +0.44 s |         +1.93 s |
| Change vs. that same input's baseline                            |   +114% |           +353% |
| Additional lockfile bytes                                        |  +9,965 |         +38,796 |
| Additional packages to audit (distinct name, version and source) |      13 |              29 |

All 12,951 successful pairs kept every baseline package identity. That does not, by itself, prove
that every runtime edge was unchanged. A large increase can be correct even for a tiny package:
`strip-ansi==0.1.1` builds with `poetry>=0.12`. Its lock goes from one to 78 package identities
before any of them are installed.

### Retesting every new failure

We checked **every one** of the 148 inputs that locked only on the baseline against commit
`75e1919f3a26e6232023b8d1be8d4d4b8e01c0e3`. **96 recovered; 52 still failed.** None panicked, and
the harness completed all 148 retries. The diagnostic counts were:

| Final diagnostic (grouped by its wording)                          | Inputs |
| ------------------------------------------------------------------ | -----: |
| Diagnostic explicitly requires wheels because of `--no-build`      |     17 |
| Source archive rejected, unreadable, or missing its Python project |     12 |
| No solution for the reported requirements                          |     11 |
| Git unavailable or TLS verification failed for an external source  |     11 |
| Workspace discovery inspected unrelated source mappings            |      1 |

There were 43 panic reports on the original candidate, including 18 for inputs that had passed on
the baseline. A debug assertion while reporting unavailable development versions does not establish
a crash in release builds. Some failures can also disappear with different network conditions. We
did not rerun all 15,000 inputs at the final revision, so these numbers alone cannot establish its
overall failure rate.

The one workspace-discovery failure was `arize-phoenix==13.17.0`. Its build requires only
`hatchling`, but its source archive references a separate workspace member not present in the
archive. Commit `441c5518a2` avoids following source mappings unrelated to the build. On this commit
the same input locked in 45.34 seconds and passed the offline freshness check in 90 ms. The earlier
retest still counts it among the 52 failures on that revision.

The survey also found that unrelated invalid `[project]` metadata in the source archive for
`dbt-extractor==0.6.0` could block the first candidate. Reading only the build declaration let both
that package and its dependent `dbt-osmosis==1.2.2` lock.

As a separate cross-check, an intermediate repair preserved the baseline outcome on 25 seeded random
inputs: 22 locked with both and 3 failed on both.

## Named projects on the repaired candidate

| Workload     | Seconds, baseline → candidate | Lockfile bytes, baseline → candidate | Packages added |
| ------------ | ----------------------------: | -----------------------------------: | -------------: |
| Black        |                   0.53 → 4.92 |                       8,611 → 48,165 |            +23 |
| HTTPX        |                   0.48 → 0.89 |                       7,668 → 19,170 |            +15 |
| Jupyter      |                  2.14 → 20.55 |                    133,352 → 176,578 |            +27 |
| Airflow      |                  6.01 → 21.53 |                    207,800 → 266,639 |            +37 |
| Transformers |                  1.79 → 18.82 |                     83,587 → 110,196 |            +24 |
| Warehouse    |                  8.97 → 45.73 |                    266,817 → 319,975 |            +24 |

Warehouse needs special treatment: requiring wheels rejects its `psycopg-c` dependency **even on the
baseline**. For that row only, source builds were permitted. Each side ran as an unprivileged user
in a separate container with no host mounts, no credentials, and an HTTPS relay allowing only
`pypi.org` and `files.pythonhosted.org`. Both produced lockfiles.

### Rechecking the same lock offline

We immediately reran each side with `uv lock --check --offline`, its own new lock and the cache it
had just populated. **All ten offline checks passed** (both binaries on five projects). These checks
measure the normal unchanged path, not cold resolution or performance without a cache.

| Workload     | Baseline | Candidate |
| ------------ | -------: | --------: |
| Black        |    55 ms |     66 ms |
| HTTPX        |    54 ms |     57 ms |
| Jupyter      |    81 ms |     89 ms |
| Airflow      |    87 ms |    100 ms |
| Transformers |    65 ms |     92 ms |

## Reproducing and interpreting the measurements

The baseline commit was `7f396534d58321e60b5ba204151326712d912ece`. The full cohort measured
candidate Git **source tree** `fac0ff5d2bfef2aaf06be22b21b8212d5d8689ee`; the named benchmarks and
the 74-input intermediate rerun measured `3922440ba9d463ce4abfeb98cb9cbe42a6ac3d43`. These are tree
IDs, not published commit SHAs. The portable report bundles both exact patches, raw paired
observations, the subset manifest, the named before/after lockfiles, and the runner used with the
[pinned inventory](https://github.com/astral-sh/uv-ecosystem-testing/blob/4ae32dc2f34d250b64c0cca7c6f2217517087ff7/data/top-15k-pypi-latest-version.csv).

The full run used equivalent debug builds on Linux, Python 3.11, PyPI, an exclusive cutoff of
**2026-11-01 00:00 UTC**, and no distribution builds. We ran 24 pairs concurrently, alternated which
binary ran first, used two separate caches initially empty, and allowed 90 seconds per lock. Four
entries in the pinned inventory were missing versions; we kept them as failed inputs rather than
substituting unpinned packages.

Timings include network traffic and concurrent compilation and tests on a shared machine. The
direction and scale of a change are more informative than individual milliseconds; a single run is
not a release-performance promise. These experiments measured locking, not whether a production
install rebuilds, reuses a cached wheel, or requests extra dependencies from a backend.
