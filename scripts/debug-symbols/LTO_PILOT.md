# Instrumentation LTO pilot

The [completed Windows results](results/2026-10-13-lto-pilot/README.md) show a 29.5% reduction in
combined build/training time without debug information and 35.0% with full debug information. All
four variants passed; this remains one exploratory allocation.

Compare fat and thin LTO during PGO instrumentation on one Windows x86-64 allocation, using main
revision `59e5912ecff500422b99cca3df68d48be3841a67`. The pilot has four clean build variants: fat/no
debug, fat/full debug, thin/no debug and thin/full debug. Every final compilation uses fat LTO, one
codegen unit, optimization level 3 and static CRT.

The [plan](lto-pilot-plan.json) records source fingerprints, runner profile, four Cargo jobs and the
build order, selected before timing with seed 20261013. Each variant trains its own PGO profile.
Source, workload, toolchain and final optimization settings are held constant. One allocation is
exploratory evidence; it cannot establish a statistically significant LTO effect or generalize to
other platforms.

The production PGO helper remains unchanged. An experiment-only driver observes its preparation,
instrumented compilation, training and profile-merge calls. Thin mode retains the helper's
instrumentation flags; fat mode replaces only the instrumentation LTO selection. Both modes log
verbose compiler commands. The pilot checks the effective final occurrence of each compiler setting,
including LTO and profile-generate/profile-use, for uv, uvx and uvw. This accounts for Cargo's
profile flags preceding explicit RUSTFLAGS overrides.

The existing harness retains overall PGO and final-build resource traces, executable/profile hashes,
wheel and symbol sizes, SBOM checks, static CRT, wheel installation, smoke checks, and
Rust/AWS-LC/jitterentropy positive and negative source lookups. The stage report separates
instrumented compilation from training waits. Final compilation and wheel construction remain a
separate measured stage; symbol postprocessing and upload are outside those build timers.

Jupyter and Trio resolution use four warmups and forty timed samples per variant, with balanced
randomized order and matching output hashes. They are descriptive checks of the four produced
binaries, not independent machine or profile replications.

Dispatch one Windows job through the experiment workflow:

```shell
gh workflow run debug-symbols-study.yml --repo astral-sh/uv \
  --ref ee/publish-debug-symbols \
  -f phase=lto-pilot -f platform=Windows -f block=0
```

The workflow rejects other platform selections for this pilot. Artifacts use
`debug-study-lto-pilot-0-Windows-X64`; outputs are in `target/debug-symbols-lto-pilot`. Collect
artifacts by ID, commit, attempt and upload time. Retain partial reports and failure logs; failed
independent variants do not prevent the other variants from being attempted, and any failure still
fails the job. No production release workflow is dispatched.
