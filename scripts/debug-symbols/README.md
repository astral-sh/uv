# Optimized Rust+C debug symbols experiment

This independent Cargo workspace tests separate debug symbols for a small Rust executable that calls
a C function compiled with `cc`. Both functions are kept out of line so source locations can be
checked under full optimization and fat LTO. The fixture uses uv's pinned Rust toolchain, Maturin
version, and `cc` version.

Run from the repository root:

```sh
uv run --no-project --with maturin==1.15.0 python scripts/debug-symbols/run.py
```

The output directory must not exist. Use `--output <directory>` for another run. Install Rust's
`llvm-tools-preview` component. Linux and Windows also need `llvm-symbolizer`; install LLVM and put
it on `PATH`, or set `LLVM_BIN` to its `bin` directory. macOS uses Xcode's command-line tools.
Windows needs the MSVC C compiler and Windows SDK. The dedicated GitHub Actions workflow runs all
three platforms.

The experiment builds two Maturin wheels from identical source:

| Build    | Debug information                 | Stripping                        | Optimization                     |
| -------- | --------------------------------- | -------------------------------- | -------------------------------- |
| Baseline | None                              | Cargo and Maturin strip symbols  | Level 3, fat LTO, abort on panic |
| Symbols  | Full Rust and C debug information | Deferred until symbols are saved | Level 3, fat LTO, abort on panic |

Both scripts accept `--debug-level line-tables-only` to reduce the symbols build's debug
information; the default is `full`. The workflow exposes the same `debug-level` choice. Line tables
retain Rust file and line information for backtraces but omit variable and parameter information.
Cargo exposes debug information to build scripts through the boolean `DEBUG` environment variable,
so the pinned `cc` crate still uses full C debug information by default (`-g` for GCC/Clang and
`/Z7` for MSVC). Native build logs and Rust/C source lookups remain part of the experiment at either
level. Reports record the selected Rust debug level; measurements below use `full` unless stated
otherwise.

Linux embeds debug information during compilation, extracts a `.debug` file with `llvm-objcopy`,
strips the executable, and adds a GNU debug link. macOS uses Cargo's packed `.dSYM` output, strips
the executable, and applies an ad hoc signature. Windows retains Cargo's packed PDB alongside its
executable.

The runner reads the executable from the Maturin wheel and checks it against the linked output. It
repacks both wheels with the prepared executables and updates `RECORD`, using the same compression
implementation for the size comparison. On macOS it applies the same ad hoc signing operation to
both executables. All temporary compiler outputs are deleted before looking up addresses, preventing
accidental use of the original build's debug files.

The experiment requires:

- Rust and C function addresses resolve to their respective source files and nonzero line numbers
  after symbol separation.
- Those source lines stop resolving when the companion symbols are removed.
- ELF build IDs or Mach-O UUIDs match. On Windows, the symbolizer loads the PDB through the
  executable's debug directory.
- Both built executables produce the expected result. Installing the processed wheel produces the
  same executable bytes and result.

Each output directory contains build logs, the baseline and processed wheels, executables, symbols,
and a `report.json` recording tool versions, source lookups, build durations, and size deltas. The
unprocessed wheels are retained with an `.original.whl` suffix for inspection; these are not
publishable artifacts.

Size equality is measured rather than required. Enabling debug information can change native
compiler flags, including frame pointers; `build-scripts/` retains native compiler invocations. Raw
Maturin wheel sizes are also recorded separately. This fixture does not exercise uv's dependency
graph, PGO, cross compilation, release signing, or publication. Those require verification with the
actual release artifacts.

## uv experiment

Build the repository's uv package using the same baseline/symbols comparison:

```sh
cargo install cargo-auditable --locked --version 0.7.6
uv run --no-project --with maturin==1.15.0 python scripts/debug-symbols/run_uv.py
```

This builds `uv` and `uvx`, plus `uvw` on Windows, with the release profile, fat LTO, `self-update`,
and the release SBOM wrapper. macOS uses the release LLD/ICF flags and Windows retains the static
CRT configuration. AWS-LC is built from the locked sources; Windows also requires NASM and disables
prebuilt NASM objects. Windows needs `llvm-pdbutil` in addition to `llvm-symbolizer`.

