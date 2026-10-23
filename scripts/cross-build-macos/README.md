# Cross-building macOS binaries on Linux

`build.sh` cross-compiles `uv` for macOS (`aarch64` or `x86_64`) from a Linux host using Zig as the
C cross-compiler and `clang`+`lld` as the Rust linker.

## Prerequisites

```bash
sudo pacman -S clang lld zig
rustup target add aarch64-apple-darwin x86_64-apple-darwin
```

## Usage

```bash
./scripts/cross-build-macos/build.sh                    # aarch64, debug
./scripts/cross-build-macos/build.sh x86_64             # x86_64,  debug
./scripts/cross-build-macos/build.sh aarch64 --release  # aarch64, release
```

The output binary is at `target/<triple>/{debug,release}/uv`.

## What is vendored and why

### `tbd/*.tbd` — macOS framework/library stubs

These are minimal [TAPI text-based dylib][tbd] files that list the symbols exported by macOS system
frameworks and libraries. The linker needs them to resolve `-framework CoreFoundation`, `-lobjc`,
etc. without having a real macOS SDK present.

| File                         | Provides symbols for                        |
| ---------------------------- | ------------------------------------------- |
| `libCoreFoundation.tbd`      | `CFRelease`, `CFArrayGetCount`, …           |
| `libFoundation.tbd`          | `NSLog`, `OBJC_CLASS$_NSObject`, …          |
| `libSecurity.tbd`            | `SecCertificateCopyData`, `SecKeychain*`, … |
| `libSystemConfiguration.tbd` | `SCDynamicStoreCopyProxies`, `kSCProp*`, …  |
| `libobjc.tbd`                | `objc_msgSend`, `sel_registerName`, …       |
| `libunwind.tbd`              | `_Unwind_Resume`, `_Unwind_GetIP`, …        |
| `libiconv.tbd`               | `iconv`, `iconv_open`, `iconv_close`        |

These only declare symbol names — they contain no Apple code. The actual implementations are
resolved at runtime on macOS from the system frameworks.

`libSystem.tbd` is symlinked from Zig's bundled copy at build time, which covers libc, libm, and
other `/usr/lib/system/` sub-libraries. The file is either [APSL-licensed][apsl] or uncopyrightable
(it only contains symbol names and metadata).

[apsl]: https://opensource.apple.com/license/apsl/

### `darwin-headers/sys/syscall.h` — jemalloc build stub

jemalloc unconditionally `#include`s `<sys/syscall.h>`, which only exists in the macOS SDK (Xcode).
jemalloc only uses the Linux-specific `SYS_*` constants from that header, so an empty stub is
sufficient for cross-compilation.

### `SDKSettings.json` — fake SDK metadata

Placed in `SDKROOT` so that `rustc`'s macOS SDK detection considers our stub directory a valid SDK
root.

[tbd]: https://github.com/apple-oss-distributions/tapi
