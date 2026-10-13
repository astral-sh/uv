# Repeated debug-level benchmarks with thin-LTO instrumentation

Four additional rounds plus the completed reference provide five fresh allocations per platform and
eighty builds overall. All sixteen new allocations (sixty-four builds) passed; including the
reference, twenty allocations and eighty builds completed with zero failures. All passed native and
retained-artifact verification; no allocation or outlier was removed.

Tables report descriptive medians and observed minimum–maximum ranges across five allocation-level
observations. Every overhead is calculated against the no-debug control from that same allocation
before aggregation. These are not confidence intervals, significance/equivalence claims or bounds on
future runs.

Pinned main: `59e5912ecff500422b99cca3df68d48be3841a67`. Repeats use experiment commit
`e6b065c1142546fccffb894d9321bc6718b44e77`; the
[reference round](../2026-10-13-all-debug-pilot/README.md) uses
`a77b32c17fa3173e4ea3a3d52bbe91f3dd421a98`. Source fingerprints, tool versions and per-platform
benchmark input/output/Python signatures match. Instrumentation is thin LTO; final compilation is
fat LTO, opt-level 3, one CGU and panic abort. See [methods](../../ALL_DEBUG_REPEATS.md) and
[plan](../../all-debug-repeat-plan.json).

## Paired build and training overhead

Percent, median [minimum, maximum]. Combined time is PGO-helper plus final-build/initial-wheel time;
setup, symbol postprocessing, verification and upload are excluded.

| Platform       | Line tables          | Limited              | Full                 |
| -------------- | -------------------- | -------------------- | -------------------- |
| Linux x86-64   | +12.9 [+12.0, +17.4] | +13.7 [+12.5, +14.4] | +52.5 [+50.2, +53.7] |
| Linux ARM64    | +10.5 [+10.2, +13.2] | +12.1 [+7.8, +14.1]  | +47.4 [+43.2, +50.7] |
| macOS ARM64    | +12.2 [+10.5, +12.8] | +13.5 [+12.5, +14.1] | +78.0 [+75.0, +79.0] |
| Windows x86-64 | +18.6 [+4.7, +30.5]  | +11.0 [+6.7, +18.1]  | +57.2 [+51.1, +60.8] |

Full debug has the largest observed build/training cost on every platform: median overhead ranges
from 47.4% on ARM64 Linux to 78.0% on macOS. Line-table medians range from 10.5% to 18.6%; limited
medians range from 11.0% to 13.7%. Windows is more variable: its line-table paired effects span
4.7–30.5%, with instrumentation taking 457.109–681.672s and final compilation 580.203–682.906s.
These observations do not establish why line tables and limited differ.

## Absolute build and training time

Descriptive total-time medians [minimum, maximum]. Ratios of these aggregate medians are not the
paired effects above.

| Platform       | No debug                   | Line tables                | Limited                    | Full                       |
| -------------- | -------------------------- | -------------------------- | -------------------------- | -------------------------- |
| Linux x86-64   | 8m 30s [8m 28s, 10m 44s]   | 9m 37s [9m 31s, 12m 05s]   | 9m 42s [9m 33s, 12m 13s]   | 12m 58s [12m 52s, 16m 07s] |
| Linux ARM64    | 10m 45s [10m 12s, 11m 36s] | 11m 53s [11m 16s, 12m 57s] | 12m 12s [11m 29s, 13m 01s] | 15m 30s [15m 02s, 17m 06s] |
| macOS ARM64    | 9m 15s [9m 12s, 9m 21s]    | 10m 23s [10m 18s, 10m 27s] | 10m 30s [10m 23s, 10m 36s] | 16m 24s [16m 21s, 16m 31s] |
| Windows x86-64 | 16m 36s [16m 23s, 17m 37s] | 19m 48s [18m 04s, 21m 39s] | 19m 01s [18m 11s, 19m 36s] | 26m 30s [25m 46s, 26m 39s] |

