# Randomized debug-symbol study: October 10, 2026

Fourteen independent runner allocations per platform completed all five treatments: two no-debug
controls, line tables, limited and full debug. All 56 allocations and 280 builds passed
verification. The first amended statistical look resolved all 84 timing contrasts; no further
collection was required.

The build comparison uses optimized PGO builds at experiment revision
`c226dc06577e8e27fd11acdb02cf67998d3f6508`, based on main revision
`238d6ba651d13f0dfddab0cc1826f963cdf21711`. Every mode uses one codegen unit, optimization level 3,
fat LTO and panic abort. Each mode has a fresh target directory and independently trains its PGO
profile. Linux has eight Cargo jobs; macOS and Windows have four. Production release profiles and
workflows are unchanged.

Line tables and limited have equivalent build costs within the selected ±10% practical margin on
every platform. Their median difference is below 0.5%. Full debug costs substantially more. Every
resolver contrast, including the controls, establishes equivalence within ±3%. Equivalence does not
mean exactly equal: some small effects also exclude zero.

## Method and analysis amendment

Build outcome includes instrumented compilation, PGO training, final compilation and wheel
construction. Symbol extraction, verification and upload are outside this timer. Treatment order is
randomized independently within each allocation. Resolver runs have five warmups and fifty timed
samples per mode, in randomized balanced order; the allocation, not each timed sample, is the
independent unit.

Each no-debug reference is the geometric mean of A and B within the same allocation. Effects are
medians of within-allocation log ratios, transformed back to percentages. No baselines are
substituted across allocations or platforms.

The original protocol used a ±5% build margin and looks at 20, 40, 80, … allocations per platform.
The [dated amendment](analysis-amendment.json), recorded after collection began but before any
confirmatory inference, sets the build margin to ±10% and looks to 14, 28, 56, …. The runtime margin
remains ±3%. The amendment followed a separately specified practical tolerance; confirmatory effects
and intervals were not inspected to choose it. Original plans, data and fingerprints are retained.
This is an explicit post-start amendment, not the original preregistration.

Intervals are exact binomial/order-statistic median intervals, Bonferroni-adjusted across 84
contrasts. The first look spends 0.025 family error; future looks would spend successively half as
much. Under independent, representative, identically distributed allocation assumptions, the family
error across all planned looks is at most 0.05. At 14 observations the reported endpoints are the
most extreme observed paired log ratios: intervals remain conservative and no slow observation is
removed. These intervals describe population median effects, not prediction bounds for a future job.

All 72 treatment contrasts establish either a difference or practical equivalence; all 12 A/A
control contrasts establish equivalence. The complete numerical family is in
[analysis.json](analysis.json).

## Build time

Median wall times across the fourteen allocations. These descriptive medians are not substituted
into the paired inference.

| Platform       | No debug A | No debug B | Line tables | Limited | Full    |
| -------------- | ---------- | ---------- | ----------- | ------- | ------- |
| Linux x86-64   | 13m 47s    | 13m 44s    | 15m 27s     | 15m 24s | 20m 35s |
| Linux ARM64    | 18m 23s    | 18m 06s    | 19m 41s     | 19m 44s | 26m 12s |
| macOS ARM64    | 12m 39s    | 12m 42s    | 14m 09s     | 14m 12s | 32m 59s |
| Windows x86-64 | 21m 03s    | 21m 04s    | 23m 48s     | 23m 54s | 33m 52s |

Paired overhead versus the within-allocation no-debug reference; brackets are simultaneous interval
endpoints, in percent.

| Platform       | Line tables              | Limited                  | Full                        |
| -------------- | ------------------------ | ------------------------ | --------------------------- |
| Linux x86-64   | +11.73% [+10.95, +19.81] | +11.97% [+11.21, +13.72] | +49.56% [+47.51, +56.82]    |
| Linux ARM64    | +8.56% [+5.99, +9.28]    | +8.68% [+3.42, +10.61]   | +44.29% [+43.10, +49.46]    |
| macOS ARM64    | +11.68% [+10.71, +12.79] | +11.81% [+11.22, +13.89] | +158.62% [+155.88, +164.91] |
| Windows x86-64 | +13.13% [+11.87, +13.97] | +13.70% [+11.62, +14.01] | +60.80% [+59.29, +62.38]    |

