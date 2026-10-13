# Windows instrumentation LTO pilot: October 13, 2026

Thin LTO reduced instrumented compilation time by 52.4% without debug information and 53.7% with
full debug information. Combined PGO/final-build time fell 29.5% and 35.0%, respectively. All four
variants succeeded on one Namespace 16-CPU/32-GB allocation. These are descriptive results from one
allocation, not statistical significance or equivalence claims.

Experiment commit: `e93dc7fae7fb2d5d8e896775085b00285e8e16c2`; main source:
`59e5912ecff500422b99cca3df68d48be3841a67`. The
[job](https://github.com/astral-sh/uv/actions/runs/38403298996/job/115316275935) ran for 1h47m40s
including setup, all four variants, verification and upload. The predeclared order was thin/no
debug, fat/full, fat/no debug, thin/full.

Every variant has a fresh target directory and separately trained PGO profile. All final
compilations use fat LTO, optimization level 3, one codegen unit, panic abort and static CRT, with
four Cargo jobs. Rust is 1.99.0 / LLVM 23.1.1; Maturin is 1.15.0. Native C debug settings follow the
no/full debug treatment. The production PGO helper remains unchanged on disk; the pilot driver
selects only instrumentation LTO. Effective flags for uv, uvx and uvw were checked in both
compilation stages.

## Build and training time

Raw wall seconds. The PGO helper total includes preparation, instrumented compilation, workload
training, profile merge and helper overhead. Final build includes compilation and initial wheel
construction. Combined total is helper total plus final build; symbol postprocessing, verification
and upload are outside this timer.

| Stage                    | Fat / no debug | Thin / no debug | Fat / full debug | Thin / full debug |
| ------------------------ | -------------- | --------------- | ---------------- | ----------------- |
| Corpus preparation       | 0.016          | 0.016           | 0.015            | 0.000             |
| Instrumented compilation | 839.359        | 399.359         | 1396.891         | 646.375           |
| Training workloads       | 36.562         | 38.547          | 50.719           | 32.657            |
| Profile merge            | 1.235          | 1.094           | 1.234            | 0.875             |
| PGO helper total         | 877.344        | 439.235         | 1449.016         | 680.046           |
| Final fat-LTO build      | 510.141        | 538.985         | 907.782          | 852.453           |
| Combined total           | 1387.485       | 978.220         | 2356.798         | 1532.499          |

| Debug level | Fat instrumentation total | Thin instrumentation total | Change |
| ----------- | ------------------------- | -------------------------- | ------ |
| None        | 23m 07s                   | 16m 18s                    | -29.5% |
| Full        | 39m 17s                   | 25m 32s                    | -35.0% |

Full debug versus no debug costs +69.9% under fat instrumentation and +56.7% under thin
instrumentation in this allocation. These are not replacements for the older multi-allocation
results.

Final compilation retained fat LTO but varied +5.7% in the no-debug pair and −6.1% in the full-debug
pair. Profiles and binaries were independently produced, and sequential builds share machine/cache
conditions. Do not attribute every final-build or runtime difference directly to the instrumentation
flag.

## Memory

Decimal GB. Peak RSS is the sampled sum over descendant processes, so it can double-count shared
pages and is not an exact physical-memory peak. “PGO phase” includes both instrumented compilation
and workload training.

| Variant           | PGO peak RSS | PGO minimum available RAM | Final peak RSS | Final minimum available RAM |
| ----------------- | ------------ | ------------------------- | -------------- | --------------------------- |
| Fat / no debug    | 6.854        | 24.116                    | 5.545          | 26.472                      |
| Thin / no debug   | 5.641        | 26.150                    | 5.668          | 25.361                      |
| Fat / full debug  | 29.120       | 2.498                     | 25.143         | 6.430                       |
| Thin / full debug | 10.509       | 21.487                    | 25.321         | 6.169                       |

With full debug, PGO-phase sampled RSS fell from 29.120 to 10.509 GB. The final fat-LTO build still
reached 25.321 GB with the thin-trained profile; 10.509 GB is not the peak of the entire pipeline.
No compiler OOM occurred. The machine reported 16 logical CPUs, 34,359,107,584 bytes physical RAM
and a virtual AMD EPYC-v4 CPU.

Resource traces had maximum sample gaps of 1.265 seconds. Recorded short-lived-process/I/O sampling
errors remain in the data. These counters do not establish paging, CPU throttling or an I/O-pressure
cause.

## Executable, wheel and symbol sizes

Raw bytes. Separate PDBs are excluded from processed wheels.

| Artifact        | Fat / no debug | Thin / no debug | Fat / full debug | Thin / full debug |
| --------------- | -------------- | --------------- | ---------------- | ----------------- |
| uv.exe          | 33548800       | 33563648        | 33585664         | 33586688          |
| uvx.exe         | 331776         | 331776          | 331264           | 331264            |
| uvw.exe         | 331264         | 331264          | 331264           | 331264            |
| Processed wheel | 15919122       | 15927067        | 15921367         | 15924688          |
| uv.pdb          | —              | —               | 443674624        | 443650048         |
| uvx.pdb         | —              | —               | 4927488          | 4927488           |
| uvw.pdb         | —              | —               | 4911104          | 4911104           |

Thin versus fat instrumentation changes uv.exe by +14,848 bytes (+0.0443%) without debug information
and +1,024 bytes (+0.0030%) with full debug information. Full uv PDBs are 443.675 MB and 443.650 MB
(decimal), a 24,576-byte decrease with thin instrumentation.

Full debug versus no debug changes uv.exe by +36,864 bytes (+0.110%) under fat instrumentation and
+23,040 bytes (+0.069%) under thin instrumentation. Wheel changes are +0.014% and −0.015%,
respectively. Executables and profiles are not byte-identical.

## Resolver runtime

Median milliseconds from forty timed resolutions per variant after four warmups, in balanced
randomized order. All outputs matched. These repetitions characterize the four binaries on this
host; there is only one machine/profile observation per treatment.

| Workload | Fat / no debug | Thin / no debug | Fat / full debug | Thin / full debug |
| -------- | -------------- | --------------- | ---------------- | ----------------- |
| Jupyter  | 34.863         | 34.830          | 34.516           | 34.842            |
| Trio     | 21.591         | 21.241          | 21.468           | 21.536            |

| Comparison                            | Jupyter change | Trio change |
| ------------------------------------- | -------------- | ----------- |
| Thin / fat, none debug                | -0.09%         | -1.62%      |
| Thin / fat, full debug                | +0.95%         | +0.32%      |
| Full / no debug, fat instrumentation  | -1.00%         | -0.57%      |
| Full / no debug, thin instrumentation | +0.04%         | +1.39%      |

Observed thin/fat median changes range from −1.62% to +0.95%. This pilot does not establish runtime
equivalence or a universal regression bound.

## PGO and correctness

Every final build reports 19 missing-profile warnings, confined to uvx/uvw, and zero profile
mismatches. Merged profiles have 91,097 functions without debug information and 91,110 with full
debug; each LTO pair has the same function count. All four profile hashes differ, and each variant
has complete independent compilation/training stage evidence. Matching counts do not prove identical
coverage.

All binaries passed native installation and smoke checks. Full-debug binaries passed Rust source
lookup for all three executables, AWS-LC RAND_bytes and jitterentropy lookup for uv, and negative
lookups without the PDB. SBOM contents match across variants. Artifact digest,
source/plan/order/attempt provenance, all executable/profile/wheel hashes, PDB sizes, compiler
commands, resource time-series summaries, and benchmark samples/output hashes were independently
verified. Downloaded PE import tables also confirm static CRT; no Windows binaries were executed on
the collecting Mac.

## Planning implication and limits

This supports retaining roughly 12–16 hours as a planning budget for the proposed first
multi-platform statistical round, rather than treating the older fat-instrumentation timings as
current release timings. It does not justify applying a 30–35% reduction universally.

For a transparent Windows-only estimate: two no-debug builds plus one full-debug build account for
about 58 minutes at the observed thin timings. If each unmeasured reduced-debug mode falls between
the observed no-debug and full-debug times, the five-treatment block would take about 91–109 minutes
before setup/verification/upload. Seven two-allocation waves would then take 10.6–12.7 hours plus
overhead. This assumption is not a measured bound: line tables, limited debug, other platforms,
queueing and allocation variability remain unmeasured here. A later study would need calibration
before treating the complete-matrix estimate as firm.

No further statistical run or other platform pilot was dispatched. The October 7/9/10 observations
remain unchanged and are not pooled into this single-allocation comparison.

## Retained evidence

[Measurements and all runtime samples](measurements.json), [derived comparisons](comparison.json),
and [pilot plan](../../lto-pilot-plan.json) accompany this report. Full reports, archive, logs,
profiles, executables, PDBs and resource series are retained locally beneath
`target/debug-symbols-experiment/lto-pilot-2026-10-13/`. The top-level report.json contains final
verification; per-variant reports are earlier build snapshots.

Artifact `11825451085` was uploaded by job `115316275935`, attempt 1, at `2026-10-13T15:02:05Z`.
Archive SHA256: `86880d316f4c883c055f1418be12789f286cd64787fdecb0b736ec128101fc46`. Canonical plan
SHA256: `ff6a59f00b4e8bbba2dfa20f40c65302194a12751bc3760e9be3eea52dcf9f34`.