Every raw preparation, instrumentation, workload-training, merge, helper, final and total time is
retained in [raw-times.md](raw-times.md) and [measurements.json](measurements.json).

## Shipped binaries, wheels and companions

Main uv executable paired size changes, percent median [minimum, maximum]. Profiles and binaries are
independently produced; they need not be byte-identical.

| Platform       | Line tables             | Limited                 | Full                    |
| -------------- | ----------------------- | ----------------------- | ----------------------- |
| Linux x86-64   | +0.669 [+0.631, +0.692] | +0.025 [+0.009, +0.032] | +0.702 [+0.662, +0.747] |
| Linux ARM64    | +0.640 [+0.615, +0.654] | +0.679 [+0.673, +0.705] | +0.094 [+0.089, +0.107] |
| macOS ARM64    | +0.181 [+0.124, +0.181] | +0.916 [+0.859, +0.972] | +0.973 [+0.916, +0.973] |
| Windows x86-64 | -0.058 [-0.081, -0.008] | +0.040 [+0.020, +0.070] | +0.101 [+0.044, +0.124] |

Processed-wheel paired size changes, percent median [minimum, maximum]. Separate symbols are
excluded from wheels.

| Platform       | Line tables             | Limited                 | Full                    |
| -------------- | ----------------------- | ----------------------- | ----------------------- |
| Linux x86-64   | +0.743 [+0.696, +0.792] | +0.040 [+0.020, +0.075] | +0.710 [+0.680, +0.781] |
| Linux ARM64    | +0.743 [+0.732, +0.794] | +0.719 [+0.695, +0.744] | +0.117 [+0.085, +0.166] |
| macOS ARM64    | +0.102 [+0.075, +0.121] | +0.850 [+0.805, +0.901] | +0.937 [+0.871, +0.947] |
| Windows x86-64 | -0.067 [-0.084, -0.023] | +0.031 [-0.005, +0.044] | +0.018 [-0.029, +0.045] |

Companion sizes in decimal MB, median [minimum, maximum]. Exact bytes, shipped sizes and all
executable deltas are in the machine-readable summary.

| Platform / executable   | Line tables                | Limited                    | Full                       |
| ----------------------- | -------------------------- | -------------------------- | -------------------------- |
| Linux x86-64: uv        | 169.656 [169.627, 169.679] | 263.658 [263.628, 263.719] | 587.529 [587.440, 587.597] |
| Linux x86-64: uvx       | 1.953 [1.953, 1.953]       | 1.995 [1.995, 1.995]       | 2.115 [2.115, 2.115]       |
| Linux ARM64: uv         | 180.658 [180.612, 180.676] | 277.048 [277.008, 277.071] | 594.027 [593.957, 594.055] |
| Linux ARM64: uvx        | 2.127 [2.127, 2.127]       | 2.168 [2.168, 2.168]       | 2.283 [2.283, 2.283]       |
| macOS ARM64: uv         | 187.162 [187.153, 187.171] | 300.777 [300.746, 300.788] | 581.313 [581.238, 581.582] |
| macOS ARM64: uvx        | 2.558 [2.558, 2.558]       | 2.608 [2.608, 2.608]       | 2.728 [2.728, 2.728]       |
| Windows x86-64: uv.exe  | 120.099 [120.066, 120.123] | 120.254 [120.214, 120.328] | 443.585 [443.568, 443.748] |
| Windows x86-64: uvx.exe | 4.502 [4.502, 4.510]       | 4.510 [4.502, 4.510]       | 4.936 [4.927, 4.936]       |
| Windows x86-64: uvw.exe | 4.510 [4.502, 4.510]       | 4.510 [4.502, 4.510]       | 4.911 [4.911, 4.911]       |

## Resolver runtime

Each cell summarizes five paired ratios of allocation-level medians, not a pool of individual
resolutions. Each allocation has four warmups and forty timed samples per treatment/workload in
balanced randomized order. All outputs matched; all samples, including noisy runs, remain in the
measurements.