Additional build comparisons, with the second mode relative to the first:

| Platform       | Limited / line tables | Full / line tables          | Full / limited              |
| -------------- | --------------------- | --------------------------- | --------------------------- |
| Linux x86-64   | +0.22% [-7.16, +1.51] | +33.58% [+30.88, +35.05]    | +33.45% [+31.93, +40.98]    |
| Linux ARM64    | +0.30% [-5.36, +3.70] | +33.27% [+32.48, +36.77]    | +32.60% [+30.10, +42.71]    |
| macOS ARM64    | +0.49% [-0.35, +1.33] | +131.67% [+127.84, +136.16] | +131.60% [+128.63, +135.23] |
| Windows x86-64 | +0.35% [-0.35, +1.69] | +42.14% [+41.21, +43.67]    | +41.71% [+39.92, +43.97]    |

Stage medians: **instrumented compilation + training / final build**. Separately computed stage
medians need not add exactly to the median total.

| Platform       | No debug A       | No debug B       | Line tables      | Limited          | Full              |
| -------------- | ---------------- | ---------------- | ---------------- | ---------------- | ----------------- |
| Linux x86-64   | 8m 50s / 4m 56s  | 8m 50s / 4m 55s  | 9m 50s / 5m 36s  | 9m 47s / 5m 38s  | 12m 43s / 7m 52s  |
| Linux ARM64    | 11m 57s / 6m 25s | 11m 46s / 6m 20s | 12m 42s / 7m 02s | 12m 40s / 7m 04s | 16m 21s / 9m 53s  |
| macOS ARM64    | 8m 06s / 4m 34s  | 8m 09s / 4m 34s  | 8m 58s / 5m 13s  | 8m 58s / 5m 14s  | 23m 01s / 9m 54s  |
| Windows x86-64 | 13m 01s / 8m 01s | 13m 02s / 8m 02s | 14m 41s / 9m 07s | 14m 42s / 9m 12s | 20m 50s / 12m 59s |

## Executables, wheels and companions

Sizes use decimal MB (1 MB = 1,000,000 bytes). Shipped-size cells give the median size, median
paired byte change and median paired percentage change versus the geometric mean of A/B sizes on
that allocation. These are descriptive measurements, without additional significance claims.
Separate companion files are absent from processed wheels. The stripped executables are not
byte-identical across modes.

### Main uv executable

| Platform       | No debug A | No debug B | Line tables                 | Limited                     | Full                        |
| -------------- | ---------- | ---------- | --------------------------- | --------------------------- | --------------------------- |
| Linux x86-64   | 39.807     | 39.808     | 39.795; -15096 B (-0.038%)  | 39.807; -4000 B (-0.010%)   | 39.591; -217384 B (-0.546%) |
| Linux ARM64    | 33.485     | 33.484     | 33.482; -760 B (-0.002%)    | 33.469; -14744 B (-0.044%)  | 33.733; +249936 B (+0.747%) |
| macOS ARM64    | 29.258     | 29.258     | 29.526; +268032 B (+0.916%) | 29.542; +284568 B (+0.973%) | 29.543; +284800 B (+0.973%) |
| Windows x86-64 | 33.280     | 33.282     | 33.290; +9600 B (+0.029%)   | 33.517; +241409 B (+0.726%) | 33.531; +252672 B (+0.759%) |

### Processed wheel

| Platform       | No debug A | No debug B | Line tables                 | Limited                     | Full                        |
| -------------- | ---------- | ---------- | --------------------------- | --------------------------- | --------------------------- |
| Linux x86-64   | 17.495     | 17.496     | 17.493; -3232 B (-0.018%)   | 17.495; -2456 B (-0.014%)   | 17.394; -101755 B (-0.582%) |
| Linux ARM64    | 16.456     | 16.456     | 16.458; +3010 B (+0.018%)   | 16.457; +1420 B (+0.009%)   | 16.594; +137583 B (+0.836%) |
| macOS ARM64    | 14.941     | 14.942     | 15.078; +136632 B (+0.914%) | 15.082; +139925 B (+0.937%) | 15.079; +136827 B (+0.916%) |
| Windows x86-64 | 15.768     | 15.770     | 15.770; +3313 B (+0.021%)   | 15.887; +118518 B (+0.752%) | 15.890; +119477 B (+0.758%) |

