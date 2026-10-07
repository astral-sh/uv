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

Both scripts accept `--debug-level limited` or `--debug-level line-tables-only` to reduce the
symbols build's debug information; the default is `full`. The workflow exposes the same
`debug-level` choices. Line tables retain Rust file and line information for backtraces but omit
variable and parameter information. Limited debug information adds module-level metadata but still
omits type and variable information. Cargo exposes debug information to build scripts through the
boolean `DEBUG` environment variable, so the pinned `cc` crate still uses full C debug information
by default (`-g` for GCC/Clang and `/Z7` for MSVC). Native build logs and Rust/C source lookups
remain part of the experiment at every level. Reports record the selected Rust debug level;
measurements below use `full` unless stated otherwise.

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
the same codegen-unit, debug, stripping, native compiler, and linker settings as its final build.
Each final Maturin build uses that mode's merged profile. Profiles are not reused between debug
settings. Windows retains the static CRT flags when setting `RUSTFLAGS`, and macOS disables native
C/C++ profile instrumentation in both stages, matching the release workflow.

The uv script accepts `--codegen-units 1` or `--codegen-units 16`; the workflow exposes the same
`codegen-units` choice. The default is 16, matching the release profile used for the measurements
below. This sets `CARGO_PROFILE_RELEASE_CODEGEN_UNITS` for both the no-debug and symbols builds,
including their instrumented PGO builds, and records the value in the report. Other optimization
settings and Cargo job counts remain unchanged. This permits testing the single-codegen-unit setting
from [#22303](https://github.com/astral-sh/uv/pull/22303) without changing production profiles or
runner sizes. Compare debug overhead against the no-debug baseline from the same run; comparisons
against earlier 16-unit runs also include variation in runner load, caches, and training workloads.
The small Rust+C fixture does not use this option.

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
build and training pipeline, not application performance benchmarks. Windows defaults to the
repository's 32 GB Namespace release runner, with Cargo build parallelism fixed at four jobs for
both configurations. The instrumented build with full debug information and 16 codegen units
exhausted memory on that runner. The `windows-runner` input also accepts
`namespace-profile-windows-2022-x86-64-32x64`, but the attempted run on that profile was cancelled
without acquiring a runner; it has no build measurements. Both the no-debug baseline and symbols
build run on the selected profile; Cargo parallelism remains four. Single-codegen-unit comparisons
can test whether the default 32 GB profile is sufficient for full debug information.

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
coverage and size comparisons with full debug information remain unverified. An earlier request
using the direct `nscloud-windows-2022-amd64-32x64` label never acquired a machine. The experiment
now supports selecting the configured `namespace-profile-windows-2022-x86-64-32x64` profile.

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

### Native PGO debug-level comparison

Comparisons with `full`, `limited`, and `line-tables-only` used the same uv source, Rust toolchain,
dependencies, optimization settings, and training corpus. Each run built a fresh no-debug baseline
and trained independent PGO profiles. Completed comparisons used the same runner profile for each
platform. Windows measurements below used the 32 GB Namespace profile; the full-debug experiment on
the 64 GB profile has not produced measurements.

The [64 GB Windows job](https://github.com/astral-sh/uv/actions/runs/37667974816/job/112952026195)
requested `namespace-profile-windows-2022-x86-64-32x64` at commit
`21fc3463b7e1bf1fdae9c27dfc39ecc68c91e400`. It was manually canceled after 57m 13s without a runner
assignment or any executed steps. This is a scheduling outcome, not a compiler or verification
failure; physical runner capacity and full-debug memory requirements remain unverified. GitHub
reported no scheduling error explaining the lack of assignment.

Combined instrumented build, training, and final build wall times are paired with the no-debug
baseline from the same run. Both Windows limited observations are shown:

| Platform       | Debug level     | No-debug baseline |         Symbols build | Paired overhead |
| -------------- | --------------- | ----------------: | --------------------: | --------------: |
| Linux x86-64   | Line tables     |           17m 24s |               19m 19s |          +11.0% |
| Linux x86-64   | Limited         |            21m 6s |               23m 26s |          +11.1% |
| Linux x86-64   | Full            |           17m 22s |               26m 38s |          +53.3% |
| Linux ARM64    | Line tables     |           23m 57s |                26m 6s |           +9.0% |
| Linux ARM64    | Limited         |            23m 6s |                25m 8s |           +8.8% |
| Linux ARM64    | Full            |           22m 53s |               32m 38s |          +42.6% |
| macOS ARM64    | Line tables     |            15m 1s |               16m 39s |          +10.8% |
| macOS ARM64    | Limited         |           14m 56s |               16m 46s |          +12.2% |
| macOS ARM64    | Full            |            15m 1s |               50m 31s |         +236.5% |
| Windows x86-64 | Line tables     |           30m 31s |               33m 13s |           +8.8% |
| Windows x86-64 | Limited, first  |            30m 3s |               40m 30s |          +34.8% |
| Windows x86-64 | Limited, repeat |           27m 25s |                33m 8s |          +20.9% |
| Windows x86-64 | Full            |            25m 6s | Out of memory (32 GB) |    Not measured |

Line tables added roughly 9–11% to the measured PGO build and training pipeline on each native
target. Windows limited overhead varied from 34.8% to 20.9%; the repeat does not establish a stable
cost or isolate the cause. These observations are not timings of the production release workflow.
They exclude runner queueing, setup, symbol processing, verification, and artifact upload. The Linux
runs do not use manylinux containers, and production signing is not exercised.

Uncompressed `uv` companion sizes are bytes. No-debug builds do not produce separate symbol
companions. Windows limited sizes use the repeat; the first observation is retained below:

| Platform       | No-debug symbols | Line-table symbols | Limited symbols | Full symbols |
| -------------- | ---------------: | -----------------: | --------------: | -----------: |
| Linux x86-64   |                0 |        197,367,616 |     292,576,848 |  681,367,000 |
| Linux ARM64    |                0 |        210,404,032 |     305,129,584 |  700,826,864 |
| macOS ARM64    |                0 |        225,785,082 |     337,300,065 |  696,890,788 |
| Windows x86-64 |                0 |        150,409,216 |     150,392,832 | Not measured |

Shipped `uv` executable changes are bytes and percentages relative to each run's own no-debug
baseline. Windows limited again uses the repeat; its first observation was +5,632 bytes (+0.014%):

| Platform       | Line-table executable delta | Limited executable delta | Full executable delta |
| -------------- | --------------------------: | -----------------------: | --------------------: |
| Linux x86-64   |           -14,328 (-0.030%) |         +6,408 (+0.013%) |    +230,024 (+0.478%) |
| Linux ARM64    |           -11,856 (-0.029%) |        -21,328 (-0.052%) |    +151,680 (+0.367%) |
| macOS ARM64    |           +36,400 (+0.102%) |        +52,896 (+0.149%) |     +87,104 (+0.245%) |
| Windows x86-64 |           -69,120 (-0.172%) |         -1,536 (-0.004%) |          Not measured |

The Windows limited repeat's PDB was 40,960 bytes larger than the first observation (0.03%). Its
executable delta changed from +5,632 to -1,536 bytes, and its wheel delta from +2,159 to +416 bytes.
Separate PGO training and debug settings can change code generation; small size differences do not
guarantee identical release binaries. Detailed wheel sizes and stage timings for both Windows
limited observations appear below.

Each line-table target passed Rust entry-point, AWS-LC, and jitterentropy source lookups; removing
the companion symbols prevented those lookups. Embedded SBOM validation, wheel installation with
exact executable hashes, smoke checks, and macOS ad hoc signature verification passed. Relative to
their fresh baselines, `uv` changed by -14,328 bytes on Linux x86-64, -11,856 bytes on Linux ARM64,
and +36,400 bytes on macOS. Wheel size changes were -13,299, -30,649, and +9,450 bytes respectively.

Neither the full nor line-table setting reported PGO profile mismatches. The Linux configurations
each reported 18 missing-profile warnings. macOS reported 5,888 for the no-debug baseline and 5,883
for line tables; the earlier full-debug build reported 6,000. These warnings still require the
coverage caveats described above.

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

### Limited PGO on all native targets

All four `limited` comparisons passed at commit `b00c5d1791ca87f9fba676dfb01b9e2aa8b06c50`, using
Rust 1.99.0, LLVM 23.1.1, and Maturin 1.15.0. Debug information was set to `limited` for both the
instrumented and final symbols builds. Each baseline used `none`. Runner profiles, Cargo job limits,
fat LTO, and the PGO training corpus matched the line-table comparisons.

Combined instrumented build, training, and final build times, with each limited run's baseline.
Windows uses the repeat; both Windows observations are detailed below:

| Native target             | No debug | Limited | Limited overhead | Line-table overhead in its own run |
| ------------------------- | -------: | ------: | ---------------: | ---------------------------------: |
| x86_64-unknown-linux-gnu  |   21m 6s | 23m 26s |            11.1% |                              11.0% |
| aarch64-unknown-linux-gnu |   23m 6s |  25m 8s |             8.8% |                               9.0% |
| aarch64-apple-darwin      |  14m 56s | 16m 46s |            12.2% |                              10.8% |
| x86_64-pc-windows-msvc    |  27m 25s |  33m 8s |            20.9% |                               8.8% |

The Linux x86-64 no-debug baseline took 21.3% longer than in the line-table run; its limited build
also took 21.4% longer than the line-table build. The paired overheads were nearly identical, so the
raw timing difference does not establish an added cost from `limited`. Linux ARM64's baseline and
symbols timings both decreased by roughly 3.5–3.7%. macOS baselines were within 0.6%.

Windows completed on `namespace-profile-windows-2022-x86-64-16x32` with four Cargo jobs. The
repeat's limited instrumented build and training took 22m 29s, and the final build took 10m 40s. Its
combined symbols time was within 0.3% of the line-table run, with a 10.2% shorter no-debug baseline.
The first limited observation took 40m 30s overall and measured 34.8% paired overhead. The repeat's
20.9% overhead shows substantial timing variation. Full-debug PGO previously exhausted memory on
this runner profile; there is no completed full-debug PGO timing or PDB size for comparison. Peak
memory was not measured.

Limited `uv` symbols were 48.2%, 45.0%, and 49.4% larger than line tables on Linux x86-64, Linux
ARM64, and macOS respectively. In the Windows repeat, `uv.pdb` was 150,392,832 bytes versus
150,409,216 bytes with line tables, a difference of only 0.01%. The limited `uvx.pdb` was 4,509,696
bytes and `uvw.pdb` was 4,501,504 bytes. All sizes are uncompressed; symbol size alone does not
establish which additional debugger capabilities are available.

All targets passed Rust entry-point, AWS-LC, and jitterentropy source lookups and the negative
checks with companions hidden. Embedded SBOM checks, wheel installation with exact executable
hashes, smoke checks, macOS ad hoc signing, and Windows static CRT checks passed. Downloaded
executable hashes and companion sizes also matched the reports. The checks establish source lookup
coverage; they do not test variable inspection or a representative crash dump.

Limited builds changed the shipped executable and processed wheel sizes relative to their paired
no-debug builds by:

| Native target             | `uv` executable delta bytes | Wheel delta bytes |
| ------------------------- | --------------------------: | ----------------: |
| x86_64-unknown-linux-gnu  |                      +6,408 |              +567 |
| aarch64-unknown-linux-gnu |                     -21,328 |           -54,837 |
| aarch64-apple-darwin      |                     +52,896 |           +12,661 |
| x86_64-pc-windows-msvc    |                      -1,536 |              +416 |

Separate PGO training and debug settings can affect code generation; these small differences do not
guarantee identical release sizes. Every mode reported zero PGO profile mismatches. Missing profile
counts matched the line-table comparisons: 18 for each Linux build, 19 for each Windows build, and
5,888/5,883 for macOS baseline/limited. The macOS coverage caveat above still applies.

Cached resolver outputs matched in every comparison. Median baseline/limited times in milliseconds
were:

| Native target             | Jupyter baseline | Jupyter limited | Trio baseline | Trio limited |
| ------------------------- | ---------------: | --------------: | ------------: | -----------: |
| x86_64-unknown-linux-gnu  |           12.615 |          12.608 |        10.425 |       10.262 |
| aarch64-unknown-linux-gnu |           10.685 |          10.610 |         8.227 |        8.299 |
| aarch64-apple-darwin      |           11.221 |          11.334 |         9.258 |        9.294 |
| x86_64-pc-windows-msvc    |           31.151 |          32.331 |        21.528 |       21.299 |

All median differences were below 1.2 ms. These short workloads and limited build observations do
not establish overall runtime equivalence or precise production CI costs.

Completed limited uv jobs:

- [Linux x86-64](https://github.com/astral-sh/uv/actions/runs/37643868981/job/112869430069)
- [Linux ARM64](https://github.com/astral-sh/uv/actions/runs/37643868981/job/112869430665)
- [macOS ARM64](https://github.com/astral-sh/uv/actions/runs/37643868981/job/112869430285)
- Windows x86-64
  [first observation](https://github.com/astral-sh/uv/actions/runs/37643868981/job/112869430170) and
  [repeat](https://github.com/astral-sh/uv/actions/runs/37643868981/job/112928391792)

### Windows limited PGO repeat

The second Windows `limited` comparison passed at the same commit
`b00c5d1791ca87f9fba676dfb01b9e2aa8b06c50`, using four Cargo jobs and the same
`namespace-profile-windows-2022-x86-64-16x32` profile. The runner reported 16 logical processors and
34,359,107,584 bytes of physical memory. Rust 1.99.0, LLVM 23.1.1, Maturin 1.15.0, optimization
level 3, fat LTO, static CRT linkage, and independent PGO training remained unchanged.

Each observation includes its own fresh no-debug baseline. Wall times exclude setup, symbol
processing, verification, and uploads; overheads use unrounded durations:

| Comparison      | Stage                           | No debug | Symbols | Paired overhead |
| --------------- | ------------------------------- | -------: | ------: | --------------: |
| Line tables     | Instrumented build and training |  19m 14s |  24m 6s |          +25.3% |
| Line tables     | Final build and wheel           |  11m 18s |   9m 7s |          -19.3% |
| Line tables     | Combined                        |  30m 31s | 33m 13s |           +8.8% |
| Limited, first  | Instrumented build and training |  20m 16s |  30m 1s |          +48.2% |
| Limited, first  | Final build and wheel           |   9m 47s | 10m 29s |           +7.1% |
| Limited, first  | Combined                        |   30m 3s | 40m 30s |          +34.8% |
| Limited, repeat | Instrumented build and training |  18m 47s | 22m 29s |          +19.7% |
| Limited, repeat | Final build and wheel           |   8m 38s | 10m 40s |          +23.5% |
| Limited, repeat | Combined                        |  27m 25s |  33m 8s |          +20.9% |

The original 34.8% total overhead did not repeat: the second observation measured 20.9%. The limited
pipeline was 18.2% faster than the first limited run, while its baseline was 8.8% faster. Most of
the symbols-build improvement came from instrumented compilation and training, which fell by 25.1%;
the final build was 1.7% slower. Its total symbols time was within 0.3% of the line-table run, but
its baseline was 10.2% faster. Two limited observations and one line-table observation do not
establish a stable debug-level overhead or isolate the cause of the variation. Baselines always run
first, and separate PGO training and runner conditions remain sources of variation.

All sizes below are bytes. Executable and wheel pairs are no-debug baseline / symbols build; PDB
sizes are uncompressed and excluded from the wheels:

| Comparison      |    `uv.pdb` | `uv.exe` baseline / symbols | Executable delta | Wheel baseline / symbols | Wheel delta |
| --------------- | ----------: | --------------------------: | ---------------: | -----------------------: | ----------: |
| Line tables     | 150,409,216 |     40,245,760 / 40,176,640 |          -69,120 |  18,137,249 / 18,104,535 |     -32,714 |
| Limited, first  | 150,351,872 |     40,146,432 / 40,152,064 |           +5,632 |  18,097,496 / 18,099,655 |      +2,159 |
| Limited, repeat | 150,392,832 |     40,158,208 / 40,156,672 |           -1,536 |  18,102,103 / 18,102,519 |        +416 |

The repeat's `uv.pdb` grew by only 40,960 bytes (0.03%) from the first limited observation and was
16,384 bytes smaller than the line-table PDB. Its `uvx.pdb` was 4,509,696 bytes and `uvw.pdb` was
4,501,504 bytes; both launchers' PDBs were 4,509,696 bytes in the earlier observations. The repeat's
shipped `uv.exe` changed by -0.004% and its wheel by +0.002% relative to its own baseline. These
measurements do not guarantee identical binaries or establish additional debugger capabilities.

Rust source lookups in all three executables, AWS-LC `RAND_bytes`, jitterentropy, negative lookups
with PDBs hidden, SBOM checks, static CRT checks, wheel installation, and smoke checks passed.
Downloaded executable and profile hashes, wheel contents, symbol sizes, and benchmark output hashes
matched the report. Both modes reported 19 missing-profile warnings, confined to `uvx` and `uvw`,
and zero mismatches, matching the first limited and line-table observations.

Twenty timed cached resolutions per mode produced identical outputs. Repeat median baseline /
limited wall times were 31.151 / 32.331 ms for Jupyter (+3.8%, or 1.181 ms) and 21.528 / 21.299 ms
for Trio (-1.1%, or 0.229 ms). The first limited observation measured 32.777 / 32.379 ms and 22.111
/ 21.527 ms respectively. Jupyter's direction changed between observations; these short workloads do
not establish an overall runtime regression or equivalence.

The
[attempt-2 Windows job](https://github.com/astral-sh/uv/actions/runs/37643868981/job/112928391792)
uploaded
[artifact 11503259981](https://github.com/astral-sh/uv/actions/runs/37643868981/artifacts/11503259981)
at 18:45:01 UTC on October 7, 2026. Its creation time and upload log identify it as the repeat,
despite sharing an artifact name with attempt 1. The archive digest was verified, and both
observations' reports and outputs were retained separately.
