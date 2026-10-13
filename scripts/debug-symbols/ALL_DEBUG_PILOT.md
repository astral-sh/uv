# All debug levels with thin-LTO instrumentation

Run one fresh native allocation on each of Linux x86-64, Linux ARM64, macOS ARM64 and Windows
x86-64. Each allocation builds no debug information, Rust line tables, limited Rust debug and full
Rust debug, with independent PGO training and fresh target directories for all four treatments. All
four platforms run in parallel; builds within each allocation run sequentially.

The [plan](all-debug-pilot-plan.json) pins main revision `59e5912ecff500422b99cca3df68d48be3841a67`
and its compiled-source fingerprints. Instrumentation uses thin LTO; final compilation uses fat LTO,
optimization level 3, one codegen unit and panic abort. Windows uses static CRT. The production PGO
helper, release profiles, workloads and Cargo parallelism are unchanged. Native C debug information
can remain full for reduced Rust levels.

| Platform       | Runner profile                                | Cargo jobs | Predeclared build order          |
| -------------- | --------------------------------------------- | ---------: | -------------------------------- |
| Linux x86-64   | `depot-ubuntu-24.04-16`                       |          8 | full, line tables, none, limited |
| Linux ARM64    | `depot-ubuntu-24.04-arm-16`                   |          8 | line tables, none, full, limited |
| macOS ARM64    | `namespace-profile-macos-15`                  |          4 | line tables, limited, none, full |
| Windows x86-64 | `namespace-profile-windows-2022-x86-64-16x32` |          4 | limited, full, line tables, none |

Each order is shuffled before dispatch with `random.Random(f"2026101303:{target}")`. Every debug
level compares against the no-debug control from that same allocation. One build per treatment per
platform is exploratory: it does not support significance or equivalence claims. Randomization does
not eliminate build-order, cache, host or independent-profile variation in a single allocation.

Retain preparation, instrumented compilation, training workloads, profile merging, overall PGO
helper and final-build wall seconds. Combined time is the helper total plus final compilation and
initial wheel construction. Setup, symbol postprocessing, verification and upload are outside that
timer. Native timings are not measurements of the complete production release pipeline or its
manylinux jobs on a smaller Linux runner.

Checks cover effective compiler flags in both compilation phases, source/plan/order provenance,
separate PGO profiles and executable hashes, processed-wheel contents and sizes, companion symbol
sizes, Rust/AWS-LC/jitterentropy source and negative lookups, SBOM retention, wheel installation,
smoke tests, Windows static CRT and macOS signatures. Retain all missing/mismatched-profile
diagnostics and sampled resource traces, including sampling gaps and unavailable counters.

Jupyter and Trio each use four warmups and forty timed samples per treatment in balanced randomized
order. Require identical outputs within each platform. Timed resolutions characterize the four
produced binaries; they are not independent build or machine replications.

```shell
gh workflow run debug-symbols-study.yml --repo astral-sh/uv \
  --ref ee/publish-debug-symbols \
  -f phase=all-debug-pilot -f platform=all -f block=0
```

Runner outputs use `target/debug-symbols-all-debug-pilot`. Artifact names are
`debug-study-all-debug-pilot-0-{Linux-X64,Linux-ARM64,macOS-ARM64,Windows-X64}`. Collect by artifact
ID, exact commit, job, attempt, upload time and archive digest, retaining raw reports, logs,
profiles, executables, symbols, wheels and resources beneath
`target/debug-symbols-experiment/all-debug-pilot-2026-10-13/`. Independent variants are attempted
even if another build fails; failures remain visible and fail the job. The top-level report.json
contains final verification; per-variant reports are earlier build snapshots.

The [completed lower-debug pilot](results/2026-10-13-lower-debug-pilot/README.md) and
[Windows no/full-debug LTO pilot](results/2026-10-13-lto-pilot/README.md) remain separate historical
observations on the same pinned source. Their baselines are not substituted into this new
allocation. The older statistical study uses an earlier source revision and fat instrumentation; it
remains unchanged. This dispatch runs sixteen fresh builds across four allocations, with no
production release jobs, fat-instrumentation controls or confirmatory-study blocks.