### Other shipped executables

| Platform / executable   | No debug A | No debug B | Line tables             | Limited                 | Full                    |
| ----------------------- | ---------- | ---------- | ----------------------- | ----------------------- | ----------------------- |
| Linux x86-64: uvx       | 0.331      | 0.331      | 0.332; +200 B (+0.060%) | 0.332; +184 B (+0.056%) | 0.332; +184 B (+0.056%) |
| Linux ARM64: uvx        | 0.297      | 0.297      | 0.297; +168 B (+0.057%) | 0.297; +168 B (+0.057%) | 0.297; +168 B (+0.057%) |
| macOS ARM64: uvx        | 0.338      | 0.338      | 0.338; +176 B (+0.052%) | 0.338; +176 B (+0.052%) | 0.338; +176 B (+0.052%) |
| Windows x86-64: uvx.exe | 0.332      | 0.332      | 0.331; -512 B (-0.154%) | 0.331; -512 B (-0.154%) | 0.331; -512 B (-0.154%) |
| Windows x86-64: uvw.exe | 0.331      | 0.331      | 0.331; +0 B (+0.000%)   | 0.331; +0 B (+0.000%)   | 0.331; +0 B (+0.000%)   |

### Separate symbol files

Median uncompressed companion bytes, displayed in decimal MB. No-debug controls publish no
companion. Reduced Rust debug levels still include native C debug information; this is not a
C-debug-level comparison.

| Platform / executable   | No debug | Line tables | Limited | Full    |
| ----------------------- | -------- | ----------- | ------- | ------- |
| Linux x86-64: uv        | 0        | 169.468     | 266.362 | 582.194 |
| Linux x86-64: uvx       | 0        | 1.953       | 1.995   | 2.115   |
| Linux ARM64: uv         | 0        | 179.289     | 273.773 | 600.215 |
| Linux ARM64: uvx        | 0        | 2.127       | 2.168   | 2.283   |
| macOS ARM64: uv         | 0        | 188.788     | 300.764 | 580.906 |
| macOS ARM64: uvx        | 0        | 2.558       | 2.608   | 2.728   |
| Windows x86-64: uv.exe  | 0        | 119.042     | 120.193 | 442.765 |
| Windows x86-64: uvx.exe | 0        | 4.510       | 4.510   | 4.932   |
| Windows x86-64: uvw.exe | 0        | 4.510       | 4.510   | 4.911   |

## Resolver runtime

Each cell is the median of fourteen allocation-level medians, in milliseconds. All fifty samples per
mode/workload are retained in [measurements.json](measurements.json). Outputs and input hashes match
across all allocations within each platform.

### Jupyter

| Platform       | No debug A | No debug B | Line tables | Limited | Full   |
| -------------- | ---------- | ---------- | ----------- | ------- | ------ |
| Linux x86-64   | 8.764      | 8.791      | 8.653       | 8.649   | 8.676  |
| Linux ARM64    | 10.528     | 10.530     | 10.478      | 10.408  | 10.448 |
| macOS ARM64    | 11.070     | 11.059     | 11.088      | 11.056  | 11.038 |
| Windows x86-64 | 38.112     | 38.032     | 38.059      | 37.920  | 38.302 |

Paired change versus no debug, with simultaneous intervals:

| Platform       | Line tables           | Limited               | Full                  |
| -------------- | --------------------- | --------------------- | --------------------- |
| Linux x86-64   | -0.81% [-2.32, +0.25] | -0.79% [-2.40, -0.10] | -0.65% [-2.23, +0.80] |
| Linux ARM64    | -0.62% [-1.03, +0.14] | -0.72% [-1.27, +0.09] | -0.53% [-1.51, +0.33] |
| macOS ARM64    | +0.10% [-0.95, +1.16] | +0.02% [-0.84, +0.92] | -0.02% [-0.53, +0.86] |
| Windows x86-64 | -0.09% [-0.57, +0.48] | -0.23% [-0.69, +0.57] | +0.42% [-0.87, +1.58] |