The runner processes all executables in the wheel and validates a Rust source location in each. For
`uv`, it additionally requires an AWS-LC `RAND_bytes` source location and checks `jent_read_entropy`
when linked. Function addresses come from the symbols; the executable source and export lists are
not modified. Source lookups run after deleting the original compiler outputs and must fail after
temporarily hiding the companion symbols.

Both builds run help/version commands and create a working Python environment offline. The processed
wheel is then installed into a separate environment; all installed executable hashes must match the
verified files. Windows additionally checks static CRT linkage, and macOS verifies ad hoc
signatures. Every executable must retain its embedded SBOM unchanged through processing. Linux uses
LLVM's GNU-compatible strip mode to retain non-debug metadata sections.

Outputs default to `target/uv-debug-symbols-experiment`. Both builds use separate, empty Cargo
target directories. The report includes build times, wheel and executable sizes, symbol sizes,
source lookups, tool versions, and the source commit. Build logs are retained even if compilation
fails. Use `--verify-only --output <directory>` to repeat validation of retained outputs without
rebuilding. CI uploads the reports, logs, wheels, and symbols for seven days, excluding the large
unprocessed wheels.

Pushes run the small fixture. To run the full comparison, dispatch the workflow with `uv` enabled
and select all platforms or one platform. Full uv builds use the repository's Depot Linux and
Namespace macOS/Windows runners to provide headroom for full debug information and fat LTO. Set
`verify-run` to a completed run ID to download its uv artifacts and repeat verification without
compiling again. Enable `uv` and select the platform whose artifacts were retained.

This comparison uses native runners without manylinux containers, cross compilation, or production
signing. It measures the effect of enabling symbols on the uv dependency graph; its baseline is not
a byte-for-byte reproduction of published release artifacts.

### PGO comparison

Pass `--pgo`, or enable the workflow's `pgo` input together with `uv`, to compare optimized builds
using the release PGO training corpus. Each mode runs `scripts/build_uv_pgo.py --train-only` with
the same debug, stripping, native compiler, and linker settings as its final build. Each final
Maturin build uses that mode's merged profile. Profiles are not reused between debug settings.
Windows retains the static CRT flags when setting `RUSTFLAGS`, and macOS disables native C/C++
profile instrumentation in both stages, matching the release workflow.

The workflow also supports native Linux ARM64 on the release's 64 GB Depot runner. It uses Rust's
bundled LLD for the instrumented build's long-range calls and sets jemalloc's page size as in the
release workflow. Together with Linux x86-64, macOS ARM64, and Windows x86-64, this covers the four
target triples that use PGO in releases. The Linux experiments still run outside manylinux
containers.

The experiment retains each merged profile, training log, native build-script logs, and profile
summary under the mode's `pgo/` directory. The report separates training and final build durations,
records profile hashes and function counts, and summarizes missing/mismatched profile warnings from
the final compilation. Full diagnostics remain in `build.log`. Training must produce executed
functions; missing-profile warnings are recorded for review because untrained functions and
launchers can legitimately have no profile data. The usual source lookup, stripping, SBOM, wheel,
and smoke checks run on the final PGO binaries after deleting the instrumented build directories.

Both modes start with empty build and training directories. This requires four optimized builds per
platform, so PGO workflow jobs have a three-hour timeout. Build durations are observations of the
build and training pipeline, not application performance benchmarks. Windows uses the repository's
32 GB Namespace release runner, with Cargo build parallelism fixed at four jobs for both
configurations. The instrumented build with full debug information exhausted memory on that runner;
`line-tables-only` provides a lower-detail comparison with the same optimization and PGO settings.

Run with Python 3.12 and pass `--benchmark`, or enable the workflow's `benchmark` input, for a small
runtime comparison of the verified binaries. It resolves the existing Jupyter and Trio requirements
against one primed cache, with two warmups and twenty timed offline resolutions per build. Build
order alternates; output files are deleted between runs to avoid reusing their pins. Both builds
must produce identical resolutions. The report retains every timing and median ratio. These
single-runner measurements cover two warm resolver workloads, not overall application performance.
This option also works with `--verify-only` / `verify-run`, so retained PGO binaries can be
benchmarked without compiling again.

### Native uv measurements without PGO

Single cold comparisons of uv 0.12.23 with Rust 1.99.0 and Maturin 1.15.0 produced the following
results. All sizes are bytes; companion symbol sizes are uncompressed and excluded from the wheels.

