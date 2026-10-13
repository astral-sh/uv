# Lower-debug thin-LTO pilot: October 13, 2026

Verified results are available for 3 of four native allocations. Still running: Windows x86-64.

All builds use thin-LTO instrumentation and fat-LTO final compilation. Each platform has one fresh
no-debug control, one line-table build and one limited-debug build, with independent PGO training.
This is one exploratory allocation per platform, not a statistical significance or equivalence
result.

[Workflow run](https://github.com/astral-sh/uv/actions/runs/38422747719), attempt 1. Experiment
commit: `c17ce3c58fb55f2c27b84591bb955648e0fca8ed`; pinned main:
`59e5912ecff500422b99cca3df68d48be3841a67`. Source fingerprints match the completed
[Windows no/full-debug LTO pilot](../2026-10-13-lto-pilot/README.md). Rust 1.99.0 / LLVM 23.1.1,
Maturin 1.15.0, one codegen unit, optimization level 3 and panic abort are fixed. Windows uses
static CRT. The [plan](../../lower-debug-pilot-plan.json) fixes profiles, Cargo jobs and randomized
build orders before dispatch.

## Combined build and training time

Cells show the same allocation's no-debug baseline → symbols time and paired overhead. Total is
PGO-helper time plus final compilation/initial wheel construction. Setup, symbol postprocessing,
verification and upload are excluded.

| Platform       | Line tables                | Limited                    |
| -------------- | -------------------------- | -------------------------- |
| Linux x86-64   | 10m 22s → 11m 45s (+13.3%) | 10m 22s → 11m 41s (+12.7%) |
| Linux ARM64    | 10m 17s → 11m 21s (+10.4%) | 10m 17s → 11m 34s (+12.6%) |
| macOS ARM64    | 9m 15s → 10m 24s (+12.5%)  | 9m 15s → 10m 30s (+13.5%)  |
| Windows x86-64 | Pending                    | Pending                    |

## Raw wall times

Seconds, rounded to three decimals; machine-readable measurements retain full precision. The PGO
helper includes preparation, instrumented compilation, training workloads, profile merging and small
helper overhead. Final compilation remains fat LTO for every treatment.

| Platform     | Debug       | Preparation | Instrumented | Training | Merge | PGO helper | Final   | Total   |
| ------------ | ----------- | ----------- | ------------ | -------- | ----- | ---------- | ------- | ------- |
| Linux x86-64 | No debug    | 0.004       | 228.573      | 28.687   | 0.960 | 258.323    | 363.736 | 622.060 |
| Linux x86-64 | Line tables | 0.004       | 260.789      | 28.797   | 0.973 | 290.663    | 414.300 | 704.963 |
| Linux x86-64 | Limited     | 0.004       | 257.012      | 28.389   | 0.888 | 286.394    | 414.400 | 700.794 |
| Linux ARM64  | No debug    | 0.005       | 236.646      | 19.642   | 0.795 | 257.184    | 359.599 | 616.783 |
| Linux ARM64  | Line tables | 0.005       | 260.994      | 18.820   | 0.802 | 280.716    | 400.477 | 681.192 |
| Linux ARM64  | Limited     | 0.005       | 262.901      | 18.761   | 0.810 | 282.572    | 411.666 | 694.238 |
| macOS ARM64  | No debug    | 0.013       | 230.331      | 53.441   | 0.414 | 284.294    | 270.667 | 554.961 |
| macOS ARM64  | Line tables | 0.010       | 264.648      | 51.994   | 0.454 | 317.222    | 307.028 | 624.250 |
| macOS ARM64  | Limited     | 0.013       | 263.151      | 55.663   | 0.399 | 319.331    | 310.754 | 630.085 |

## Shipped sizes and separate symbols

Raw bytes. Symbols are separate from shipped executables and processed wheels. Each delta uses this
allocation's no-debug executable or wheel, not a historical build.

Main `uv` executable:

| Platform       | No debug | Line tables                 | Limited                     |
| -------------- | -------- | --------------------------- | --------------------------- |
| Linux x86-64   | 39563168 | 39812400 (+249232; +0.630%) | 39553456 (-9712; -0.025%)   |
| Linux ARM64    | 33475920 | 33686488 (+210568; +0.629%) | 33699352 (+223432; +0.667%) |
| macOS ARM64    | 29274128 | 29327056 (+52928; +0.181%)  | 29542240 (+268112; +0.916%) |
| Windows x86-64 | Pending  | Pending                     | Pending                     |

Processed wheel:

| Platform       | No debug | Line tables                 | Limited                     |
| -------------- | -------- | --------------------------- | --------------------------- |
| Linux x86-64   | 17404299 | 17534206 (+129907; +0.746%) | 17405446 (+1147; +0.007%)   |
| Linux ARM64    | 16473747 | 16591360 (+117613; +0.714%) | 16588064 (+114317; +0.694%) |
| macOS ARM64    | 14953236 | 14977953 (+24717; +0.165%)  | 15084278 (+131042; +0.876%) |
| Windows x86-64 | Pending  | Pending                     | Pending                     |

Separate companion files, decimal MB. Values in each cell are `uv / uvx / uvw`; uvw is only present
on Windows. Exact bytes and executable identities are in the measurements.

| Platform       | Line tables     | Limited         |
| -------------- | --------------- | --------------- |
| Linux x86-64   | 169.646 / 1.953 | 263.586 / 1.995 |
| Linux ARM64    | 180.692 / 2.127 | 277.041 / 2.168 |
| macOS ARM64    | 187.179 / 2.558 | 300.749 / 2.608 |
| Windows x86-64 | Pending         | Pending         |

Profile-driven code generation can produce different shipped bytes and sizes. These observations do
not establish byte-identical binaries, and the small runtime changes below cannot be assigned solely
to debug metadata.

## Resolver runtime

Median milliseconds from thirty timed samples after three warmups per treatment, in balanced
randomized order. Each symbols cell includes its percentage change against the no-debug median.
Every output matched within its platform. Repeated resolutions characterize these binaries, not
independent build or host replications.

Jupyter:

| Platform       | No debug | Line tables     | Limited         |
| -------------- | -------- | --------------- | --------------- |
| Linux x86-64   | 13.319   | 13.298 (-0.16%) | 13.271 (-0.36%) |
| Linux ARM64    | 10.253   | 10.196 (-0.56%) | 10.157 (-0.93%) |
| macOS ARM64    | 11.192   | 11.210 (+0.16%) | 11.195 (+0.03%) |
| Windows x86-64 | Pending  | Pending         | Pending         |

Trio:

| Platform       | No debug | Line tables     | Limited         |
| -------------- | -------- | --------------- | --------------- |
| Linux x86-64   | 10.767   | 10.676 (-0.84%) | 10.599 (-1.56%) |
| Linux ARM64    | 7.884    | 7.812 (-0.90%)  | 7.763 (-1.53%)  |
| macOS ARM64    | 9.044    | 9.067 (+0.26%)  | 9.082 (+0.42%)  |
| Windows x86-64 | Pending  | Pending         | Pending         |

## Sampled memory and runner evidence

Decimal GB. Cells show PGO-phase / final-build sampled process-tree RSS peaks. Summed RSS can
double-count shared pages and misses peaks between samples; it is not exact physical-memory
consumption. PGO-phase memory includes instrumentation and workload training.

| Platform       | No debug        | Line tables     | Limited         |
| -------------- | --------------- | --------------- | --------------- |
| Linux x86-64   | 11.542 / 10.784 | 15.874 / 15.368 | 14.540 / 15.267 |
| Linux ARM64    | 11.261 / 11.165 | 16.848 / 15.199 | 16.761 / 15.332 |
| macOS ARM64    | 6.510 / 8.059   | 8.454 / 10.216  | 8.651 / 10.405  |
| Windows x86-64 | Pending         | Pending         | Pending         |

| Platform     | Logical CPUs | Reported physical RAM (bytes) | Cargo jobs | Job                                                                               |
| ------------ | ------------ | ----------------------------- | ---------- | --------------------------------------------------------------------------------- |
| Linux x86-64 | 16           | 132916703232                  | 8          | [Logs](https://github.com/astral-sh/uv/actions/runs/38422747719/job/115384153263) |
| Linux ARM64  | 16           | 66192830464                   | 8          | [Logs](https://github.com/astral-sh/uv/actions/runs/38422747719/job/115384153257) |
| macOS ARM64  | 12           | 30064771072                   | 4          | [Logs](https://github.com/astral-sh/uv/actions/runs/38422747719/job/115384153836) |

Maximum sampling gap in collected phases: 1.111s. Resource series retain sampled child CPU/I/O
counters, host memory and disk counters, and unavailable-counter errors. macOS process I/O counters
are unavailable; short-lived-process errors also occur. These measurements do not establish a
paging, throttling or I/O-pressure explanation for any timing difference. All collected builds
completed without compiler OOM.

## PGO and correctness

Missing-profile warnings, no debug / line tables / limited:

| Platform       | No debug | Line tables | Limited |
| -------------- | -------- | ----------- | ------- |
| Linux x86-64   | 18       | 18          | 18      |
| Linux ARM64    | 18       | 18          | 18      |
| macOS ARM64    | 3787     | 3823        | 3819    |
| Windows x86-64 | Pending  | Pending     | Pending |

All collected profile mismatch counts are zero. macOS still has substantial missing-profile
diagnostics, including dependencies; fewer warnings alone do not demonstrate better coverage. Each
completed allocation has three independently built/trained profiles with distinct hashes and one
completed preparation, instrumentation, training and merge stage per treatment.

All collected variants passed native installation and smoke checks. Symbols passed Rust source
lookup for every executable, AWS-LC and linked jitterentropy lookup in uv, and negative lookup with
companions hidden. SBOM hashes match across treatments within each platform. Artifact digest and
upload-job/attempt/time provenance, source/plan/order, executable/profile/wheel hashes and sizes,
companion sizes, both stages' effective compiler flags, resource summaries, and benchmark
order/samples/medians/output hashes were verified. Downloaded macOS signatures passed locally.
Windows static CRT and import evidence will accompany its completed results; Linux and Windows
binaries are never executed on the collecting Mac.

## Interpretation and limits

Compare each reduced debug level with its same-allocation baseline. Build order was randomized in
advance but only one order is observed on each platform, so order/cache/host variation and
independent PGO training remain possible influences. Native C debug settings can remain full when
Rust uses line tables or limited information. The older fat-instrumentation study uses an earlier uv
source revision and different allocations; subtracting its timings would not isolate the LTO effect.
The completed Windows full-debug pilot shares source but a different allocation and is historical
context only.

These are native optimized-build measurements, not the duration of production manylinux CI or the
complete release pipeline. Production Linux uses a smaller runner. No production workflow/profile or
statistical study was changed or dispatched for this pilot.

## Retained evidence

[Normalized measurements and all timed samples](measurements.json),
[paired differences](comparison.json) and [provenance manifest](manifest.json) accompany this
report. Raw authoritative reports, archives, metadata, job logs, binaries, profiles, symbols and
resource series are retained under `target/debug-symbols-experiment/lower-debug-pilot-2026-10-13/`.
Top-level report.json contains final verification; per-variant reports are earlier build snapshots.

Canonical plan SHA256: `4c1cba7a8fc99bb04170bf8ae556644db6d27c39b23af239452391d78c47d628`.

| Platform     | Artifact ID | Uploaded UTC         | Archive SHA256                                                     |
| ------------ | ----------- | -------------------- | ------------------------------------------------------------------ |
| Linux x86-64 | 11829531764 | 2026-10-13T16:07:41Z | `2cceff44a03060aff0cf0b1c90bfd6a2db1b234dda109c52d59511f7a28ebe02` |
| Linux ARM64  | 11828903028 | 2026-10-13T16:06:51Z | `c371e842ed573bc053d25a7460d9bd7663780f2169a100e05a26c2db0bf0317f` |
| macOS ARM64  | 11829491089 | 2026-10-13T16:03:20Z | `d633c388156b947f4561a4b9318d2a3d6d3516e8bc7d5a20d1261a07f6d6ed3f` |