Comparisons between symbol levels:

| Platform       | Limited / line tables | Full / line tables    | Full / limited        |
| -------------- | --------------------- | --------------------- | --------------------- |
| Linux x86-64   | -0.22% [-1.32, +1.08] | +0.12% [-1.28, +0.91] | +0.24% [-0.58, +0.90] |
| Linux ARM64    | -0.21% [-1.13, +0.88] | +0.14% [-1.32, +0.70] | +0.22% [-0.96, +0.83] |
| macOS ARM64    | +0.04% [-1.09, +0.81] | -0.11% [-0.69, +0.75] | +0.00% [-1.37, +0.45] |
| Windows x86-64 | -0.14% [-1.03, +0.64] | +0.44% [-0.58, +1.75] | +0.48% [-0.31, +1.93] |

### Trio

| Platform       | No debug A | No debug B | Line tables | Limited | Full   |
| -------------- | ---------- | ---------- | ----------- | ------- | ------ |
| Linux x86-64   | 7.133      | 7.128      | 7.029       | 7.003   | 7.011  |
| Linux ARM64    | 8.080      | 8.069      | 8.011       | 8.039   | 8.062  |
| macOS ARM64    | 9.282      | 9.247      | 9.288       | 9.256   | 9.256  |
| Windows x86-64 | 22.686     | 22.678     | 22.714      | 22.698  | 22.755 |

Paired change versus no debug, with simultaneous intervals:

| Platform       | Line tables           | Limited               | Full                  |
| -------------- | --------------------- | --------------------- | --------------------- |
| Linux x86-64   | -1.48% [-2.65, +0.06] | -1.35% [-2.30, +0.27] | -1.44% [-2.48, +1.17] |
| Linux ARM64    | -1.10% [-1.75, -0.63] | -0.88% [-1.66, +0.41] | -0.79% [-1.50, +0.17] |
| macOS ARM64    | +0.13% [-1.17, +0.97] | +0.06% [-0.78, +0.67] | -0.10% [-0.39, +0.69] |
| Windows x86-64 | -0.07% [-0.64, +0.96] | -0.33% [-0.79, +0.99] | +0.26% [-0.42, +0.74] |

Comparisons between symbol levels:

| Platform       | Limited / line tables | Full / line tables    | Full / limited        |
| -------------- | --------------------- | --------------------- | --------------------- |
| Linux x86-64   | +0.02% [-1.02, +1.51] | +0.18% [-1.07, +1.60] | +0.13% [-1.31, +1.58] |
| Linux ARM64    | +0.27% [-1.03, +1.09] | +0.25% [-0.47, +1.03] | +0.34% [-1.22, +1.77] |
| macOS ARM64    | -0.18% [-0.97, +1.69] | -0.22% [-1.25, +1.26] | +0.08% [-0.98, +0.66] |
| Windows x86-64 | -0.09% [-1.46, +0.90] | +0.39% [-0.81, +0.97] | +0.37% [-0.48, +1.25] |

## Baseline stability and allocation limits

A/A effects compare no-debug B with A on the same allocation. All intervals fit the corresponding
practical margin.

| Platform       | Build                 | Jupyter               | Trio                  |
| -------------- | --------------------- | --------------------- | --------------------- |
| Linux x86-64   | -0.10% [-1.60, +0.53] | +0.29% [-1.37, +1.09] | +0.16% [-1.19, +2.43] |
| Linux ARM64    | -0.05% [-8.70, +3.49] | +0.00% [-1.06, +0.94] | +0.12% [-1.38, +0.99] |
| macOS ARM64    | +0.25% [-2.03, +2.05] | +0.20% [-0.87, +0.88] | -0.26% [-0.95, +0.61] |
| Windows x86-64 | -0.01% [-2.50, +1.95] | -0.07% [-0.80, +1.04] | +0.03% [-1.37, +0.91] |