Jupyter change, percent median [minimum, maximum]:

| Platform       | Line tables          | Limited              | Full                  |
| -------------- | -------------------- | -------------------- | --------------------- |
| Linux x86-64   | -0.56 [-1.80, +0.52] | -1.03 [-1.81, +0.00] | -0.67 [-1.44, +0.05]  |
| Linux ARM64    | -0.50 [-0.83, -0.15] | -0.53 [-1.01, -0.10] | -0.41 [-1.18, -0.13]  |
| macOS ARM64    | +0.78 [+0.27, +1.66] | +0.41 [-0.23, +1.94] | +0.40 [-0.28, +0.64]  |
| Windows x86-64 | +0.06 [-8.60, +1.54] | +0.10 [-6.06, +1.26] | +0.08 [-12.21, +0.89] |

Absolute allocation-level medians in milliseconds, median [minimum, maximum]:

| Platform       | No debug                | Line tables             | Limited                 | Full                    |
| -------------- | ----------------------- | ----------------------- | ----------------------- | ----------------------- |
| Linux x86-64   | 8.763 [8.700, 13.718]   | 8.793 [8.543, 13.616]   | 8.673 [8.542, 13.719]   | 8.726 [8.574, 13.572]   |
| Linux ARM64    | 10.388 [10.173, 11.088] | 10.337 [10.130, 11.071] | 10.323 [10.119, 11.077] | 10.371 [10.132, 11.074] |
| macOS ARM64    | 11.060 [11.034, 11.184] | 11.130 [11.076, 11.313] | 11.125 [11.037, 11.345] | 11.131 [11.088, 11.167] |
| Windows x86-64 | 28.759 [27.496, 57.573] | 28.808 [27.286, 52.623] | 29.120 [27.621, 54.085] | 28.502 [27.742, 50.543] |

Trio change, percent median [minimum, maximum]:

| Platform       | Line tables          | Limited              | Full                 |
| -------------- | -------------------- | -------------------- | -------------------- |
| Linux x86-64   | -1.64 [-4.19, -0.78] | -0.66 [-2.46, -0.61] | -1.62 [-3.53, +0.36] |
| Linux ARM64    | -0.56 [-0.62, -0.12] | -0.50 [-0.76, +0.06] | -1.03 [-1.35, +0.15] |
| macOS ARM64    | +0.19 [-0.46, +0.72] | +0.62 [-0.02, +0.82] | +0.76 [-0.97, +0.94] |
| Windows x86-64 | -1.18 [-4.59, -0.15] | -0.08 [-4.67, +3.12] | -1.69 [-2.41, +1.85] |

Absolute allocation-level medians in milliseconds, median [minimum, maximum]:

| Platform       | No debug                | Line tables             | Limited                 | Full                    |
| -------------- | ----------------------- | ----------------------- | ----------------------- | ----------------------- |
| Linux x86-64   | 7.298 [6.965, 11.089]   | 6.993 [6.910, 10.907]   | 7.119 [6.922, 11.018]   | 7.040 [6.867, 11.129]   |
| Linux ARM64    | 8.019 [7.788, 8.468]    | 7.971 [7.745, 8.415]    | 7.957 [7.747, 8.425]    | 7.910 [7.704, 8.481]    |
| macOS ARM64    | 9.270 [9.245, 9.402]    | 9.294 [9.211, 9.420]    | 9.345 [9.251, 9.416]    | 9.331 [9.163, 9.367]    |
| Windows x86-64 | 21.326 [18.152, 38.087] | 21.075 [18.038, 36.340] | 20.987 [18.111, 39.277] | 20.965 [18.043, 37.171] |

Across both workloads and all platforms, medians of the five paired runtime effects range from
−1.69% to +0.78%. Individual allocation effects have wider ranges, as the tables show; matching
outputs and these small descriptive medians do not establish performance equivalence.

