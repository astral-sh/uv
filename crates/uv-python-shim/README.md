# Python shim

This crate produces the Python launchers embedded by `uv-trampoline-builder`. It is excluded from
the workspace: normal builds, wheels, archives, and container images do not build or ship a separate
`uv-python` executable. `uv python install` writes the embedded launcher under each requested Python
name.

The launcher delegates interpreter selection to `uv python find`, finding `uv` beside itself or on
`PATH`. Its behavior is exercised by `crates/uv/tests/python/python_shim.rs`, including installation
from a standalone copy of `uv`.

## Regenerating the launchers

Commit source changes together with the corresponding files in
`crates/uv-trampoline-builder/python-shims`. Build with Python 3.11+, the Rust toolchain pinned in
`rust-toolchain.toml`, its `llvm-tools` component, and the relevant Rust targets installed with
`rustup target add`.

From the repository root, repeat `--target` to build multiple targets:

```sh
python3 scripts/build-python-shims.py \
  --work-dir "$HOME/code/tmp/uv-python-shims" \
  --target aarch64-apple-darwin \
  --target x86_64-apple-darwin
```

The script uses the locked dependencies and size-optimized `shim` profile. It copies the source to
the work directory, normalizes uv crate versions, remaps source paths, and normalizes Windows PE
metadata with the same utility used for trampolines. ELF linker-version comments and Mach-O UUIDs
are omitted. Use a fresh work directory when changing linker tools or sysroots; Cargo does not track
their contents. `--output-dir` can write a second set for comparison.

Target requirements:

- macOS: a macOS host with Xcode command-line tools. Deployment targets are macOS 11 on ARM64 and
  macOS 10.12 on x86-64.
- Linux musl: Rust's bundled linker and self-contained musl libraries. These static launchers are
  also embedded in glibc builds. The ARMv6 launcher serves ARMv7, and Android uses the corresponding
  static Linux launcher.
- Linux s390x: Zig 0.16.0, targeting glibc 2.17. Rust does not distribute the s390x musl standard
  library.
- Windows: `cargo-xwin` 0.22.0, LLVM's `llvm-mt` (or `mt.exe`) on `PATH`, and `llvm-tools` installed
  with `rustup component add llvm-tools`. The script pins SDK 10.0.22621 and CRT 14.44.17.14,
  matching the trampoline build. All three MSVC architectures use a static CRT.
- FreeBSD x86-64: Clang and a FreeBSD sysroot passed as `--freebsd-sysroot`. The committed launcher
  uses the libraries and headers from
  [the official FreeBSD 14.3 distribution](https://download.freebsd.org/releases/amd64/14.3-RELEASE/base.txz)
  (SHA-256 `e38b5cf756d60086a6c2f736eff19cc7685f7e2313e31d14342fc8df57200a92`).

The macOS SDK, FreeBSD sysroot, Zig, and Windows SDK/CRT are external build inputs. macOS UUIDs and
Windows code layout can also vary with the build directory and compiler-generated location data. The
byte-for-byte CI check currently covers Linux x86-64. The Rust source is checked separately in CI
because this crate is not a workspace member. Runtime integration tests exercise the committed
embedded launcher.