| Native target            | Baseline `uv` | Stripped `uv` with symbols | Executable growth |    Wheel growth | `uv` companion symbols |
| ------------------------ | ------------: | -------------------------: | ----------------: | --------------: | ---------------------: |
| x86_64-unknown-linux-gnu |    55,947,144 |                 56,223,600 |   276,456 (0.49%) |  64,566 (0.28%) |            778,511,400 |
| aarch64-apple-darwin     |    41,023,728 |                 41,192,848 |   169,120 (0.41%) | 111,554 (0.59%) |            793,413,808 |
| x86_64-pc-windows-msvc   |    47,971,328 |                 48,211,456 |   240,128 (0.50%) |  89,133 (0.43%) |            585,371,648 |

`uvx` grew by 200 bytes on Linux and 176 bytes on macOS. Windows `uvx.exe` and `uvw.exe` had no size
change. Separate symbols keep the installed artifacts close to the baseline size, but do not
guarantee identical executable sizes.

| Native target            | Baseline build | Build with symbols |
| ------------------------ | -------------: | -----------------: |
| x86_64-unknown-linux-gnu |         6m 17s |            10m 29s |
| aarch64-apple-darwin     |         5m 33s |            16m 13s |
| x86_64-pc-windows-msvc   |        10m 29s |            20m 22s |

Each target passed source lookups for its Rust entry points, AWS-LC's `RAND_bytes`, and
`jent_read_entropy`. Removing the companion files prevented those source lookups. Embedded SBOM
checks, wheel installation and executable hashes, help/version commands, and offline environment
creation also passed, along with macOS signature and Windows static CRT checks.

Build times are single observations on the configured runners, not performance benchmarks. These
native samples do not establish sizes or coverage for the complete release target matrix, PGO, or
production signing.

Reports and logs:

