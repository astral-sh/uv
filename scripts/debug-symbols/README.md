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

The experiment retains each merged profile, training log, native build-script logs, and profile
summary under the mode's `pgo/` directory. The report separates training and final build durations,
records profile hashes and function counts, and summarizes missing/mismatched profile warnings from
the final compilation. Full diagnostics remain in `build.log`. Training must produce executed
functions; missing-profile warnings are recorded for review because untrained functions and
launchers can legitimately have no profile data. The usual source lookup, stripping, SBOM, wheel,
and smoke checks run on the final PGO binaries after deleting the instrumented build directories.

Both modes start with empty build and training directories. This requires four optimized builds per
platform, so PGO workflow jobs have a three-hour timeout. Build durations are observations of the
entire experiment, not application performance benchmarks.

### Native uv measurements

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