The reference Windows runtime observations include a −12.21% full-debug Jupyter median and +3.12%
limited-debug Trio median, with wide individual sample ranges. They remain in this cohort. Median
differences do not establish repeatable speedups or regressions; runtime resources were not sampled.

## Memory and hardware

Sampled process-tree RSS peaks in decimal GB, median [minimum, maximum]. RSS sums can double-count
shared pages and miss peaks between samples. They are not exact physical-memory peaks.

PGO phase (instrumentation plus workload training):

| Platform       | No debug                | Line tables             | Limited                 | Full                    |
| -------------- | ----------------------- | ----------------------- | ----------------------- | ----------------------- |
| Linux x86-64   | 10.740 [9.964, 12.834]  | 16.302 [14.338, 16.579] | 16.499 [14.566, 16.742] | 24.243 [24.132, 24.264] |
| Linux ARM64    | 11.116 [10.938, 11.132] | 16.596 [13.541, 16.677] | 16.669 [16.628, 16.950] | 24.785 [23.190, 24.850] |
| macOS ARM64    | 6.489 [6.389, 6.522]    | 8.697 [8.439, 8.708]    | 8.683 [8.414, 8.707]    | 12.200 [11.582, 12.469] |
| Windows x86-64 | 5.449 [5.239, 5.609]    | 7.271 [7.219, 7.391]    | 7.395 [7.193, 7.534]    | 11.039 [11.019, 11.059] |

Final fat-LTO compilation:

| Platform       | No debug                | Line tables             | Limited                 | Full                    |
| -------------- | ----------------------- | ----------------------- | ----------------------- | ----------------------- |
| Linux x86-64   | 12.092 [11.753, 12.179] | 14.079 [13.215, 16.028] | 13.438 [12.614, 15.427] | 23.548 [23.504, 23.729] |
| Linux ARM64    | 11.105 [11.028, 11.161] | 14.686 [13.694, 15.659] | 14.555 [14.283, 15.656] | 22.908 [22.352, 23.880] |
| macOS ARM64    | 8.083 [8.043, 8.096]    | 10.172 [10.163, 10.219] | 10.253 [10.190, 10.293] | 19.572 [19.493, 20.219] |
| Windows x86-64 | 5.644 [5.559, 5.670]    | 12.595 [12.409, 12.792] | 12.579 [12.276, 13.157] | 25.077 [24.724, 25.341] |