Allocations were spread across seven successive waves, two concurrent allocations per platform. All
56 runner identities are distinct. Rust/Maturin versions, source, Cargo settings, Python versions,
resolver inputs and resolver output hashes match within each platform. Distinct allocation names do
not prove distinct physical hosts or eliminate shared infrastructure/time dependence; the intervals
are conditional on the stated allocation assumptions.

The Linux x86-64 pool supplied thirteen AMD EPYC 9R45 allocations with about 66.15 GB RAM and one
Intel Xeon 6975P-C allocation with 132.92 GB RAM. Both use the same 16-CPU label. The Intel block's
no-debug reference was 16m47s, while thirteen AMD references ranged from 13m37s to 14m52s. It
remains included. Its paired overheads were +10.95% / +11.21% / +47.51% for line tables / limited /
full. Hardware differs alongside allocation conditions, so one Intel observation does not isolate a
CPU-vendor effect. The results describe the sampled runner pool.

ARM64 uses 16 Neoverse-V2 CPUs and about 66.19 GB RAM. macOS uses 12 virtual Apple M5 Max CPUs and
30.06 GB RAM. Windows uses 16 virtual AMD EPYC CPUs and 34,359,107,584 bytes RAM. The profiles are
respectively `depot-ubuntu-24.04-16`, `depot-ubuntu-24.04-arm-16`, `namespace-profile-macos-15`, and
`namespace-profile-windows-2022-x86-64-16x32`. Native Linux timings are not production manylinux CI
timings on its smaller runner.

Descriptive later-versus-earlier no-debug median changes were +0.05% (Linux x86-64), +0.38% (ARM64),
−0.06% (macOS) and +0.01% (Windows). There is no large consistent order penalty in these observed
controls. The widest control difference was ARM64 block 1001: A took 19m27s and B 17m45s.
Instrumented Cargo compilation was 12m33s versus 11m09s, and final uv compilation 6m23s versus
6m01s. Child CPU time also increased; minimum available RAM during training stayed above 42 GB. This
localizes that observation mainly to compilation, without establishing a CPU, storage or scheduling
root cause.

Calibration separately caught a roughly 51-second delay in the network-enabled Sentry installation
workload with nearly unchanged compilation and child CPU time; calibration is excluded from
inference. The older uninstrumented pairs cannot be assigned a definitive resource-pressure cause
retrospectively. Hardware variation, compilation variation and training waits must be distinguished
rather than explained by one universal baseline penalty.

## Resource and correctness evidence

Full-debug instrumented builds: maximum sampled aggregate descendant RSS and minimum sampled host
available memory across fourteen allocations. RSS can double-count shared pages and does not
establish an exact process peak or paging by itself.

| Platform       | Maximum sampled RSS (GB) | Minimum host available RAM (GB) | Failures |
| -------------- | ------------------------ | ------------------------------- | -------- |
| Linux x86-64   | 31.158                   | 28.135                          | 0 / 14   |
| Linux ARM64    | 37.905                   | 17.986                          | 0 / 14   |
| macOS ARM64    | 22.214                   | 9.551                           | 0 / 14   |
| Windows x86-64 | 31.316                   | 0.469                           | 0 / 14   |

Missing PGO warnings (minimum–maximum across allocations), shown per mode. Every mismatch count is
zero.

| Platform       | No debug A | No debug B | Line tables | Limited | Full |
| -------------- | ---------- | ---------- | ----------- | ------- | ---- |
| Linux x86-64   | 18         | 18         | 18          | 18      | 18   |
| Linux ARM64    | 18         | 18         | 18          | 18      | 18   |
| macOS ARM64    | 3565       | 3565       | 3567        | 3555    | 3585 |
| Windows x86-64 | 19         | 19         | 19          | 19      | 19   |

Linux missing-profile diagnostics are confined to uvx; Windows includes uvx/uvw. macOS warnings
include uv itself and numerous uv/dependency crates; profile coverage is incomplete in every mode.
Warning counts alone do not measure PGO training quality.

