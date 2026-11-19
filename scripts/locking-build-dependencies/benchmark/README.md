# Compare lockfiles with and without locked build dependencies

Run the paired benchmark with **two Linux uv binaries** built with the same
toolchain, profile and features. Only the candidate runs with
`--preview-features lock-build-dependencies`.

```sh
# Airflow, Black, HTTPX with HTTP/2, Jupyter, Transformers with torch and sentencepiece, Warehouse
./run.sh /path/to/baseline/uv /path/to/candidate/uv named-results

# All 15,000 pinned releases from astral-sh/uv-ecosystem-testing
./run.sh /path/to/baseline/uv /path/to/candidate/uv inventory-results \
  /path/to/uv-ecosystem-testing/data/top-15k-pypi-latest-version.csv
```

Open `data/index.html` under the output directory. It shows the distribution of
changes in seconds and percent, how many packages were added, and how many bytes
and explicit dependency entries the lockfiles gained or lost.
`data/results.jsonl` contains every measurement and failure. Named runs also
retain each successful lockfile. Inventory runs discard individual lockfiles to
limit disk use.

Both binaries use **November 1, 2026, 00:00 UTC** as an exclusive upload cutoff,
PyPI as the index, and CPython 3.11. Each starts with its own empty cache,
reused across that side's workloads; each individual lock starts from an empty
project. The runner alternates which binary goes first. It allows three minutes
per lock, disallows arbitrary builds, and records failures separately. In
particular this can preclude locking Warehouse, whose dependencies include a
source-only package. Network conditions, earlier cache entries and one timing
per workload limit timing precision.

The named projects use pinned published releases except Warehouse, where we copy
the fixture's runtime requirements into a virtual project. These inputs measure
the added costs of locking transitive build dependencies; they do not measure
building wheels or the build requirements of the Warehouse project itself.

The inventory runner shares each side's cache across parallel pairs (8 by default); named examples run
one pair at a time. Contention affects individual timings; compare distributions across equivalent
runs, not individual timings across different concurrency settings. Interrupted runs can be rendered
with `python report.py path/to/data`; the page reports how many inputs actually completed.