| Platform       | Round | Reported CPU             | Logical CPUs | Physical CPUs | Reported RAM (bytes) | Build order                             |
| -------------- | ----- | ------------------------ | ------------ | ------------- | -------------------- | --------------------------------------- |
| Linux x86-64   | 0     | AMD EPYC 9R45            | 16           | 16            | 66149724160          | Full → Line tables → No debug → Limited |
| Linux x86-64   | 1     | AMD EPYC 9R45            | 16           | 16            | 66149720064          | No debug → Full → Line tables → Limited |
| Linux x86-64   | 2     | AMD EPYC 9R45            | 16           | 16            | 66149724160          | Full → Line tables → Limited → No debug |
| Linux x86-64   | 3     | Intel(R) Xeon(R) 6975P-C | 16           | 8             | 132916699136         | Limited → No debug → Full → Line tables |
| Linux x86-64   | 4     | Intel(R) Xeon(R) 6975P-C | 16           | 8             | 132916703232         | Line tables → Limited → No debug → Full |
| Linux ARM64    | 0     | Neoverse-V2              | 16           | 16            | 66192830464          | Line tables → No debug → Full → Limited |
| Linux ARM64    | 1     | Neoverse-V2              | 16           | 16            | 66192826368          | No debug → Line tables → Full → Limited |
| Linux ARM64    | 2     | Neoverse-V2              | 16           | 16            | 66192826368          | Full → Limited → No debug → Line tables |
| Linux ARM64    | 3     | Neoverse-V2              | 16           | 16            | 66192826368          | Line tables → Full → Limited → No debug |
| Linux ARM64    | 4     | Neoverse-V2              | 16           | 16            | 66192826368          | Limited → No debug → Line tables → Full |
| macOS ARM64    | 0     | Apple M5 Max (Virtual)   | 12           | 12            | 30064771072          | Line tables → Limited → No debug → Full |
| macOS ARM64    | 1     | Apple M5 Max (Virtual)   | 12           | 12            | 30064771072          | Limited → Full → Line tables → No debug |
| macOS ARM64    | 2     | Apple M5 Max (Virtual)   | 12           | 12            | 30064771072          | Full → Line tables → No debug → Limited |
| macOS ARM64    | 3     | Apple M5 Max (Virtual)   | 12           | 12            | 30064771072          | No debug → Limited → Full → Line tables |
| macOS ARM64    | 4     | Apple M5 Max (Virtual)   | 12           | 12            | 30064771072          | Line tables → No debug → Limited → Full |
| Windows x86-64 | 0     | AMD EPYC-v4 Processor    | 16           | 16            | 34359107584          | Limited → Full → Line tables → No debug |
| Windows x86-64 | 1     | AMD EPYC-v4 Processor    | 16           | 16            | 34359107584          | No debug → Full → Line tables → Limited |
| Windows x86-64 | 2     | AMD EPYC-v4 Processor    | 16           | 16            | 34359107584          | Line tables → Limited → No debug → Full |
| Windows x86-64 | 3     | AMD EPYC-v4 Processor    | 16           | 16            | 34359107584          | Limited → No debug → Full → Line tables |
| Windows x86-64 | 4     | AMD EPYC-v4 Processor    | 16           | 16            | 34359107584          | Full → Line tables → Limited → No debug |

The Linux x86-64 cohort contains three AMD EPYC 9R45 allocations (16 physical CPUs, about 66.15 GB
RAM) and two Intel Xeon 6975P-C allocations (8 physical / 16 logical CPUs, about 132.92 GB RAM), all
under the same requested Depot profile. Hardware and host variation therefore remain part of the
descriptive cohort; no hardware subset is excluded.

Requested runner profiles and Cargo jobs are fixed; reported hardware is retained rather than
inferred from the label. Fresh runner names do not prove independent physical hosts. Provider
hardware mix, host conditions, cache/order effects and independently trained profiles can affect
observations. The four repeat rounds balance every treatment across all four build positions; the
reference retains its original order.

Maximum observed resource-sampling gap: 1.759s. Unavailable macOS process-I/O counters and
short-lived-process errors remain in the data. These measurements alone do not identify CPU
throttling, paging or I/O-pressure causes. The reference Windows line-table instrumented build took
681.672s versus 533.578s limited; that slower observation is retained.

## PGO and correctness

Missing-profile warning counts, median [minimum, maximum]:

| Platform       | No debug          | Line tables       | Limited           | Full              |
| -------------- | ----------------- | ----------------- | ----------------- | ----------------- |
| Linux x86-64   | 18 [18, 18]       | 18 [18, 18]       | 18 [18, 18]       | 18 [18, 18]       |
| Linux ARM64    | 18 [18, 18]       | 18 [18, 18]       | 18 [18, 18]       | 18 [18, 18]       |
| macOS ARM64    | 3787 [3787, 3787] | 3823 [3823, 3823] | 3819 [3819, 3819] | 3845 [3845, 3845] |
| Windows x86-64 | 19 [19, 19]       | 19 [19, 19]       | 19 [19, 19]       | 19 [19, 19]       |

Total mismatched-profile diagnostics across all builds: 0. Missing warnings, especially on macOS,
remain a coverage caveat; fewer warnings alone do not establish better training. Native C debug
settings can remain full under reduced Rust levels.