All native jobs passed Rust source lookup for every executable, AWS-LC RAND_bytes and jitterentropy
lookup in uv, negative lookup without companion information, SBOM retention, wheel installation and
smoke checks. Windows uses static CRT; downloaded PE import tables were independently checked. macOS
downloaded signatures were independently verified. Artifact/job/attempt/commit provenance, archive
digests, executable/profile/wheel hashes, effective final compiler flags and telemetry-series
consistency were verified. Instrumented compilation inherits the same Cargo environment by source
inspection; its log does not print every rustc invocation. No downloaded Linux or Windows executable
was run on the collecting Mac.

Process counters can miss short-lived descendants; I/O or page-fault counters can be unavailable,
and the collected errors remain in the data. POSIX reaped-child CPU counters supplement sampling.
Host counters include other activity; macOS page-in/out counters must not be treated as proof of
swap pressure. No memory or I/O causality is inferred from an aggregate counter alone.

## Reproduction and provenance

The published dataset contains every allocation, raw stage times, executable and companion sizes,
processed wheel sizes, resource summaries, PGO diagnostics, all resolver samples and artifact/job
links. Full logs, profiles, binaries, time series, original amendment and original analysis remain
retained under `target/debug-symbols-statistics/confirmatory-v1/`.
[analysis-inputs.json](analysis-inputs.json) is an exact projection of the raw reports' analysis
fields; [analysis.json](analysis.json) preserves the numerical first-look result and original report
hashes. Narrative amendment metadata is normalized for publication; its original file hash is
retained.

```shell
python3 scripts/debug-symbols/analyze_study.py \
  scripts/debug-symbols/results/2026-10-10/analysis-inputs.json \
  --amendment scripts/debug-symbols/results/2026-10-10/analysis-amendment.json \
  --sample-size 14 --output target/debug-symbols-statistics/reproduced-look-14.json
```

