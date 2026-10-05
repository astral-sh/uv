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
compiler flags, including frame pointers; the build logs capture the compiler invocation. Raw
Maturin wheel sizes are also recorded separately. This fixture does not exercise uv's dependency
graph, PGO, cross compilation, release signing, or publication. Those require verification with the
actual release artifacts.
