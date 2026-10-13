# All-debug thin-LTO pilot: October 13, 2026

Verified results are available for 3 of four native allocations. Still running: Windows x86-64.

All builds use thin-LTO instrumentation and fat-LTO final compilation. Each platform has one fresh
no-debug control, one line-table, one limited-debug and one full-debug build, with independent PGO
training. This is one exploratory allocation per platform, not a statistical significance or
equivalence result.

[Workflow run](https://github.com/astral-sh/uv/actions/runs/38437984289), attempt 1. Experiment
commit: `a77b32c17fa3173e4ea3a3d52bbe91f3dd421a98`; pinned main:
`59e5912ecff500422b99cca3df68d48be3841a67`. Source fingerprints match the completed
[Windows no/full-debug LTO pilot](../2026-10-13-lto-pilot/README.md). Rust 1.99.0 / LLVM 23.1.1,
Maturin 1.15.0, one codegen unit, optimization level 3 and panic abort are fixed. Windows uses
static CRT. The [plan](../../all-debug-pilot-plan.json) fixes profiles, Cargo jobs and randomized
build orders before dispatch.

## Combined build and training time

Cells show the same allocation's no-debug baseline → symbols time and paired overhead. Total is
PGO-helper time plus final compilation/initial wheel construction. Setup, symbol postprocessing,
verification and upload are excluded.

| Platform       | Line tables                | Limited                    | Full                       |
| -------------- | -------------------------- | -------------------------- | -------------------------- |
| Linux x86-64   | 8m 29s → 9m 37s (+13.5%)   | 8m 29s → 9m 42s (+14.4%)   | 8m 29s → 12m 58s (+52.9%)  |
| Linux ARM64    | 10m 45s → 11m 53s (+10.5%) | 10m 45s → 12m 15s (+14.1%) | 10m 45s → 15m 30s (+44.3%) |
| macOS ARM64    | 9m 13s → 10m 18s (+11.7%)  | 9m 13s → 10m 23s (+12.6%)  | 9m 13s → 16m 24s (+78.0%)  |
| Windows x86-64 | Pending                    | Pending                    | Pending                    |

## Raw wall times

Seconds, rounded to three decimals; machine-readable measurements retain full precision. The PGO
helper includes preparation, instrumented compilation, training workloads, profile merging and small
helper overhead. Final compilation remains fat LTO for every treatment.

| Platform     | Debug       | Preparation | Instrumented | Training | Merge | PGO helper | Final   | Total   |
| ------------ | ----------- | ----------- | ------------ | -------- | ----- | ---------- | ------- | ------- |
| Linux x86-64 | No debug    | 0.004       | 188.705      | 26.541   | 0.844 | 216.170    | 292.457 | 508.627 |
| Linux x86-64 | Line tables | 0.004       | 214.811      | 28.537   | 0.842 | 244.271    | 332.876 | 577.146 |
| Linux x86-64 | Limited     | 0.004       | 214.699      | 29.746   | 0.818 | 245.345    | 336.687 | 582.031 |
| Linux x86-64 | Full        | 0.004       | 273.829      | 29.238   | 0.854 | 304.002    | 473.927 | 777.929 |
| Linux ARM64  | No debug    | 0.005       | 248.208      | 21.118   | 0.864 | 270.296    | 374.549 | 644.844 |
| Linux ARM64  | Line tables | 0.005       | 270.975      | 21.428   | 0.851 | 293.356    | 419.519 | 712.875 |
| Linux ARM64  | Limited     | 0.006       | 273.287      | 40.023   | 0.823 | 314.236    | 421.252 | 735.488 |
| Linux ARM64  | Full        | 0.006       | 333.591      | 20.469   | 0.839 | 355.004    | 575.336 | 930.340 |
| macOS ARM64  | No debug    | 0.014       | 230.916      | 50.172   | 0.434 | 281.629    | 271.391 | 553.020 |
| macOS ARM64  | Line tables | 0.009       | 263.721      | 46.523   | 0.449 | 310.823    | 306.867 | 617.691 |
| macOS ARM64  | Limited     | 0.012       | 262.703      | 48.094   | 0.408 | 311.323    | 311.456 | 622.779 |
| macOS ARM64  | Full        | 0.013       | 339.914      | 55.667   | 0.400 | 396.098    | 588.368 | 984.466 |

## Shipped sizes and separate symbols

Raw bytes. Symbols are separate from shipped executables and processed wheels. Each delta uses this
allocation's no-debug executable or wheel, not a historical build.

Main `uv` executable:

| Platform       | No debug | Line tables                 | Limited                     | Full                        |
| -------------- | -------- | --------------------------- | --------------------------- | --------------------------- |
| Linux x86-64   | 39540512 | 39814032 (+273520; +0.692%) | 39550384 (+9872; +0.025%)   | 39833456 (+292944; +0.741%) |
| Linux ARM64    | 33467664 | 33686488 (+218824; +0.654%) | 33703736 (+236072; +0.705%) | 33499832 (+32168; +0.096%)  |
| macOS ARM64    | 29274128 | 29327040 (+52912; +0.181%)  | 29542240 (+268112; +0.916%) | 29558976 (+284848; +0.973%) |
| Windows x86-64 | Pending  | Pending                     | Pending                     | Pending                     |

Processed wheel:

| Platform       | No debug | Line tables                 | Limited                     | Full                        |
| -------------- | -------- | --------------------------- | --------------------------- | --------------------------- |
| Linux x86-64   | 17397903 | 17532355 (+134452; +0.773%) | 17410983 (+13080; +0.075%)  | 17516255 (+118352; +0.680%) |
| Linux ARM64    | 16469085 | 16589666 (+120581; +0.732%) | 16587544 (+118459; +0.719%) | 16488398 (+19313; +0.117%)  |
| macOS ARM64    | 14953280 | 14968460 (+15180; +0.102%)  | 15080383 (+127103; +0.850%) | 15094949 (+141669; +0.947%) |
| Windows x86-64 | Pending  | Pending                     | Pending                     | Pending                     |

Separate companion files, decimal MB. Values in each cell are `uv / uvx / uvw`; uvw is only present
on Windows. Exact bytes and executable identities are in the measurements.

| Platform       | Line tables     | Limited         | Full            |
| -------------- | --------------- | --------------- | --------------- |
| Linux x86-64   | 169.656 / 1.953 | 263.628 / 1.995 | 587.440 / 2.115 |
| Linux ARM64    | 180.666 / 2.127 | 277.048 / 2.168 | 593.957 / 2.283 |
| macOS ARM64    | 187.153 / 2.558 | 300.758 / 2.608 | 581.238 / 2.728 |
| Windows x86-64 | Pending         | Pending         | Pending         |

Profile-driven code generation can produce different shipped bytes and sizes. These observations do
not establish byte-identical binaries, and the small runtime changes below cannot be assigned solely
to debug metadata.

## Resolver runtime

Median milliseconds from forty timed samples after four warmups per treatment, in balanced
randomized order. Each symbols cell includes its percentage change against the no-debug median.
Every output matched within its platform. Repeated resolutions characterize these binaries, not
independent build or host replications.

Jupyter:

| Platform       | No debug | Line tables     | Limited         | Full            |
| -------------- | -------- | --------------- | --------------- | --------------- |
| Linux x86-64   | 8.763    | 8.793 (+0.33%)  | 8.673 (-1.03%)  | 8.705 (-0.67%)  |
| Linux ARM64    | 10.388   | 10.337 (-0.50%) | 10.323 (-0.63%) | 10.371 (-0.16%) |
| macOS ARM64    | 11.034   | 11.076 (+0.38%) | 11.037 (+0.03%) | 11.098 (+0.59%) |
| Windows x86-64 | Pending  | Pending         | Pending         | Pending         |

Trio:

| Platform       | No debug | Line tables    | Limited        | Full           |
| -------------- | -------- | -------------- | -------------- | -------------- |
| Linux x86-64   | 7.298    | 6.993 (-4.19%) | 7.119 (-2.46%) | 7.040 (-3.53%) |
| Linux ARM64    | 8.019    | 7.971 (-0.60%) | 7.957 (-0.76%) | 7.910 (-1.35%) |
| macOS ARM64    | 9.253    | 9.211 (-0.46%) | 9.251 (-0.02%) | 9.163 (-0.97%) |
| Windows x86-64 | Pending  | Pending        | Pending        | Pending        |

## Sampled memory and runner evidence

Decimal GB. Cells show PGO-phase / final-build sampled process-tree RSS peaks. Summed RSS can
double-count shared pages and misses peaks between samples; it is not exact physical-memory
consumption. PGO-phase memory includes instrumentation and workload training.

| Platform       | No debug        | Line tables     | Limited         | Full            |
| -------------- | --------------- | --------------- | --------------- | --------------- |
| Linux x86-64   | 11.569 / 12.094 | 16.302 / 15.544 | 16.565 / 12.614 | 24.247 / 23.603 |
| Linux ARM64    | 10.939 / 11.059 | 16.677 / 15.659 | 16.628 / 15.656 | 24.850 / 22.352 |
| macOS ARM64    | 6.489 / 8.069   | 8.439 / 10.194  | 8.707 / 10.263  | 12.469 / 19.493 |
| Windows x86-64 | Pending         | Pending         | Pending         | Pending         |

| Platform     | Logical CPUs | CPU model              | Reported physical RAM (bytes) | Cargo jobs | Job                                                                               |
| ------------ | ------------ | ---------------------- | ----------------------------- | ---------- | --------------------------------------------------------------------------------- |
| Linux x86-64 | 16           | AMD EPYC 9R45          | 66149724160                   | 8          | [Logs](https://github.com/astral-sh/uv/actions/runs/38437984289/job/115436950535) |
| Linux ARM64  | 16           | Neoverse-V2            | 66192830464                   | 8          | [Logs](https://github.com/astral-sh/uv/actions/runs/38437984289/job/115436950528) |
| macOS ARM64  | 12           | Apple M5 Max (Virtual) | 30064771072                   | 4          | [Logs](https://github.com/astral-sh/uv/actions/runs/38437984289/job/115436950470) |

Maximum sampling gap in collected phases: 1.110s. Resource series retain sampled child CPU/I/O
counters, host memory and disk counters, and unavailable-counter errors. macOS process I/O counters
are unavailable; short-lived-process errors also occur. These measurements do not establish a
paging, throttling or I/O-pressure explanation for any timing difference. All collected builds
completed without compiler OOM.

## PGO and correctness

Missing-profile warnings, no debug / line tables / limited / full:

| Platform       | No debug | Line tables | Limited | Full    |
| -------------- | -------- | ----------- | ------- | ------- |
| Linux x86-64   | 18       | 18          | 18      | 18      |
| Linux ARM64    | 18       | 18          | 18      | 18      |
| macOS ARM64    | 3787     | 3823        | 3819    | 3845    |
| Windows x86-64 | Pending  | Pending     | Pending | Pending |

All collected profile mismatch counts are zero. macOS still has substantial missing-profile
diagnostics, including dependencies; fewer warnings alone do not demonstrate better coverage. Each
completed allocation has four independently built/trained profiles with distinct hashes and one
completed preparation, instrumentation, training and merge stage per treatment.

All collected variants passed native installation and smoke checks. Symbols passed Rust source
lookup for every executable, AWS-LC and linked jitterentropy lookup in uv, and negative lookup with
companions hidden. SBOM hashes match across treatments within each platform. Artifact digest and
upload-job/attempt/time provenance, source/plan/order, executable/profile/wheel hashes and sizes,
companion sizes, both stages' effective compiler flags, resource summaries, and benchmark
order/samples/medians/output hashes were verified. Downloaded macOS signatures passed locally. Linux
and Windows binaries were not executed on the collecting Mac.

## Comparison with the earlier October 13 pilots

The earlier lower-debug pilot shares compiled-source fingerprints, tool versions, Python version and
resolver input/output hashes with the completed rows here. Each cell compares independently paired
overhead observations; neither run borrows the other allocation's baseline. Runtime sampling changed
from three warmups/thirty timings to four/forty, and build orders and independently trained profiles
differ.

| Platform     | No-debug total: earlier → current | Line overhead: earlier → current | Limited overhead: earlier → current |
| ------------ | --------------------------------- | -------------------------------- | ----------------------------------- |
| Linux x86-64 | 10m 22s → 8m 29s                  | +13.3% → +13.5%                  | +12.7% → +14.4%                     |
| Linux ARM64  | 10m 17s → 10m 45s                 | +10.4% → +10.5%                  | +12.6% → +14.1%                     |
| macOS ARM64  | 9m 15s → 9m 13s                   | +12.5% → +11.7%                  | +13.5% → +12.6%                     |

Linux x86-64 used the same requested Depot profile in both runs, but reported different machines:
Intel(R) Xeon(R) 6975P-C with 132,916,703,232 bytes RAM earlier, and AMD EPYC 9R45 with
66,149,724,160 bytes now. The requested label did not hold hardware identical across allocations.
This is direct evidence of allocation differences, not proof of a single cause for every timing
change; retain each run's baseline and avoid interpreting absolute speed differences as debug-level
effects.

The earlier Windows no/full-debug pilot also used this pinned source, but its full-debug observation
came from another allocation. Older all-platform full-debug results use fat instrumentation and an
earlier source revision; they are historical context, not current thin-LTO full-debug measurements.

## Interpretation and limits

Compare each debug level with its same-allocation baseline. Build order was randomized in advance
but only one order is observed on each platform, so order/cache/host variation and independent PGO
training remain possible influences. Native C debug settings can remain full when Rust uses line
tables or limited information. The older fat-instrumentation study uses an earlier uv source
revision and different allocations; subtracting its timings would not isolate the LTO effect. The
completed Windows full-debug pilot shares source but a different allocation and is historical
context only.

These are native optimized-build measurements, not the duration of production manylinux CI or the
complete release pipeline. Production Linux uses a smaller runner. No production workflow/profile or
statistical study was changed or dispatched for this pilot.

## Retained evidence

[Normalized measurements and all timed samples](measurements.json),
[paired differences](comparison.json), [historical comparisons](historical-comparison.json) and
[provenance manifest](manifest.json) accompany this report. Raw authoritative reports, archives,
metadata, job logs, binaries, profiles, symbols and resource series are retained under
`target/debug-symbols-experiment/all-debug-pilot-2026-10-13/`. Top-level report.json contains final
verification; per-variant reports are earlier build snapshots.

Canonical plan SHA256: `2549e7992ccfb59b2f9dde8d07b752d6a0a53982e52a06323da57a78ce9b7cae`.

| Platform     | Artifact ID | Uploaded UTC         | Archive SHA256                                                     |
| ------------ | ----------- | -------------------- | ------------------------------------------------------------------ |
| Linux x86-64 | 11836463891 | 2026-10-13T18:04:48Z | `f46b314eb2dcb6ce2a5ae975f92aa113393d024a5b355e2e09f761674456d6b3` |
| Linux ARM64  | 11837663272 | 2026-10-13T18:14:47Z | `2ac1dd86c4708003f04439271517c69ebba557ccdabf2fa11427b8042edd31ba` |
| macOS ARM64  | 11837727586 | 2026-10-13T18:10:16Z | `b33c1ff825af17591519142db6083ae86286c7160b19c86556bb192e15b9ea31` |