| Block | Linux x86-64                                                                                  | Linux ARM64                                                                                   | macOS ARM64                                                                                   | Windows x86-64                                                                                |
| ----- | --------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- |
| 1000  | [job 114092697230](https://github.com/astral-sh/uv/actions/runs/38011615355/job/114092697230) | [job 114092697228](https://github.com/astral-sh/uv/actions/runs/38011615355/job/114092697228) | [job 114092697289](https://github.com/astral-sh/uv/actions/runs/38011615355/job/114092697289) | [job 114092697262](https://github.com/astral-sh/uv/actions/runs/38011615355/job/114092697262) |
| 1001  | [job 114092724607](https://github.com/astral-sh/uv/actions/runs/38011623483/job/114092724607) | [job 114092724641](https://github.com/astral-sh/uv/actions/runs/38011623483/job/114092724641) | [job 114092724674](https://github.com/astral-sh/uv/actions/runs/38011623483/job/114092724674) | [job 114092724691](https://github.com/astral-sh/uv/actions/runs/38011623483/job/114092724691) |
| 1002  | [job 114119127997](https://github.com/astral-sh/uv/actions/runs/38020138214/job/114119127997) | [job 114119128000](https://github.com/astral-sh/uv/actions/runs/38020138214/job/114119128000) | [job 114119127998](https://github.com/astral-sh/uv/actions/runs/38020138214/job/114119127998) | [job 114119128050](https://github.com/astral-sh/uv/actions/runs/38020138214/job/114119128050) |
| 1003  | [job 114119585510](https://github.com/astral-sh/uv/actions/runs/38020285839/job/114119585510) | [job 114119585526](https://github.com/astral-sh/uv/actions/runs/38020285839/job/114119585526) | [job 114119585505](https://github.com/astral-sh/uv/actions/runs/38020285839/job/114119585505) | [job 114119585560](https://github.com/astral-sh/uv/actions/runs/38020285839/job/114119585560) |
| 1004  | [job 114142263267](https://github.com/astral-sh/uv/actions/runs/38027797422/job/114142263267) | [job 114142263231](https://github.com/astral-sh/uv/actions/runs/38027797422/job/114142263231) | [job 114142263235](https://github.com/astral-sh/uv/actions/runs/38027797422/job/114142263235) | [job 114142263244](https://github.com/astral-sh/uv/actions/runs/38027797422/job/114142263244) |
| 1005  | [job 114142277740](https://github.com/astral-sh/uv/actions/runs/38027802528/job/114142277740) | [job 114142277835](https://github.com/astral-sh/uv/actions/runs/38027802528/job/114142277835) | [job 114142277769](https://github.com/astral-sh/uv/actions/runs/38027802528/job/114142277769) | [job 114142277753](https://github.com/astral-sh/uv/actions/runs/38027802528/job/114142277753) |
| 1006  | [job 114164339203](https://github.com/astral-sh/uv/actions/runs/38035280730/job/114164339203) | [job 114164339208](https://github.com/astral-sh/uv/actions/runs/38035280730/job/114164339208) | [job 114164339250](https://github.com/astral-sh/uv/actions/runs/38035280730/job/114164339250) | [job 114164339213](https://github.com/astral-sh/uv/actions/runs/38035280730/job/114164339213) |
| 1007  | [job 114164349643](https://github.com/astral-sh/uv/actions/runs/38035285543/job/114164349643) | [job 114164349631](https://github.com/astral-sh/uv/actions/runs/38035285543/job/114164349631) | [job 114164349602](https://github.com/astral-sh/uv/actions/runs/38035285543/job/114164349602) | [job 114164349597](https://github.com/astral-sh/uv/actions/runs/38035285543/job/114164349597) |
| 1008  | [job 114186594786](https://github.com/astral-sh/uv/actions/runs/38042913234/job/114186594786) | [job 114186594813](https://github.com/astral-sh/uv/actions/runs/38042913234/job/114186594813) | [job 114186594766](https://github.com/astral-sh/uv/actions/runs/38042913234/job/114186594766) | [job 114186594789](https://github.com/astral-sh/uv/actions/runs/38042913234/job/114186594789) |
| 1009  | [job 114186613559](https://github.com/astral-sh/uv/actions/runs/38042918427/job/114186613559) | [job 114186613585](https://github.com/astral-sh/uv/actions/runs/38042918427/job/114186613585) | [job 114186613549](https://github.com/astral-sh/uv/actions/runs/38042918427/job/114186613549) | [job 114186613546](https://github.com/astral-sh/uv/actions/runs/38042918427/job/114186613546) |
| 1010  | [job 114208670627](https://github.com/astral-sh/uv/actions/runs/38050557294/job/114208670627) | [job 114208670604](https://github.com/astral-sh/uv/actions/runs/38050557294/job/114208670604) | [job 114208670631](https://github.com/astral-sh/uv/actions/runs/38050557294/job/114208670631) | [job 114208670641](https://github.com/astral-sh/uv/actions/runs/38050557294/job/114208670641) |
| 1011  | [job 114208679855](https://github.com/astral-sh/uv/actions/runs/38050562641/job/114208679855) | [job 114208679798](https://github.com/astral-sh/uv/actions/runs/38050562641/job/114208679798) | [job 114208679843](https://github.com/astral-sh/uv/actions/runs/38050562641/job/114208679843) | [job 114208679850](https://github.com/astral-sh/uv/actions/runs/38050562641/job/114208679850) |
| 1012  | [job 114232638169](https://github.com/astral-sh/uv/actions/runs/38058790789/job/114232638169) | [job 114232638524](https://github.com/astral-sh/uv/actions/runs/38058790789/job/114232638524) | [job 114232638112](https://github.com/astral-sh/uv/actions/runs/38058790789/job/114232638112) | [job 114232638168](https://github.com/astral-sh/uv/actions/runs/38058790789/job/114232638168) |
| 1013  | [job 114232653872](https://github.com/astral-sh/uv/actions/runs/38058797092/job/114232653872) | [job 114232653775](https://github.com/astral-sh/uv/actions/runs/38058797092/job/114232653775) | [job 114232653825](https://github.com/astral-sh/uv/actions/runs/38058797092/job/114232653825) | [job 114232653849](https://github.com/astral-sh/uv/actions/runs/38058797092/job/114232653849) |