All allocations passed installation/smoke checks, source and negative lookups, SBOM retention and
applicable Windows static-CRT/macOS-signature checks. Retained artifacts passed
commit/plan/block/order/job/attempt/upload/digest validation, both phases' effective flag checks,
profile/binary/wheel hashes, companion sizes, stage/resource-series checks and benchmark
sample/order/output validation. Windows PE imports were independently inspected without executing
Windows binaries; downloaded Linux binaries were not executed locally either.

The four new Windows allocations also passed independent decoding and report-digest checks for all
48 embedded PE SBOMs, hashing and size checks for all 36 PDBs, and agreement between capacity-step
output and telemetry (16 logical CPUs and 34,359,107,584 RAM bytes). The
[audit records](windows-audit.json) retain the hashes and job provenance.

## Scope and evidence

This cohort includes exactly the completed four-level reference plus the four repeats. Earlier
lower-debug-only, Windows LTO-factorial and October 7/9/10 studies remain separate. No production
release workflow/profile was changed. These native build timings are not production manylinux CI or
the complete release pipeline.

[All normalized measurements and timed samples](measurements.json),
[derived summaries with every per-round value](summary.json), [raw build stages](raw-times.md) and
[provenance manifest](manifest.json) accompany this report. Large artifacts, authoritative top-level
reports, job logs, profiles, symbols and resource traces remain locally under
`target/debug-symbols-experiment/all-debug-repeat-2026-10-13/` and the reference's
`all-debug-pilot-2026-10-13/` directory. Per-variant reports are earlier snapshots; the top-level
report contains final verification.

