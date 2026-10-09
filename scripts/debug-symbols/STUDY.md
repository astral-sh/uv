# Registered debug-level comparison

Study identifier: `uv-debug-statistics-20261009-v1`.

The source is frozen at main revision `238d6ba651d13f0dfddab0cc1826f963cdf21711`. `study-plan.json`
records hashes for the source trees, manifests, compiler version file, and production PGO/wrapper
scripts. Study jobs reject changes to these inputs. Experiment code commits are recorded separately;
all confirmatory blocks must use one experiment commit.

## Scope and experimental unit

Compare no debug, line tables, limited, and full debug on Linux x86-64, Linux ARM64, macOS ARM64,
and Windows x86-64. Use one codegen unit, release optimization level 3, fat LTO, the release PGO
corpus, and the same native runner profiles and Cargo job counts as the preceding experiment.
Windows remains on Namespace. This estimates native-runner effects, not production manylinux wall
times on smaller Linux runners.

One independent observation is a complete runner allocation. Each allocation builds five treatments:
two independently built no-debug controls (`none-a`, `none-b`) and one of each debug level. Every
build has an empty target directory and independently trained PGO profile. Crate fetching is setup
outside the treatment timers. Symbol processing, verification, and uploads also remain outside those
timers.

Treatment order is independently randomized for each runner, using the published seed and block ID.
Every level therefore has an opportunity to occupy every position; finite samples are not forced
into a Latin-square allocation. The order is recorded before compilation. This removes the fixed
baseline-first design, while preserving independent randomization across runner blocks. Order counts
and sensitivity to position/carryover must be reported. No order is selected based on its measured
outcome.

The two no-debug builds provide an A/A control. The no-debug reference for a treatment contrast is
their geometric mean, within that runner only. Different runners' baselines are never substituted or
pooled before computing contrasts. A runner may contribute only once, even after a rerun.
Physical-host independence is not observable from a runner label alone; report allocation identities
and spread collection across waves.

The existing two rounds used another design and are exploratory evidence only. They are not included
in confirmatory sample counts. Calibration blocks 0 and 1 on every platform exercise the new
harness, telemetry and A/A control; they are also excluded. Confirmatory block IDs begin at 1000.
Correct setup/telemetry problems before freezing the confirmatory experiment commit.

## Outcomes and inference

The build outcome is instrumented compilation plus PGO training plus final compilation and wheel
construction. Training/final stage times are also reported separately.

Two resolver outcomes are the within-runner medians for Jupyter and Trio. Each binary gets five
warmups and fifty timed resolutions. Order is randomized and balanced within each five-iteration
batch. All resolved outputs must match. The fifty timings estimate a runner's median; they are not
fifty independent build or machine observations.

For each platform and outcome, compare all six pairs of the four levels, and the A/A control. This
is a fixed family of 84 contrasts: 4 platforms × 3 timing outcomes × 7 contrasts. Each value is a
within-runner log ratio. The estimand is the population median of these log ratios, transformed back
to a multiplicative ratio; it is not a ratio of pooled mean times.

Inference uses an exact binomial/order-statistic interval for the median. It assumes independent,
identically distributed randomized runner blocks; ties are conservative. There is no normality
assumption. Host/time dependence or protocol differences must be examined before treating the
intervals as population evidence.

Inferential looks occur only after 20, 40, 80, 160, ... complete blocks per platform. At look index
`k` starting at zero, spend `0.05 / 2^(k+1)` family error, and divide that allowance by 84 for each
two-sided interval. A union bound over all contrasts and all looks limits the probability of any
interval miss to at most 5%, under the stated sampling assumptions. Intermediate progress checks do
not make significance claims. Blocks must be the prespecified consecutive IDs; late, inconvenient,
or noisy observations cannot be replaced with nicer ones.

The registered practical-equivalence margins are ±5% for build time and ±3% for the two resolver
medians. A treatment contrast is resolved when its adjusted interval excludes a ratio of 1, or lies
entirely within its equivalence margin. A/A controls must establish equivalence, rather than merely
fail to detect a difference. Stop only when all 72 treatment contrasts and all 12 control contrasts
resolve, and all validity and correctness gates pass. Otherwise continue to the next prespecified
look. This does not promise that every debug level differs significantly: practically equivalent
levels are a valid outcome.

`analyze_study.py` implements these rules and rejects duplicate blocks, reused runner identities,
mixed experiment commits, missing scheduled blocks, and changed inputs. Calibration and historical
data are excluded. Size measurements and deterministic correctness checks supplement the timing
family; they are not additional timing samples.

## Resource evidence and integrity

Record CPU model, OS, logical/physical CPU counts, memory allocation and runner identity. Sample
once per second during each compilation/training phase: host CPU and CPU-time counters, available
memory, swap and disk counters, descendant CPU times, aggregate RSS, I/O and available page-fault
counters. POSIX additionally records deltas of CPU time and fault/I/O counters for reaped children
through `getrusage`.

Sampled descendant CPU/I/O totals are lower bounds because short-lived processes can exit between
samples. Aggregate RSS can double-count shared pages, and its sampled maximum is not an exact peak.
Host counters include other activity. Some platforms return unsupported swap counters as zero;
page-fault counts can include soft faults. Unavailable metrics and collection errors are recorded
explicitly. Do not infer absence of paging from zeros or attribute causality from one aggregate
counter.

Retain the raw resource time series and build logs. Successful blocks must pass the existing
Rust/AWS-LC/jitterentropy source and negative lookup checks, SBOM retention, wheel installation,
executable/profile hashes, static CRT or signature checks, smoke tests, and matching resolver output
hashes. Keep missing/mismatched PGO diagnostics visible. Calibration must demonstrate working
process/resource collection on each OS.

Retain every attempt. Provisioning/setup failure before timing may be retried with the same block
ID/order and a recorded reason. Compiler OOM, timeout, incorrect outputs, and failures after timing
begins are outcomes, not silently discarded samples. Resolve their interpretation before making an
uncensored successful-build timing claim. A telemetry or collector error must be distinguished from
compiler failure. Do not exclude a slow successful block based on its timing or resource readings.

Publish point estimates, simultaneous intervals, both control measurements, raw stage times,
allocation/order information, failure counts, sizes, benchmarks and limitations. All old results
remain intact. Changes to this protocol must be recorded before confirmatory data are collected; do
not change margins or comparisons to obtain a result.

References:
[NIST randomized blocks](https://www.itl.nist.gov/div898/handbook/pri/section3/pri332.htm) and
[NIST sample-size considerations](https://www.itl.nist.gov/div898/handbook/prc/section2/prc222.htm).
