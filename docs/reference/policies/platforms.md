# Platform support

uv has Tier 1 support for the following platforms:

- macOS (Apple Silicon)
- macOS (x86_64)
- Linux (x86_64)
- Windows (x86_64)

uv is continuously built, tested, and developed against its Tier 1 platforms. Inspired by the Rust
project, Tier 1 can be thought of as
["guaranteed to work"](https://doc.rust-lang.org/beta/rustc/platform-support.html).

uv has Tier 2 support
(["guaranteed to build"](https://doc.rust-lang.org/beta/rustc/platform-support.html)) for the
following platforms:

- Linux (PPC64)
- Linux (PPC64LE)
- Linux (RISC-V64)
- Linux (aarch64)
- Linux (armv7)
- Linux (i686)
- Linux (s390x)
- Windows (arm64)

uv ships pre-built wheels to [PyPI](https://pypi.org/project/uv/) for its Tier 1 and Tier 2
platforms. However, while Tier 2 platforms are continuously built, they are not continuously tested
or developed against, and so stability may vary in practice.

Beyond the Tier 1 and Tier 2 platforms, uv is known to build on i686 Windows, and known _not_ to
build on aarch64 Windows, but does not consider either platform to be supported at this time. The
minimum supported Windows versions are Windows 10 and Windows Server 2016, following
[Rust's own Tier 1 support](https://blog.rust-lang.org/2024/02/26/Windows-7.html).

## macOS versions

uv supports macOS 13+ (Ventura).

uv is known to work on macOS 12, but requires installation of a `realpath` executable.

## Python support

uv supports and is tested against the following Python versions:

- 3.8
- 3.9
- 3.10
- 3.11
- 3.12
- 3.13
- 3.14

uv has Tier 1 support for the following Python implementations:

- CPython

As with platforms, Tier 1 support can be thought of "guaranteed to work". uv supports managed
installations of these implementations, and the builds are maintained by Astral.

uv has Tier 2 support for:

- PyPy
- GraalPy

uv is "expected to work" with these implementations. uv also supports managed installations of these
Python implementations, but the builds are not maintained by Astral.

uv has Tier 3 support for:

- Pyston
- Pyodide

uv "should work" with these implementations, but stability may vary.
