# Lower debug levels with thin-LTO instrumentation

[Results and retained measurements](results/2026-10-13-lower-debug-pilot/README.md) are available
for Linux x86-64, Linux ARM64 and macOS. Windows remains in progress.

Compare no debug information, Rust line tables and limited Rust debug information on four native
platforms. Each platform gets one independent runner allocation and three fresh builds, each with
its own PGO profile. Instrumentation uses thin LTO; final compilation uses fat LTO, optimization
level 3, one codegen unit and panic abort. Windows retains static CRT linkage.

The [registered plan](lower-debug-pilot-plan.json) pins main revision
`59e5912ecff500422b99cca3df68d48be3841a67`, the same compiled source as the
[Windows no/full-debug LTO pilot](results/2026-10-13-lto-pilot/README.md). Production release
profiles and the production PGO helper are unchanged. Native C debug flags can remain full even when
Rust uses a reduced debug level.

| Platform       | Runner profile                                | Cargo jobs | Predeclared build order    |
| -------------- | --------------------------------------------- | ---------: | -------------------------- |
| Linux x86-64   | `depot-ubuntu-24.04-16`                       |          8 | line tables, none, limited |
| Linux ARM64    | `depot-ubuntu-24.04-arm-16`                   |          8 | none, line tables, limited |
| macOS ARM64    | `namespace-profile-macos-15`                  |          4 | line tables, limited, none |
| Windows x86-64 | `namespace-profile-windows-2022-x86-64-16x32` |          4 | none, limited, line tables |

Orders are independently shuffled with `random.Random(f"2026101302:{target}")` before dispatch. All
four allocations may run in parallel; builds within each allocation run sequentially. Each
reduced-debug build is compared with the no-debug build from its own allocation. There is only one
build observation per treatment per platform: these are descriptive comparisons, not confidence
intervals or equivalence tests. Build position and temporal runner variation remain possible
confounders. The earlier full-debug Windows result is context, not a replacement paired control.
These native measurements do not measure the complete release pipeline or production manylinux CI on
its smaller Linux runner.

The pilot records preparation, instrumented compilation, training workloads, profile merge and
final-build wall times, plus overall PGO-helper time. Report combined PGO-helper and final-build
times against the paired baseline. Symbol postprocessing, verification, setup and artifact upload
are outside those timers. Resource traces retain sampled process-tree memory, CPU and available I/O
counters; sampling gaps and unavailable counters must remain visible.

Validation covers effective Rust flags in instrumentation and final compiler logs, source
fingerprints, separate PGO profiles, executable and wheel hashes/sizes, separate symbol files,
Rust/AWS-LC/jitterentropy lookups, negative lookups with symbols hidden, SBOM retention, wheel
installation, smoke checks, Windows static CRT and macOS signatures. Missing and mismatched PGO
diagnostics are retained; warning counts alone do not establish profile coverage.

Jupyter and Trio each use three warmups and thirty timed samples per treatment in balanced
randomized order, with identical resolver outputs required within each platform. Runtime samples
describe the produced binaries and are not independent build or runner replications.

```shell
gh workflow run debug-symbols-study.yml --repo astral-sh/uv \
  --ref ee/publish-debug-symbols \
  -f phase=lower-debug-pilot -f platform=all -f block=0
```

Outputs use `target/debug-symbols-lower-debug-pilot` on each runner. Artifact names are
`debug-study-lower-debug-pilot-0-{Linux-X64,Linux-ARM64,macOS-ARM64,Windows-X64}`. Collect each
artifact by ID, exact commit, job, attempt, digest and upload time. Retain the authoritative
top-level `report.json`, per-stage resource traces, compiler logs and any failure evidence
separately under `target/debug-symbols-experiment/lower-debug-pilot-2026-10-13/`. Independent
variants are attempted even if another build fails; any failure still fails the job.

This dispatch includes no full-debug variants, fat-instrumentation controls, production release jobs
or statistical-study blocks. The completed statistical study and previous pilot evidence remain
separate.
