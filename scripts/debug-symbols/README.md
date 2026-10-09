# Optimized Rust+C debug symbols experiment

The randomized statistical follow-up is specified in [STUDY.md](STUDY.md). It adds independent
runner blocks, randomized build order, duplicate no-debug controls, resource sampling, and
predeclared simultaneous inference. The measurements below remain the exploratory record; they are
not counted as confirmatory samples in that study.

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
`codegen-units` choice. The default is 1, matching the current release profile; the earlier
measurements below identify their codegen-unit setting. This sets
`CARGO_PROFILE_RELEASE_CODEGEN_UNITS` for both the no-debug and symbols builds, including their
instrumented PGO builds, and records the value in the report. Other optimization settings and Cargo
job counts remain unchanged. This permits testing the single-codegen-unit setting from
[#22303](https://github.com/astral-sh/uv/pull/22303) without changing production profiles or runner
sizes. Compare debug overhead against the no-debug baseline from the same run; comparisons against
earlier 16-unit runs also include variation in runner load, caches, and training workloads. The
small Rust+C fixture does not use this option.

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
build run on the selected profile; Cargo parallelism remains four. The single-codegen-unit
full-debug comparison completed on the default 32 GB profile; its measurements are recorded below.

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

### Full debug with one codegen unit

[Run 37684681738](https://github.com/astral-sh/uv/actions/runs/37684681738), at commit
`dd77ecdf26cf3d0ca3867be2f77bd8a8925a023d`, compares a fresh no-debug baseline with full debug using
one codegen unit for both modes and their PGO training. Rust 1.99.0, LLVM 23.1.1, Maturin 1.15.0,
optimization level 3, fat LTO, Cargo job counts, and runner profiles match the earlier comparisons.
The codegen-unit count is the intentional optimization change. All four platforms completed
successfully, including Windows on the same 32 GB profile that exhausted memory with 16 units.

Combined wall times include instrumented compilation, training, and the final build and wheel. Each
cell is no debug → full debug, with overhead against that run's own baseline:

| Platform       |           16 codegen units |             1 codegen unit |
| -------------- | -------------------------: | -------------------------: |
| Linux x86-64   | 17m 22s → 26m 38s (+53.3%) |  14m 9s → 20m 51s (+47.4%) |
| Linux ARM64    | 22m 53s → 32m 38s (+42.6%) |  19m 6s → 27m 17s (+42.8%) |
| macOS ARM64    | 15m 1s → 50m 31s (+236.5%) | 12m 26s → 31m 7s (+150.4%) |
| Windows x86-64 |      25m 6s → OOM on 32 GB | 26m 22s → 38m 39s (+46.6%) |

The one-unit stages, with the same baseline → full convention, were:

| Platform       | Instrumented build and training |     Final build and wheel |
| -------------- | ------------------------------: | ------------------------: |
| Linux x86-64   |       8m 58s → 12m 48s (+42.9%) |   5m 11s → 8m 3s (+55.3%) |
| Linux ARM64    |       12m 8s → 16m 48s (+38.5%) | 6m 58s → 10m 29s (+50.4%) |
| macOS ARM64    |      7m 59s → 21m 52s (+173.6%) | 4m 26s → 9m 15s (+108.6%) |
| Windows x86-64 |       18m 0s → 24m 45s (+37.5%) | 8m 22s → 13m 54s (+66.2%) |

Sizes below are bytes. Executable and wheel cells show no debug → full debug and their paired delta.
Companion symbols are uncompressed and excluded from the wheel:

| Platform       |                              Stripped `uv` |                            Processed wheel | Full `uv` symbols |
| -------------- | -----------------------------------------: | -----------------------------------------: | ----------------: |
| Linux x86-64   | 39,926,920 → 39,950,384 (+23,464; +0.059%) | 17,304,686 → 17,321,965 (+17,279; +0.100%) |       574,947,544 |
| Linux ARM64    | 33,588,408 → 33,626,024 (+37,616; +0.112%) |  16,374,577 → 16,384,058 (+9,481; +0.058%) |       587,737,968 |
| macOS ARM64    | 29,418,800 → 29,488,544 (+69,744; +0.237%) | 14,869,827 → 14,897,987 (+28,160; +0.189%) |       571,731,223 |
| Windows x86-64 | 33,600,000 → 33,619,968 (+19,968; +0.059%) |  15,696,749 → 15,699,919 (+3,170; +0.020%) |       461,410,304 |

Changes from the earlier 16-unit full-debug builds to the one-unit full-debug builds were:

| Platform       |              Build and training | Stripped `uv` |  Wheel | Full symbols |
| -------------- | ------------------------------: | ------------: | -----: | -----------: |
| Linux x86-64   |                          -21.7% |        -17.3% | -13.9% |       -15.6% |
| Linux ARM64    |                          -16.4% |        -19.0% | -14.2% |       -16.1% |
| macOS ARM64    |                          -38.4% |        -17.1% | -13.3% |       -18.0% |
| Windows x86-64 | No completed 16-unit full build |             — |      — |            — |

The no-debug pipelines also became faster: 18.6% on Linux x86-64, 16.6% on Linux ARM64, and 17.2% on
macOS. These historical comparisons include variation in runner load, caches, and training
workloads; they do not isolate a causal timing effect or establish repeatability. Full debug's
paired overhead remains higher than the earlier 16-unit line-table and limited observations
(8.8–12.2% on Linux and macOS). Comparing their absolute times with this run would also change the
codegen-unit setting. Full symbol files remain larger than the earlier line-table and limited files.

All four targets passed Rust, AWS-LC, and jitterentropy source lookups, negative lookups with
symbols hidden, SBOM retention, wheel installation, executable hashes, and smoke checks. ELF build
IDs and Mach-O UUIDs matched their companion files. Windows resolved addresses using the PDB
referenced by each executable and passed static CRT checks. macOS signatures also passed
verification, including a local check of the downloaded files. Downloaded executable/profile hashes,
wheel contents, symbol sizes, artifact digests, and benchmark output hashes were verified. Final
`uv` compiler commands explicitly contain `codegen-units=1`; instrumented training inherits the same
Cargo profile environment, although its log does not print individual compiler commands. `uvx`
companion sizes were 2,114,776 bytes on Linux x86-64, 2,281,248 on Linux ARM64, and 2,727,501 on
macOS. Windows `uvx.pdb` was 4,935,680 bytes and `uvw.pdb` was 4,911,104 bytes.

Both Linux targets reported 18 missing-profile warnings per mode, confined to `uvx`. macOS reported
2,367 per mode, including uv dependency functions, compared with 5,888/6,000 in the earlier full
comparison and 5,888/5,883 in the line-table and limited comparisons. All modes reported zero
profile mismatches. Windows reported 19 missing-profile warnings per mode, confined to `uvx` and
`uvw`, matching the earlier Windows comparisons. The smaller macOS warning count does not prove
better training coverage: codegen units change the generated functions and diagnostics. The macOS
profile coverage caveat above still applies.

Twenty cached resolutions per mode produced identical outputs. Median baseline → full times in
milliseconds were:

| Platform       |                 Jupyter |                    Trio |
| -------------- | ----------------------: | ----------------------: |
| Linux x86-64   |   8.579 → 8.605 (+0.3%) |   6.873 → 6.932 (+0.9%) |
| Linux ARM64    | 10.941 → 10.730 (-1.9%) |   8.272 → 8.211 (-0.7%) |
| macOS ARM64    | 10.901 → 11.047 (+1.3%) |   9.062 → 9.203 (+1.6%) |
| Windows x86-64 | 26.043 → 27.116 (+4.1%) | 16.805 → 16.983 (+1.1%) |

These short workloads do not establish overall runtime equivalence. The earlier 16-unit full-debug
Jupyter/Trio symbol-build medians were 8.915/7.123 ms on Linux x86-64, 11.482/8.804 ms on Linux
ARM64, and 13.525/13.449 ms on macOS; comparisons across runs also include runner variation.

Reports, outputs, and provenance are retained separately with each platform's `pgo-*-full-cgu1`
prefix. Verified attempt-1 uploads on October 7, 2026:

- [Linux x86-64 job](https://github.com/astral-sh/uv/actions/runs/37684681738/job/113009336877):
  [artifact 11512143673](https://github.com/astral-sh/uv/actions/runs/37684681738/artifacts/11512143673),
  21:24:19 UTC.
- [Linux ARM64 job](https://github.com/astral-sh/uv/actions/runs/37684681738/job/113009337147):
  [artifact 11513356214](https://github.com/astral-sh/uv/actions/runs/37684681738/artifacts/11513356214),
  21:35:56 UTC.
- [macOS ARM64 job](https://github.com/astral-sh/uv/actions/runs/37684681738/job/113009336560):
  [artifact 11513205740](https://github.com/astral-sh/uv/actions/runs/37684681738/artifacts/11513205740),
  21:32:29 UTC.
- [Windows x86-64 job](https://github.com/astral-sh/uv/actions/runs/37684681738/job/113009336918):
  [artifact 11514037026](https://github.com/astral-sh/uv/actions/runs/37684681738/artifacts/11514037026),
  21:54:35 UTC.

The Windows job ran on `namespace-profile-windows-2022-x86-64-16x32`, reporting 16 logical
processors and 34,359,107,584 bytes of physical memory. Full debug with one codegen unit completed
both the instrumented and final builds with four Cargo jobs. This establishes feasibility on that
runner for this observation; peak process memory was not measured. The 16-unit full-debug run failed
during instrumented compilation, so there is no successful 16-unit full PGO result for a direct
comparison. The cancelled 64 GB run never acquired a runner and contributes no compiler or capacity
measurement.

The Windows comparisons below use each run's own no-debug baseline. They change both debug level and
codegen units; historical differences also include runner and training variation:

| Debug level     | Codegen units | No debug → symbols, combined | `uv` symbols bytes | Stripped `uv.exe` bytes |
| --------------- | ------------: | ---------------------------: | -----------------: | ----------------------: |
| Line tables     |            16 |    30m 31s → 33m 13s (+8.8%) |        150,409,216 |              40,176,640 |
| Limited, first  |            16 |    30m 3s → 40m 30s (+34.8%) |        150,351,872 |              40,152,064 |
| Limited, repeat |            16 |    27m 25s → 33m 8s (+20.9%) |        150,392,832 |              40,156,672 |
| Full            |             1 |   26m 22s → 38m 39s (+46.6%) |        461,410,304 |              33,619,968 |

The new full PDB is approximately 3.07 times the earlier line-table/limited PDBs. The executable is
about 16.3% smaller and its wheel about 13.3% smaller than those builds. Full debug adds only 19,968
bytes to its own no-debug executable and 3,170 bytes to its wheel. Compared with the limited repeat,
the full pipeline took 16.6% longer, while its no-debug baseline took 3.8% less time; it therefore
still has substantial debug overhead despite the smaller shipped binary. The earlier failed-full
run's no-debug baseline took 25m 6s, versus 26m 22s here, so this Windows observation does not show
uniformly faster compilation from one codegen unit.

Windows median baseline/full resolution times were 26.043/27.116 ms for Jupyter (+4.1%, 1.074 ms)
and 16.805/16.983 ms for Trio (+1.1%, 0.178 ms), with identical output hashes. Earlier 16-unit line
tables measured 26.176/26.121 and 16.976/16.957 ms, and limited repeat measured 31.151/32.331 and
21.528/21.299 ms. These small cached workloads and differing runner observations do not establish a
general runtime regression or improvement.

### Debug levels on the current single-codegen-unit release profile

These comparisons use main revision `238d6ba651d13f0dfddab0cc1826f963cdf21711`, including the merged
[single-codegen-unit release configuration](https://github.com/astral-sh/uv/pull/22303), with
experiment commit `eb7920ef2a1c1f62d89a3c68f6f494193f5ca093`. All levels use one codegen unit,
optimization level 3, fat LTO, Rust 1.99.0, LLVM 23.1.1, and Maturin 1.15.0. Each comparison builds
a fresh no-debug baseline and its selected symbols mode, training an independent PGO profile for
each. The source, lockfile, toolchain, PGO corpus, and resolver benchmark inputs match across
levels. These results form a separate set from October 7 because uv's source and dependencies
changed.

Runner profiles and Cargo job counts match the earlier experiment: Linux uses the 16-vCPU Depot
profiles with eight Cargo jobs; macOS uses `namespace-profile-macos-15` with four; Windows uses
`namespace-profile-windows-2022-x86-64-16x32` with four. Production Linux x86-64 now uses the
smaller `depot-ubuntu-latest-4` profile. These native timings do not reproduce production manylinux
CI.

12 of 12 comparisons have completed and been verified. All twelve jobs passed.

Combined instrumented-build, training, and final-build wall times are no debug → symbols, with
overhead against each run's own baseline. Setup, symbol processing, verification, and uploads are
excluded:

| Platform       |               Line tables |                    Limited |                        Full |
| -------------- | ------------------------: | -------------------------: | --------------------------: |
| Linux x86-64   | 17m 16s → 19m 5s (+10.6%) |   20m 4s → 18m 51s (-6.1%) |  17m 11s → 25m 18s (+47.3%) |
| Linux ARM64    |  18m 4s → 19m 29s (+7.9%) |  16m 56s → 18m 20s (+8.3%) |   17m 5s → 24m 50s (+45.4%) |
| macOS ARM64    | 12m 50s → 14m 9s (+10.3%) | 12m 52s → 14m 21s (+11.6%) | 12m 40s → 32m 39s (+157.8%) |
| Windows x86-64 | 23m 57s → 35m 4s (+46.4%) |  23m 16s → 25m 25s (+9.3%) |   22m 48s → 36m 0s (+57.9%) |

The negative Linux x86-64 limited total is a timing anomaly, not evidence of a reliable speedup. Its
no-debug instrumented Cargo build took 13m 24s, compared with 11m 22s for limited and 10m 26s for
the line-table run's no-debug build. The rest of PGO training took about 44s in each limited mode,
so the extra baseline time is primarily compilation rather than corpus downloads or training. The
limited final build was slower than its baseline. The observed total is retained; baselines are
neither pooled nor substituted, and single observations do not establish recurring overhead.

Windows line tables also need a repeat before interpreting their measured +46.4% overhead as a
stable debug-level cost. The symbols final build took 15m 38s, versus 9m 54s for limited. Both
binaries in the line-table job had substantially slower resolver medians: Jupyter measured
62.129/60.613 ms, versus 38.194/38.520 ms for limited and 38.446/38.219 ms for full. This is
consistent with variation in run conditions, but does not identify its cause or establish that line
tables intrinsically cost more than limited debug information. The configured runner profile,
CPU/RAM allocation, optimization settings, benchmark inputs and outputs were checked and matched.

Instrumented compilation and training, using the same no debug → symbols convention:

| Platform       |                Line tables |                    Limited |                       Full |
| -------------- | -------------------------: | -------------------------: | -------------------------: |
| Linux x86-64   |  11m 11s → 12m 13s (+9.3%) |   14m 8s → 12m 8s (-14.1%) |  11m 8s → 15m 43s (+41.1%) |
| Linux ARM64    |  11m 46s → 12m 31s (+6.3%) |   11m 3s → 11m 46s (+6.5%) | 11m 10s → 15m 18s (+37.0%) |
| macOS ARM64    |    8m 17s → 8m 56s (+7.9%) |     8m 16s → 9m 4s (+9.7%) |  8m 7s → 22m 49s (+181.5%) |
| Windows x86-64 | 15m 25s → 19m 26s (+26.1%) | 13m 51s → 15m 31s (+12.0%) |  14m 7s → 22m 19s (+58.2%) |

Final compilation and wheel creation:

| Platform       |               Line tables |                  Limited |                      Full |
| -------------- | ------------------------: | -----------------------: | ------------------------: |
| Linux x86-64   |   6m 5s → 6m 52s (+12.8%) | 5m 56s → 6m 42s (+13.1%) |   6m 3s → 9m 35s (+58.5%) |
| Linux ARM64    |  6m 17s → 6m 58s (+10.9%) | 5m 53s → 6m 34s (+11.7%) |  5m 55s → 9m 33s (+61.3%) |
| macOS ARM64    |  4m 33s → 5m 13s (+14.4%) | 4m 36s → 5m 17s (+15.0%) | 4m 33s → 9m 50s (+115.6%) |
| Windows x86-64 | 8m 32s → 15m 38s (+83.1%) |  9m 24s → 9m 54s (+5.3%) | 8m 41s → 13m 41s (+57.6%) |

Separate `uv` symbol sizes are uncompressed decimal MB (1 MB = 1,000,000 bytes), excluded from
shipped wheels. Reports also retain `uvx` and Windows `uvw` companion sizes:

| Platform       | No debug | Line tables | Limited |  Full |
| -------------- | -------: | ----------: | ------: | ----: |
| Linux x86-64   |        0 |       169.6 |   266.4 | 582.5 |
| Linux ARM64    |        0 |       179.3 |   273.8 | 600.3 |
| macOS ARM64    |        0 |       188.8 |   300.8 | 580.8 |
| Windows x86-64 |        0 |       119.1 |   120.2 | 442.9 |

Stripped `uv` executable sizes in decimal MB, no debug → symbols, with the paired size change:

| Platform       |               Line tables |                   Limited |                      Full |
| -------------- | ------------------------: | ------------------------: | ------------------------: |
| Linux x86-64   | 39.832 → 39.821 (-0.028%) | 39.835 → 39.828 (-0.018%) | 39.823 → 39.623 (-0.502%) |
| Linux ARM64    | 33.462 → 33.481 (+0.057%) | 33.464 → 33.469 (+0.015%) | 33.474 → 33.739 (+0.790%) |
| macOS ARM64    | 29.258 → 29.542 (+0.972%) | 29.258 → 29.542 (+0.973%) | 29.258 → 29.543 (+0.973%) |
| Windows x86-64 | 33.286 → 33.289 (+0.009%) | 33.269 → 33.511 (+0.728%) | 33.272 → 33.538 (+0.799%) |

Processed wheel sizes in decimal MB, with the same convention:

| Platform       |               Line tables |                   Limited |                      Full |
| -------------- | ------------------------: | ------------------------: | ------------------------: |
| Linux x86-64   | 17.509 → 17.489 (-0.112%) | 17.502 → 17.505 (+0.019%) | 17.500 → 17.404 (-0.548%) |
| Linux ARM64    | 16.442 → 16.460 (+0.104%) | 16.450 → 16.465 (+0.090%) | 16.455 → 16.595 (+0.853%) |
| macOS ARM64    | 14.944 → 15.079 (+0.905%) | 14.943 → 15.080 (+0.916%) | 14.945 → 15.078 (+0.890%) |
| Windows x86-64 | 15.773 → 15.772 (-0.009%) | 15.763 → 15.884 (+0.767%) | 15.764 → 15.891 (+0.806%) |

These comparisons measure shipped artifacts after companion symbols have been separated; they do not
guarantee identical executable sizes. Reduced Rust debug levels still enable full native C debug
information through Cargo's boolean `DEBUG` setting in the pinned native build scripts.

Cached resolver benchmark medians below are no debug → symbols in milliseconds. Each mode has twenty
timed samples after two warmups, with alternating order and identical resolved outputs. Where
multiple levels have completed on a platform, their requirements hashes, resolution hashes, Python
version, Rust version, and Maturin version also match. These short workloads do not establish
overall runtime equivalence or a general regression from small timing differences.

Jupyter:

| Platform       |             Line tables |                 Limited |                    Full |
| -------------- | ----------------------: | ----------------------: | ----------------------: |
| Linux x86-64   | 12.882 → 12.977 (+0.7%) | 12.202 → 12.338 (+1.1%) | 13.469 → 13.442 (-0.2%) |
| Linux ARM64    | 10.694 → 10.414 (-2.6%) |   9.914 → 9.833 (-0.8%) | 10.533 → 10.467 (-0.6%) |
| macOS ARM64    | 10.849 → 10.948 (+0.9%) | 11.253 → 11.429 (+1.6%) | 10.912 → 11.085 (+1.6%) |
| Windows x86-64 | 62.129 → 60.613 (-2.4%) | 38.194 → 38.520 (+0.9%) | 38.446 → 38.219 (-0.6%) |

Trio:

| Platform       |             Line tables |                 Limited |                    Full |
| -------------- | ----------------------: | ----------------------: | ----------------------: |
| Linux x86-64   | 10.410 → 10.306 (-1.0%) |  9.804 → 10.014 (+2.1%) | 10.817 → 10.680 (-1.3%) |
| Linux ARM64    |   8.148 → 8.036 (-1.4%) |   7.643 → 7.586 (-0.7%) |   8.156 → 8.092 (-0.8%) |
| macOS ARM64    |   8.986 → 9.133 (+1.6%) |   9.111 → 9.201 (+1.0%) |   9.079 → 9.119 (+0.4%) |
| Windows x86-64 | 36.847 → 38.040 (+3.2%) | 22.545 → 22.520 (-0.1%) | 22.594 → 22.517 (-0.3%) |

Every completed job passed Rust source lookups for its executables, AWS-LC and jitterentropy source
lookups, negative lookups with companion files hidden, SBOM retention, wheel installation, and smoke
checks. Linux build IDs, macOS UUIDs/signatures, and Windows static CRT checks passed where
applicable. Downloaded archive digests, artifact/job provenance, executable/profile hashes, wheel
contents, symbol sizes, and benchmark output hashes were verified. Final compiler invocations were
checked for the requested optimization, debug, CGU, and PGO flags. Instrumented training inherits
the same Cargo profile environment; its logs do not print individual compiler invocations.

All completed modes reported zero profile mismatches. Missing-profile warning counts are no debug →
symbols:

| Platform       | Line tables |     Limited |        Full |
| -------------- | ----------: | ----------: | ----------: |
| Linux x86-64   |     18 → 18 |     18 → 18 |     18 → 18 |
| Linux ARM64    |     18 → 18 |     18 → 18 |     18 → 18 |
| macOS ARM64    | 3565 → 3567 | 3565 → 3555 | 3565 → 3585 |
| Windows x86-64 |     19 → 19 |     19 → 19 |     19 → 19 |

The Linux counts are confined to `uvx` and the Windows counts to `uvx`/`uvw`. macOS includes uv
dependency functions, so its profile coverage caveat still applies. Warning counts alone do not
measure workload coverage. Windows completed jobs report 16 logical CPUs and 34,359,107,584 bytes of
physical RAM; peak process memory was not measured.

Reports, prepared outputs, provenance, raw timings, and normalized comparisons are retained under
`target/debug-symbols-experiment/main-2026-10-09/`. Workflow runs:

- [Line tables](https://github.com/astral-sh/uv/actions/runs/37963882186)
- [Limited](https://github.com/astral-sh/uv/actions/runs/37963887151)
- [Full](https://github.com/astral-sh/uv/actions/runs/37963892027)

### Repeat of the current-main debug-level comparison

All twelve attempt-2 jobs passed at the same experiment commit
`eb7920ef2a1c1f62d89a3c68f6f494193f5ca093` and main revision
`238d6ba651d13f0dfddab0cc1826f963cdf21711`. Source, toolchain, optimization settings, one codegen
unit, runner profiles, Cargo job counts, PGO corpus, and benchmark harness were unchanged. Each job
again built a fresh no-debug baseline first, then its symbols build, with independent PGO training.
GitHub Actions runner/step diagnostic logging was enabled for this round.

The first-round measurements above remain intact. These are two observations per configuration, with
each symbols build compared against its own baseline. The observed spread is not a confidence
interval. With the baseline always first, cache warming remains a potential source of bias. These
native experiment runners also differ from production manylinux CI, including its smaller Linux
x86-64 runner.

Combined build/training overhead, first observation → repeat:

| Platform       |     Line tables |         Limited |              Full |
| -------------- | --------------: | --------------: | ----------------: |
| Linux x86-64   | +10.6% → +11.4% |  -6.1% → +11.4% |   +47.3% → +48.4% |
| Linux ARM64    |   +7.9% → +8.3% |  +8.3% → +10.3% |   +45.4% → +44.3% |
| macOS ARM64    | +10.3% → +10.8% | +11.6% → +10.8% | +157.8% → +155.1% |
| Windows x86-64 | +46.4% → +20.5% |  +9.3% → +21.2% |   +57.9% → +37.3% |

Repeat wall times below are no debug → symbols, including instrumented compilation, PGO training,
and final compilation/wheel creation. Parentheses show the paired overhead. Setup, symbol
processing, verification, and upload time are excluded:

| Platform       |                Line tables |                    Limited |                        Full |
| -------------- | -------------------------: | -------------------------: | --------------------------: |
| Linux x86-64   | 13m 37s → 15m 10s (+11.4%) | 16m 48s → 18m 43s (+11.4%) |  17m 10s → 25m 29s (+48.4%) |
| Linux ARM64    |  18m 24s → 19m 55s (+8.3%) | 17m 54s → 19m 44s (+10.3%) |  17m 32s → 25m 18s (+44.3%) |
| macOS ARM64    |  12m 46s → 14m 8s (+10.8%) | 12m 51s → 14m 14s (+10.8%) | 12m 48s → 32m 38s (+155.1%) |
| Windows x86-64 | 20m 44s → 24m 59s (+20.5%) | 21m 26s → 25m 58s (+21.2%) |   28m 4s → 38m 32s (+37.3%) |

Repeat instrumented compilation and training:

| Platform       |                Line tables |                    Limited |                       Full |
| -------------- | -------------------------: | -------------------------: | -------------------------: |
| Linux x86-64   |   8m 46s → 9m 39s (+10.0%) |  10m 55s → 12m 2s (+10.3%) |  11m 9s → 15m 38s (+40.3%) |
| Linux ARM64    |   12m 0s → 12m 47s (+6.5%) |  11m 38s → 12m 41s (+9.0%) | 11m 23s → 15m 50s (+39.2%) |
| macOS ARM64    |    8m 12s → 8m 56s (+8.9%) |    8m 16s → 8m 59s (+8.7%) | 8m 14s → 22m 49s (+177.2%) |
| Windows x86-64 | 12m 56s → 15m 13s (+17.6%) | 13m 35s → 15m 38s (+15.0%) | 17m 56s → 21m 40s (+20.8%) |

Repeat final compilation and wheel creation:

| Platform       |              Line tables |                   Limited |                      Full |
| -------------- | -----------------------: | ------------------------: | ------------------------: |
| Linux x86-64   | 4m 51s → 5m 31s (+14.0%) |  5m 53s → 6m 41s (+13.5%) |   6m 1s → 9m 51s (+63.5%) |
| Linux ARM64    |  6m 24s → 7m 8s (+11.5%) |   6m 16s → 7m 3s (+12.7%) |   6m 9s → 9m 28s (+53.9%) |
| macOS ARM64    | 4m 34s → 5m 13s (+14.2%) |  4m 35s → 5m 15s (+14.6%) | 4m 33s → 9m 49s (+115.3%) |
| Windows x86-64 | 7m 48s → 9m 46s (+25.2%) | 7m 51s → 10m 21s (+31.8%) | 10m 8s → 16m 52s (+66.5%) |

Linux x86-64 limited's negative first-round overhead did not repeat. Its symbols total changed from
18m 51s to 18m 43s, while the no-debug baseline fell from 20m 4s to 16m 48s. Baseline instrumented
Cargo compilation fell from 13m 24s to 10m 10s; the symbols compilation changed only from 11m 22s to
11m 17s. This supports treating the original negative overhead as an anomalous baseline observation,
without establishing its underlying cause. In both rounds, the baseline logged its first compilation
within two seconds of starting the Cargo stage, with no crate-download messages during that stage.
The delay occurred after compilation had begun, rather than in a long initial dependency download.

Windows line tables changed from 23m 57s → 35m 4s (+46.4%) to 20m 44s → 24m 59s (+20.5%). Its final
symbols stage fell from 15m 38s to 9m 46s. Line-table and limited costs were close in the repeat
(+20.5% and +21.2%), although limited had measured +9.3% initially. The line-table job's Jupyter
baseline/symbol medians also fell from 62.129/60.613 ms to 40.328/40.878 ms. The large first-round
gap between the reduced debug levels did not recur.

Windows full illustrates why the percentage alone is insufficient: overhead fell from +57.9% to
+37.3%, but the symbols build became slower, from 36m 0s to 38m 32s. Its baseline grew even more,
from 22m 48s to 28m 4s. Both raw times and each run's paired overhead are retained. The three
Windows repeat baselines were checked for configuration differences: all use `debug=none`, one
codegen unit, four Cargo jobs, the same AWS-LC settings and optimization flags, and MSVC
14.44.35207. Their instrumented Cargo stages began compiling within 8–11 seconds and logged no crate
downloads. The full job's baseline instrumented Cargo build took 16m 51s, versus 12m 17s for the
line-table job and 12m 56s for limited. This locates the extra time inside the build, but does not
distinguish slower available compute from scheduling, I/O, or compiler-work variation.

Separate `uv` symbol sizes in the repeat are uncompressed decimal MB (1 MB = 1,000,000 bytes). They
remain excluded from shipped wheels; no-debug companion size is zero. All twelve companion sizes
changed by less than 0.1% from their first-round counterparts:

| Platform       | Line tables | Limited |  Full |
| -------------- | ----------: | ------: | ----: |
| Linux x86-64   |       169.5 |   266.5 | 582.2 |
| Linux ARM64    |       179.3 |   273.7 | 600.0 |
| macOS ARM64    |       188.8 |   300.8 | 581.1 |
| Windows x86-64 |       119.0 |   120.2 | 442.8 |

Repeat stripped `uv` executable sizes, no debug → symbols in decimal MB, with paired size changes:

| Platform       |               Line tables |                   Limited |                      Full |
| -------------- | ------------------------: | ------------------------: | ------------------------: |
| Linux x86-64   | 39.809 → 39.796 (-0.031%) | 39.824 → 39.835 (+0.029%) | 39.840 → 39.612 (-0.572%) |
| Linux ARM64    | 33.481 → 33.481 (+0.001%) | 33.482 → 33.474 (-0.024%) | 33.486 → 33.734 (+0.740%) |
| macOS ARM64    | 29.258 → 29.526 (+0.916%) | 29.258 → 29.542 (+0.973%) | 29.258 → 29.543 (+0.973%) |
| Windows x86-64 | 33.268 → 33.287 (+0.057%) | 33.288 → 33.515 (+0.681%) | 33.267 → 33.537 (+0.813%) |

Repeat processed wheel sizes, using the same convention:

| Platform       |               Line tables |                   Limited |                      Full |
| -------------- | ------------------------: | ------------------------: | ------------------------: |
| Linux x86-64   | 17.499 → 17.493 (-0.031%) | 17.499 → 17.510 (+0.061%) | 17.509 → 17.402 (-0.613%) |
| Linux ARM64    | 16.457 → 16.459 (+0.014%) | 16.456 → 16.461 (+0.034%) | 16.462 → 16.588 (+0.766%) |
| macOS ARM64    | 14.937 → 15.079 (+0.950%) | 14.946 → 15.082 (+0.907%) | 14.947 → 15.078 (+0.876%) |
| Windows x86-64 | 15.762 → 15.772 (+0.059%) | 15.773 → 15.886 (+0.715%) | 15.762 → 15.894 (+0.838%) |

The raw reports retain exact bytes for every executable and `uv`/`uvx`/`uvw` companion. Debug
settings and independently trained PGO profiles can change executable layout; symbol separation does
not guarantee identical shipped bytes. Reduced Rust debug levels still enable full native C debug
information in the configured native build scripts.

Repeat cached resolver medians below are no debug → symbols in milliseconds. All twenty samples per
mode and their output hashes were checked. Source, Rust/Maturin/Python versions, requirements
hashes, and resolution hashes match across levels within each platform and against attempt 1. The
paired comparisons remain useful even where absolute timings differ between runners, but these short
workloads and two observations per configuration do not establish overall runtime equivalence.

Jupyter:

| Platform       |             Line tables |                 Limited |                    Full |
| -------------- | ----------------------: | ----------------------: | ----------------------: |
| Linux x86-64   |   8.338 → 8.381 (+0.5%) | 12.123 → 12.090 (-0.3%) | 14.052 → 13.692 (-2.6%) |
| Linux ARM64    | 10.722 → 10.819 (+0.9%) | 10.579 → 10.499 (-0.8%) | 10.298 → 10.423 (+1.2%) |
| macOS ARM64    | 10.770 → 10.805 (+0.3%) | 10.848 → 10.946 (+0.9%) | 10.950 → 11.006 (+0.5%) |
| Windows x86-64 | 40.328 → 40.878 (+1.4%) | 43.338 → 43.698 (+0.8%) | 39.644 → 39.942 (+0.8%) |

Trio:

| Platform       |             Line tables |                 Limited |                    Full |
| -------------- | ----------------------: | ----------------------: | ----------------------: |
| Linux x86-64   |   6.764 → 6.727 (-0.6%) |   9.837 → 9.781 (-0.6%) | 11.188 → 11.064 (-1.1%) |
| Linux ARM64    |   8.288 → 8.205 (-1.0%) |   8.151 → 8.128 (-0.3%) |   7.778 → 7.964 (+2.4%) |
| macOS ARM64    |   9.034 → 8.975 (-0.7%) |   8.912 → 8.961 (+0.5%) |   8.992 → 9.050 (+0.6%) |
| Windows x86-64 | 24.958 → 24.554 (-1.6%) | 26.096 → 26.157 (+0.2%) | 24.208 → 23.835 (-1.5%) |

Every repeat passed Rust/AWS-LC/jitterentropy source lookup, negative lookup with companion files
hidden, SBOM retention, wheel installation, and smoke checks. Final compiler flags, archive/job
provenance, executable/profile hashes, wheel contents, and symbol sizes were verified. macOS
signatures and Windows static CRT checks passed. Windows again reported 16 logical CPUs and
34,359,107,584 bytes of physical RAM. The training environment retained the selected debug/CGU
settings; training logs do not expose every compiler invocation.

PGO diagnostics matched the corresponding first-round counts exactly, with zero mismatches: 18
missing-profile warnings per mode on Linux, 19 on Windows, and macOS baseline 3,565 with symbols
3,567/3,555/3,585 for line tables/limited/full. Linux and Windows warnings are confined to the
launchers; the macOS coverage caveat remains.

All three attempt-2 workflow log archives were retained, including twelve nested per-job diagnostic
ZIPs with a runner log and a worker log each. Inspection found execution/coordination details but no
sampled CPU utilization, paging, peak-process-memory, or I/O-pressure measurements that explain the
timing variation. Generic resource terms appeared in embedded workflow definitions. The logs
therefore do not establish CPU contention, paging, cache state, or another specific root cause.
Diagnostic logging was added only in the repeat, and baseline-first order was unchanged; neither
round should be discarded or treated as a definitive recurring CI cost.

Repeat evidence is retained under `target/debug-symbols-experiment/main-2026-10-09-repeat/`,
separately from the original `main-2026-10-09/` directory. Artifact IDs, creation times, job IDs,
upload logs, and archive digests distinguish attempt 2 despite duplicate artifact names and the
identical source commit. Workflow logs for both observations:

- Line tables: [first](https://github.com/astral-sh/uv/actions/runs/37963882186/attempts/1),
  [repeat](https://github.com/astral-sh/uv/actions/runs/37963882186/attempts/2).
- Limited: [first](https://github.com/astral-sh/uv/actions/runs/37963887151/attempts/1),
  [repeat](https://github.com/astral-sh/uv/actions/runs/37963887151/attempts/2).
- Full: [first](https://github.com/astral-sh/uv/actions/runs/37963892027/attempts/1),
  [repeat](https://github.com/astral-sh/uv/actions/runs/37963892027/attempts/2).
