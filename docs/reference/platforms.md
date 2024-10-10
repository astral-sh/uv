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
- Linux (aarch64)
- Linux (armv7)
- Linux (i686)
- Linux (s390x)

uv ships pre-built wheels to [PyPI](https://pypi.org/project/uv/) for its Tier 1 and Tier 2
platforms. However, while Tier 2 platforms are continuously built, they are not continuously tested
or developed against, and so stability may vary in practice.

Beyond the Tier 1 and Tier 2 platforms, uv is known to build on i686 Windows, and known _not_ to
build on aarch64 Windows, but does not consider either platform to be supported at this time. The
minimum supported Windows versions are Windows 10 and Windows Server 2016, following
[Rust's own Tier 1 support](https://blog.rust-lang.org/2024/02/26/Windows-7.html).

uv supports and is tested against Python 3.8, 3.9, 3.10, 3.11, 3.12, and 3.13.
