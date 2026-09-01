# Python shim

This crate produces the Python launchers embedded by `uv-trampoline-builder`. It is excluded from
the workspace: normal builds and release artifacts do not build or ship a separate `uv-python`
executable. `uv python install` extracts the embedded launcher under each Python name.

The generic `python` and `python3` names use normal `uv python find` discovery, including
`.python-version` files and project `requires-python` constraints. Minor-version and variant names
request that CPython version explicitly. A leading `+<request>` overrides both the executable name
and project selection. The shim selects an interpreter; it does not activate the environment.

## Regenerating the launchers

Commit source changes together with the assets in `crates/uv-trampoline-builder/python-shims`.
Regeneration uses clean builds, locked dependencies, and the pinned Rust toolchain. There are two
build groups, covering every target in `scripts/build-python-shims.py`:

- Linux, Windows, and FreeBSD use the pinned `linux/amd64` container. It fixes the source paths,
  compiler/linker tools, Windows SDK/CRT, Zig, and FreeBSD sysroot. Downloads for Zig and the
  FreeBSD sysroot are checked against their SHA-256 digests.
- macOS uses SDK 26.5 (Xcode 26.5 in CI) and Rust's bundled Mach-O linker. The linker gets a stable
  output name so Cargo's path-dependent filename does not affect the UUID. UUIDs and the ARM64
  ad-hoc signature remain intact; removing the UUID prevents launch on current macOS.

From the repository root, regenerate the non-macOS assets with Docker or Podman:

```sh
scripts/build-python-shims.sh
```

On macOS, install Python 3.11+ and the pinned Rust toolchain's `llvm-tools` component and both macOS
Rust targets. Then regenerate the macOS assets using a new, empty work directory:

```sh
python3 scripts/build-python-shims.py \
  --group macos --work-dir "$HOME/code/tmp/python-shims-macos"
```

Use `--check` to compare without changing committed assets:

```sh
scripts/build-python-shims.sh --check
python3 scripts/build-python-shims.py \
  --group macos --work-dir "$HOME/code/tmp/python-shims-macos-check" --check
```

Checks fail on different bytes or an incomplete/unexpected asset catalog. The Python driver rejects
non-empty work directories to prevent a cached executable from hiding a build problem. Compiler
flags and profile overrides from the caller's environment are ignored. ELF linker comments and
Windows PE timestamps/debug identifiers are normalized using the existing trampoline utility for PE
files. uv crate versions and source paths are normalized so unrelated release-version bumps do not
change the launchers.

The dedicated CI workflow runs both build groups when launcher sources, dependencies, assets,
regeneration tools, or relevant CI configuration change. It also checks that the separately locked
`windows` dependency agrees with the main workspace. The ordinary Python integration tests exercise
the committed assets; the byte comparisons ensure those are also the assets produced by the current
source. macOS integration tests are enabled when these inputs change.