| Platform       | Round | Job                                                                              | Artifact    | Uploaded UTC         | Archive SHA256                                                     |
| -------------- | ----- | -------------------------------------------------------------------------------- | ----------- | -------------------- | ------------------------------------------------------------------ |
| Linux x86-64   | 0     | [Job](https://github.com/astral-sh/uv/actions/runs/38437984289/job/115436950535) | 11836463891 | 2026-10-13T18:04:48Z | `f46b314eb2dcb6ce2a5ae975f92aa113393d024a5b355e2e09f761674456d6b3` |
| Linux x86-64   | 1     | [Job](https://github.com/astral-sh/uv/actions/runs/38459563230/job/115510947939) | 11848267386 | 2026-10-13T20:45:04Z | `61c1aa9af9ca3b1de52f8160e1406c716952200e3051bf0db1fb18ed3608e0ce` |
| Linux x86-64   | 2     | [Job](https://github.com/astral-sh/uv/actions/runs/38459567017/job/115510958343) | 11847599086 | 2026-10-13T20:45:09Z | `3a5709b9a38ebbb44fb9cf2876d9914d8a030f587911544e0b3727172dcca28d` |
| Linux x86-64   | 3     | [Job](https://github.com/astral-sh/uv/actions/runs/38459570394/job/115510978236) | 11849025864 | 2026-10-13T20:56:07Z | `9d2110834c451e38a56b9cadd3a1f43a4ae42be2eaf8c6dca7902093dc7c34a4` |
| Linux x86-64   | 4     | [Job](https://github.com/astral-sh/uv/actions/runs/38459574190/job/115510987083) | 11849255521 | 2026-10-13T20:54:49Z | `89a7cbf5217951c6bf7c421dc063218e815494a0c4dfee59028a691fdc7e605d` |
| Linux ARM64    | 0     | [Job](https://github.com/astral-sh/uv/actions/runs/38437984289/job/115436950528) | 11837663272 | 2026-10-13T18:14:47Z | `2ac1dd86c4708003f04439271517c69ebba557ccdabf2fa11427b8042edd31ba` |
| Linux ARM64    | 1     | [Job](https://github.com/astral-sh/uv/actions/runs/38459563230/job/115510948200) | 11848868164 | 2026-10-13T20:53:28Z | `4b2fabc0f2a42bc3e29b1f69d92054dfe968fe6cb8d786a3163426e985565a85` |
| Linux ARM64    | 2     | [Job](https://github.com/astral-sh/uv/actions/runs/38459567017/job/115510957909) | 11848858005 | 2026-10-13T20:53:09Z | `f9440c53c6230ab8e3fd318f05a08fa53ab23cebcbd8b0da88d916a2118fcc35` |
| Linux ARM64    | 3     | [Job](https://github.com/astral-sh/uv/actions/runs/38459570394/job/115510978191) | 11848564976 | 2026-10-13T20:59:54Z | `e7e82d5c4318687ca4792221ceb77da44118b7aa30222800816e91749ac21fff` |
| Linux ARM64    | 4     | [Job](https://github.com/astral-sh/uv/actions/runs/38459574190/job/115510987285) | 11848514036 | 2026-10-13T20:58:28Z | `2c4e1766b0765dc1b4cefe08ab3e9e3682c7e0bf4ee0f9be51991a368e3e8d4c` |
| macOS ARM64    | 0     | [Job](https://github.com/astral-sh/uv/actions/runs/38437984289/job/115436950470) | 11837727586 | 2026-10-13T18:10:16Z | `b33c1ff825af17591519142db6083ae86286c7160b19c86556bb192e15b9ea31` |
| macOS ARM64    | 1     | [Job](https://github.com/astral-sh/uv/actions/runs/38459563230/job/115510947918) | 11847918748 | 2026-10-13T20:51:16Z | `7be3a1556d97773243432b83182bb3b88217be923c63eda05ff82e61f0232169` |
| macOS ARM64    | 2     | [Job](https://github.com/astral-sh/uv/actions/runs/38459567017/job/115510957931) | 11847823055 | 2026-10-13T20:51:06Z | `f45b0a20a73be5fc261e930534da41a99573cfc861e6a7ce4b6cbfa0854b2801` |
| macOS ARM64    | 3     | [Job](https://github.com/astral-sh/uv/actions/runs/38459570394/job/115510977963) | 11848132995 | 2026-10-13T20:51:16Z | `f5ab51ee84701952391c354161b279281331300bb1bd443cfe4a2f9fb363bf0e` |
| macOS ARM64    | 4     | [Job](https://github.com/astral-sh/uv/actions/runs/38459574190/job/115510987177) | 11848328532 | 2026-10-13T20:51:05Z | `9c983d3d54080afe0e9b11124e8e16a075067bc6b686117ccc5761c10b95f2eb` |
| Windows x86-64 | 0     | [Job](https://github.com/astral-sh/uv/actions/runs/38437984289/job/115436950574) | 11839503781 | 2026-10-13T18:50:15Z | `e9cd1b02bf86d78ed8c907b332fc75fb011bf51a46c1096b5cd81a6a1ac03b4e` |
| Windows x86-64 | 1     | [Job](https://github.com/astral-sh/uv/actions/runs/38459563230/job/115510947920) | 11851028249 | 2026-10-13T21:24:44Z | `a81e9ae3a63dbad82a959dff9091af9aa97b5efe9c801f063e5b84203cd8342f` |
| Windows x86-64 | 2     | [Job](https://github.com/astral-sh/uv/actions/runs/38459567017/job/115510958175) | 11851956790 | 2026-10-13T21:27:52Z | `75fd9922f2e45df6e2c37e9e2821ac0dc04fd487ea51eb9adb998555400a15c7` |
| Windows x86-64 | 3     | [Job](https://github.com/astral-sh/uv/actions/runs/38459570394/job/115510978147) | 11851462922 | 2026-10-13T21:26:15Z | `a467fd9e8e640a11659f7013575a25225cfa749a32979277b1f4209495c49b86` |
| Windows x86-64 | 4     | [Job](https://github.com/astral-sh/uv/actions/runs/38459574190/job/115510987146) | 11851233723 | 2026-10-13T21:30:11Z | `42cb27681ec567dcae4187758a2e7b135e7bafee8ccc3662a0e32cc44c9d6038` |