- [Linux build and verification](https://github.com/astral-sh/uv/actions/runs/37329101850)
- [macOS build and verification](https://github.com/astral-sh/uv/actions/runs/37332499963)
- Windows [build](https://github.com/astral-sh/uv/actions/runs/37330457108) and
  [artifact verification](https://github.com/astral-sh/uv/actions/runs/37335292541)

### Native uv measurements with PGO

The same uv, Rust, and Maturin versions produced the following results using independently trained
profiles for the baseline and symbols builds. All sizes are bytes; companion symbols are
uncompressed and excluded from the wheels.

| Native target             | Baseline `uv` | Stripped `uv` with symbols | Executable growth |    Wheel growth | `uv` companion symbols |
| ------------------------- | ------------: | -------------------------: | ----------------: | --------------: | ---------------------: |
| x86_64-unknown-linux-gnu  |    48,081,384 |                 48,311,408 |   230,024 (0.48%) | 101,731 (0.51%) |            681,367,000 |
| aarch64-unknown-linux-gnu |    41,379,224 |                 41,530,904 |   151,680 (0.37%) |  39,198 (0.21%) |            700,826,864 |
| aarch64-apple-darwin      |    35,502,816 |                 35,589,920 |    87,104 (0.25%) |  34,963 (0.20%) |            696,890,788 |

`uvx` grew by 200 bytes on Linux x86-64 and 176 bytes on Linux ARM64 and macOS. All three targets
passed Rust, AWS-LC, and jitterentropy source lookups, the negative checks with symbols hidden,
embedded SBOM validation, wheel installation with exact executable hashes, and smoke checks. macOS
ad hoc signatures also passed verification. Linux ARM64's symbolizer emitted nonfatal
`.debug_aranges` warnings; its JSON output and source lookups were valid. The harness keeps those
stderr diagnostics separate from the structured stdout used for validation.

The combined instrumented build, training, and final build durations were:

| Native target             | Baseline | With symbols |
| ------------------------- | -------: | -----------: |
| x86_64-unknown-linux-gnu  |  17m 22s |      26m 38s |
| aarch64-unknown-linux-gnu |  22m 53s |      32m 38s |
| aarch64-apple-darwin      |   15m 1s |      50m 31s |

These are single cold observations, excluding symbol processing and verification. Full debug
information increases build cost substantially even though the installed files remain close in size.
On Windows x86-64, the baseline completed in 25m 6s, but LLVM exhausted memory while compiling the
instrumented `uv` with full debug information on the 32 GB release runner. Windows PGO symbol
coverage and size comparisons with full debug information remain unverified. A request for a 64 GB
runner never acquired a machine; access to that runner configuration has not been established.

Neither Linux target reported profile mismatches. Each configuration reported 18 missing-profile
warnings, all for `uvx`. macOS reported 5,888 missing-profile warning lines for the baseline and
6,000 for the symbols build, with no mismatches. These include repeated diagnostics across uv and
its dependency crates, corresponding to 2,899 and 2,937 distinct mangled function names. Both macOS
profiles contain more than 172,000 functions and record 90 calls to `uv::main`. Of the symbols
build's functions with missing profiles, 128 remain in the final `uv` symbol table. The baseline
also has missing coverage; the measurements do not establish complete PGO coverage or explain every
missing function.

Twenty timed cached resolutions per configuration produced identical dependency resolutions. The
median wall times, including process startup, were:

| Native target             | Jupyter baseline / symbols | Trio baseline / symbols |
| ------------------------- | -------------------------: | ----------------------: |
| x86_64-unknown-linux-gnu  |           8.582 / 8.915 ms |        7.229 / 7.123 ms |
| aarch64-unknown-linux-gnu |         11.456 / 11.482 ms |        8.793 / 8.804 ms |
| aarch64-apple-darwin      |         13.644 / 13.525 ms |      13.212 / 13.449 ms |

Every median difference was below 0.4 ms. These short cached workloads are a smoke comparison, not
evidence of equivalent performance across uv workloads. The native Linux results also do not
validate the release's manylinux containers or the remaining non-PGO release targets.

Reports and logs:

- [Linux x86-64 and macOS builds; Windows memory failure](https://github.com/astral-sh/uv/actions/runs/37338399810)
- [Linux x86-64 verification and timings](https://github.com/astral-sh/uv/actions/runs/37344439379)
- Linux ARM64 [build](https://github.com/astral-sh/uv/actions/runs/37338680716) and
  [artifact verification and timings](https://github.com/astral-sh/uv/actions/runs/37346420376)
- [macOS verification and timings](https://github.com/astral-sh/uv/actions/runs/37347084186)

### Windows PGO with line tables

The `line-tables-only` comparison completed successfully on
`namespace-profile-windows-2022-x86-64-16x32`. The runner reported 16 logical processors and
34,359,107,584 bytes of physical memory. Both modes used four Cargo jobs, fat LTO, static CRT
linkage, and independently trained PGO profiles. Peak process memory was not measured; this
establishes successful completion on the 32 GB runner, not a precise memory reduction.

| Measurement                      | No debug information | Line tables |
| -------------------------------- | -------------------: | ----------: |
| Instrumented build and training  |              19m 14s |      24m 6s |
| Final build and wheel generation |              11m 18s |       9m 7s |
| Combined pipeline                |              30m 31s |     33m 13s |
| `uv.exe` bytes                   |           40,245,760 |  40,176,640 |
| Wheel bytes                      |           18,137,249 |  18,104,535 |

The pipeline took 8.8% longer overall. The executable was 69,120 bytes smaller (0.17%) and the wheel
32,714 bytes smaller (0.18%). These are single observations with separate PGO training; the small
size differences are not a guarantee of smaller binaries.

| Companion file | Uncompressed bytes |
| -------------- | -----------------: |
| `uv.pdb`       |        150,409,216 |
| `uvx.pdb`      |          4,509,696 |
| `uvw.pdb`      |          4,509,696 |

A standalone ZIP containing all three PDBs, compressed with Python's `ZIP_DEFLATED` at level 9, was
45,362,507 bytes. It contains only companion symbols, excluding the executables, wheels, profiles,
and logs. The earlier full-debug Windows PDB was 585,371,648 bytes without PGO; that is not a
controlled comparison of debug levels because PGO also changes the output.

Rust source lines in all three executables, AWS-LC's `RAND_bytes`, and `jent_read_entropy` resolved
through the retained PDBs and stopped resolving when the PDBs were hidden. Embedded SBOM checks,
static CRT checks, wheel installation with exact executable hashes, and smoke checks passed.
AWS-LC's retained compiler logs show `/Z7` debug information in both PGO stages, despite the reduced
Rust debug level. Both modes reported 19 missing-profile warnings confined to `uvx` and `uvw`, with
no profile mismatches.

The cached resolver comparisons produced identical resolutions. Median baseline/line-table times
were 26.176/26.121 ms for Jupyter and 16.976/16.957 ms for Trio. These short workloads do not
establish overall runtime equivalence.

[Windows line-table PGO build, verification, and timings](https://github.com/astral-sh/uv/actions/runs/37352288394)

### Linux and macOS PGO with line tables

Full uv comparisons with `line-tables-only` completed on the same runner profiles as the earlier
`full` experiments, with the same Rust toolchain, dependencies, optimization settings, and training
corpus. The uv source was unchanged; the intervening commits updated the experiment tooling and
documentation. Each run built a fresh no-debug baseline and trained independent PGO profiles.

Combined instrumented build, training, and final build times were, using the no-debug baseline from
each line-table run:

| Native target             | No debug | Full debug info | Line tables | Reduction vs. full |
| ------------------------- | -------: | --------------: | ----------: | -----------------: |
| x86_64-unknown-linux-gnu  |  17m 24s |         26m 38s |     19m 19s |              27.5% |
| aarch64-unknown-linux-gnu |  23m 57s |         32m 38s |      26m 6s |              20.0% |
| aarch64-apple-darwin      |   15m 1s |         50m 31s |     16m 39s |              67.0% |

The fresh baselines help account for differences between runners and runs:

| Native target             | No-debug baseline in full run | No-debug baseline in line-table run | Full overhead over its baseline | Line-table overhead over its baseline |
| ------------------------- | ----------------------------: | ----------------------------------: | ------------------------------: | ------------------------------------: |
| x86_64-unknown-linux-gnu  |                       17m 22s |                             17m 24s |                           53.3% |                                 11.0% |
| aarch64-unknown-linux-gnu |                       22m 53s |                             23m 57s |                           42.6% |                                  9.0% |
| aarch64-apple-darwin      |                        15m 1s |                              15m 1s |                          236.5% |                                 10.8% |

Together with Windows's 8.8% overhead, line tables added roughly 9–11% to the measured PGO build and
training pipeline on each native target. These are single cold observations, not repeated benchmarks
or timings of the production release workflow. They exclude runner queueing, setup, symbol
processing, verification, and artifact upload. The Linux runs do not use manylinux containers, and
production signing is not exercised.

The uncompressed `uv` companions were also substantially smaller. No-debug builds do not produce
separate symbol companions:

| Native target             | No-debug symbol bytes | Full symbol bytes | Line-table symbol bytes | Size reduction vs. full |
| ------------------------- | --------------------: | ----------------: | ----------------------: | ----------------------: |
| x86_64-unknown-linux-gnu  |                     0 |       681,367,000 |             197,367,616 |                   71.0% |
| aarch64-unknown-linux-gnu |                     0 |       700,826,864 |             210,404,032 |                   70.0% |
| aarch64-apple-darwin      |                     0 |       696,890,788 |             225,785,082 |                   67.6% |

Each target passed Rust entry-point, AWS-LC, and jitterentropy source lookups; removing the
companion symbols prevented those lookups. Embedded SBOM validation, wheel installation with exact
executable hashes, smoke checks, and macOS ad hoc signature verification passed. Relative to their
fresh baselines, `uv` changed by -14,328 bytes on Linux x86-64, -11,856 bytes on Linux ARM64, and
+36,400 bytes on macOS. Wheel size changes were -13,299, -30,649, and +9,450 bytes respectively.

Neither debug setting reported PGO profile mismatches. The Linux configurations each reported 18
missing-profile warnings. macOS reported 5,888 for the no-debug baseline and 5,883 for line tables;
the earlier full-debug build reported 6,000. These warnings still require the coverage caveats
described above.

Cached Jupyter and Trio resolutions were identical between each baseline and line-table binary. All
six median timing differences were below 0.14 ms; these limited workloads do not establish overall
runtime equivalence.

Completed uv jobs:

- [Linux x86-64](https://github.com/astral-sh/uv/actions/runs/37365248373/job/111948713197)
- [Linux ARM64](https://github.com/astral-sh/uv/actions/runs/37365250301/job/111948718615)
- [macOS ARM64](https://github.com/astral-sh/uv/actions/runs/37365252485/job/111948724584)

The Linux x86-64 workflow is marked failed because an auxiliary macOS fixture job never acquired a
GitHub-hosted runner and was canceled. Its full uv job passed; the macOS full uv experiment passed
in its separate run.
