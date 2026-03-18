# Platform support

uv has Tier 1 support for the following platforms:

- macOS (Apple Silicon)
- macOS (x86_64)
- Linux (x86_64)
- Windows (x86_64)

uv is continuously built, tested, and developed against its Tier 1 platforms. Inspired by the Rust
project, Tier 1 can be thought of as
["guaranteed to work"](https://doc.rust-lang.org/beta/rustc/platform-support.html#tier-1).

uv has Tier 2 support
(["guaranteed to build"](https://doc.rust-lang.org/beta/rustc/platform-support.html#tier-2-with-host-tools))
for the following platforms:

- Linux (PPC64LE)
- Linux (RISC-V64)
- Linux (aarch64)
- Linux (armv7)
- Linux (i686)
- Linux (s390x)
- Windows (arm64)

uv has Tier 3 support
(["best effort"](https://doc.rust-lang.org/beta/rustc/platform-support.html#tier-3)) for the
following platforms:

- FreeBSD (x86_64)
- Windows (i686)

uv provides official binaries on GitHub and pre-built wheels on [PyPI](https://pypi.org/project/uv/)
for its Tier 1 and Tier 2 platforms.

Tier 2 platforms are continuously built, but the uv test suite is not run on them and stability may
vary in practice.

Tier 3 platforms may not be built or tested, but uv will accept patches to fix bugs.

## Windows versions

The minimum supported Windows versions are Windows 10 and Windows Server 2016, following
[Rust's own Tier 1 support](https://blog.rust-lang.org/2024/02/26/Windows-7.html).

## macOS versions

uv supports macOS 13+ (Ventura).

uv is known to work on macOS 12, but requires installation of a `realpath` executable.
