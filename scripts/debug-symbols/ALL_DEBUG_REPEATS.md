# Repeated debug-level measurements with thin-LTO instrumentation

The [completed five-round results](results/2026-10-13-all-debug-repeat/README.md) include all 20
allocations and 80 builds with zero failures. Median paired build/PGO overhead for full debug is
52.5% on Linux x86-64, 47.4% on Linux ARM64, 78.0% on macOS and 57.2% on Windows. Line-table and
limited medians are 10.5–18.6% and 11.0–13.7%, respectively. The report retains observed ranges, raw
stage times, all resolver samples, hardware differences and every allocation's own baseline. These
are descriptive results; the predeclared protocol follows.

Four additional rounds repeat no debug, line tables, limited and full debug on Linux x86-64, Linux
ARM64, macOS ARM64 and Windows x86-64. Each round provisions one fresh allocation per platform; each
allocation builds all four treatments sequentially with separate target directories and independent
PGO training. This adds sixteen allocations and sixty-four builds.

The [repeat plan](all-debug-repeat-plan.json) keeps the same main source revision
`59e5912ecff500422b99cca3df68d48be3841a67`, compiled-source fingerprints, toolchain, training
workloads and optimization settings as the
[completed reference round](results/2026-10-13-all-debug-pilot/README.md). Instrumentation uses thin
LTO; final compilation uses fat LTO, opt-level 3, one codegen unit and panic abort. Windows retains
static CRT. Production profiles, workflows and the PGO helper are unchanged.

| Platform       | Runner profile                                | Cargo jobs |
| -------------- | --------------------------------------------- | ---------: |
| Linux x86-64   | `depot-ubuntu-24.04-16`                       |          8 |
| Linux ARM64    | `depot-ubuntu-24.04-arm-16`                   |          8 |
| macOS ARM64    | `namespace-profile-macos-15`                  |          4 |
| Windows x86-64 | `namespace-profile-windows-2022-x86-64-16x32` |          4 |

Blocks 1–4 have predeclared orders. For each target, shuffle the four treatment labels and the four
cyclic shifts with `random.Random(f"2026101304:{target}")`, then assign shifts to blocks in order.
Each treatment occupies each build position exactly once across these four additional rounds. The
completed reference round keeps its original randomized order. Every round has a fresh no-debug
control; no baseline is shared across allocations. The block number also seeds the balanced runtime
ordering, with four warmups and forty timed Jupyter/Trio resolutions per treatment.

The same requested profile can supply different physical hardware. Record actual CPU model,
logical/physical CPU counts and RAM for every allocation, particularly the Linux x86-64 Intel/AMD
variation already observed. Do not silently change providers, requested profiles, Cargo parallelism,
source or optimization settings to select favorable timings.

After all rounds finish, summarize the completed reference plus four repeats: five allocation-level
observations per platform/treatment. Report medians and observed ranges of paired build/training
overheads and resolver changes, retaining every raw observation. Compute each paired effect first,
then summarize those effects; do not divide unrelated medians or pool baselines. Report absolute
stage/total times, executable/wheel/companion sizes, sampled memory, PGO diagnostics and correctness
outcomes. Runtime samples remain nested within each allocation and are not independent build
replications. These descriptive summaries are not confidence intervals or significance/equivalence
claims, and observed ranges do not bound future jobs.

The completed reference round's source and tool/input/output signatures must match before it enters
the summary. Older lower-debug and Windows LTO pilots remain contextual observations, not additional
members of the five-round cohort. Keep all failures, retry provenance and anomalous measurements;
report any incomplete allocation explicitly rather than dropping it from a successful-only summary.

Retain the existing verification: plan/source/order/job/attempt/upload provenance, both compilation
stages' effective flags, separate profiles and hashes, binary and wheel contents/sizes, companions,
source/negative lookups, AWS-LC/jitterentropy, SBOMs, installation/smoke checks, Windows static CRT,
macOS signatures, per-stage resource series and balanced runtime samples with identical outputs.
Record unavailable counters and sampling gaps. Native times exclude setup, symbol postprocessing,
verification and upload and do not measure production manylinux CI or the complete release pipeline.

Dispatch each registered block through `debug-symbols-study.yml` with `phase=all-debug-repeat`,
`platform=all` and its explicit `block` number (1–4). Rounds may run concurrently subject to
provider capacity. Runner outputs use `target/debug-symbols-all-debug-repeat`; artifact names are
`debug-study-all-debug-repeat-{block}-{Linux-X64,Linux-ARM64,macOS-ARM64,Windows-X64}`. Retain
reports, logs, archives, provenance and outputs under
`target/debug-symbols-experiment/all-debug-repeat-2026-10-13/block-{block}/`. The top-level
report.json is authoritative; per-variant reports are earlier snapshots.

Publish a separate repeated-results summary and link it from the experiment README without
overwriting the reference round or older studies. The earlier completed statistical study is not
restarted by this repeat cohort.
