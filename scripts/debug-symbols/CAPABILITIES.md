# Debugger capability probes

The [October 14 native results](results/2026-10-14-capabilities/README.md) include all four targets,
live positive/negative controls, debugger transcripts and optimization edge cases. Full debug
provides state inspection that reduced levels omit, while retaining important limits on variable
availability and partial-object displays.

This experiment tests actual debugger operations on a small Rust+C program. It complements the
build-time studies, whose symbol checks establish address-to-source lookup but do not establish
variable inspection, inline-stack reconstruction or stepping quality.

The native matrix is Linux x86-64, Linux ARM64, macOS ARM64 and Windows x86-64. Each allocation uses
the repository's Rust toolchain and its Rust LLDB formatters. The debugger version, raw
observations, exact source hashes, compiler commands and runner identity are retained. Results
describe that compiler/debugger/target combination, not every debugger implementation.

Five variants are built independently:

| Variant          | Rust debug information | Optimization              | PGO                      |
| ---------------- | ---------------------- | ------------------------- | ------------------------ |
| Positive control | Full                   | Level 0                   | None                     |
| Negative control | None                   | Level 3, fat LTO, one CGU | Thin-LTO instrumentation |
| Line tables      | Line tables only       | Level 3, fat LTO, one CGU | Thin-LTO instrumentation |
| Limited          | Limited                | Level 3, fat LTO, one CGU | Thin-LTO instrumentation |
| Full             | Full                   | Level 3, fat LTO, one CGU | Thin-LTO instrumentation |

All variants use panic abort and Windows static CRT. Every optimized variant trains a fresh profile
using seeds 0–63. Live debugging uses seed 17; every executable must produce the same known
checksum. The C probe follows Cargo's native optimization/debug settings; reduced Rust debug levels
can still produce full C debug information. As in the production PGO helper, macOS disables native C
profile generation/use to avoid mixing Apple Clang and Rust LLVM profiling runtimes.

The executable is stripped after extracting its companion, with platform identity checks. Original
build directories are removed before debugging. LLDB loads the retained companion explicitly and
disables external symbol lookup. A second session runs an isolated binary without the companion;
fixture source locations and source breakpoints must then be unavailable. Windows CDB replays add an
independent native-debugger check with the same retained PE/PDB files and a missing-PDB control.

The fixture covers:

| Probe                   | Observation                                                                                                 |
| ----------------------- | ----------------------------------------------------------------------------------------------------------- |
| Live values             | Known runtime argument, scalar and constant; memory-resident struct, tuple, option, enum, string and vector |
| Optimized intermediates | Unused arithmetic, a folded bit-mask intermediate, a constant and a value consumed before the stop          |
| Shadowing               | Inner and outer bindings with different known values                                                        |
| Inline chain            | Forced inline leaf/middle functions, their logical frames and local variables                               |
| Partial structure       | One consumed field and one unused field, without forcing the object into memory                             |
| Last use                | The same local before and after its final use                                                               |
| Native C                | C argument, scalar and structure while Rust debug level varies                                              |
| Source stepping         | A file/line breakpoint followed by six source step-over operations                                          |
| Tail call               | A forwarding caller that can lose its physical frame; record real, inline and artificial frames             |

The checkpoint itself is exported so no-debug and missing-symbol controls can stop at the same
locations. LLDB reads variables through its debug-information API and Rust formatters. CDB reads
named variables and fields using its local-variable display and typed expressions; neither backend
executes target functions or computes the expected values as a substitute for inspection. Both raw
and synthetic children are retained. Memory-resident live values form a positive control; other
probes deliberately allow optimization. An optimized-away value is an observation, not a harness
failure. A failed unoptimized positive control is an infrastructure/tool-compatibility failure
requiring investigation.

No fixed list of variables is required to disappear. The compiler can preserve constants or
reconstruct values even when it removes their physical storage. Likewise, a variable that is
readable at one checkpoint is not guaranteed readable throughout its source scope. Source stepping
records actual stops rather than assuming one debugger step per source line.

Run the native experiment with:

```shell
python scripts/debug-symbols/run_capabilities.py
```

It requires the repository Rust toolchain with `llvm-tools-preview`, a native C compiler, LLDB with
Python scripting and `psutil==7.0.0` for the shared harness imports. GitHub Actions provides these
through `debug-symbols-capabilities.yml`; `platform=all` runs the native matrix concurrently. A
`--build-only` preflight is available where debugger attachment is prohibited, but it produces no
debugger capability claims.

Artifacts retain `report.json`, verbose instrumented/final build logs, individual PGO profiles,
companion symbols, binaries, LLDB commands/logs, detailed checkpoint observations and missing-symbol
controls. Failed variants retain their evidence and do not prevent independent variants from being
attempted. These fixtures do not measure variable availability in the complete uv binary, core-dump
support, or performance costs.

`replay_capabilities.py` can repeat observations on hash-verified retained binaries without building
them again. It records the compiled source commit separately from the observer commit. Replays can
add normal debugger CLI output alongside structured values and use lexical lookup for shadowed
names. The original observations remain retained. `backend=cdb` in the workflow uses the Windows SDK
debugger engine; `platform=Unix` selects the three Unix targets for LLDB replays. Both require a
source run ID and exact compiled-source commit.
