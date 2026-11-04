# /// script
# requires-python = ">=3.12"
# dependencies = []
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///

"""Regenerate the Mach-O fixtures with Apple's command-line tools on macOS.

Run `uv run --locked scripts/generate-macho-fixtures.py` from the repository root.
The dylibs export `uv_macho_fixture`, which returns 42, and contain no third-party code.

The ARM64 linker-signed fixture contains a single SHA-256 CodeDirectory.
Compiler and SDK versions can change the binary layout.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


def main() -> None:
    if sys.platform != "darwin":
        sys.exit("Fixture generation requires macOS and Apple's command-line tools")

    directory = (
        Path(__file__).resolve().parent.parent / "crates/uv-macho/tests/fixtures"
    )
    for architecture, minimum_version, name, signing in [
        ("arm64", "11.0", "arm64.dylib", "-no_adhoc_codesign"),
        ("x86_64", "10.9", "x86_64.dylib", "-no_adhoc_codesign"),
        ("arm64", "11.0", "linker-signed-arm64.dylib", "-adhoc_codesign"),
    ]:
        subprocess.run(
            [
                "xcrun",
                "clang",
                "-dynamiclib",
                "-arch",
                architecture,
                f"-mmacosx-version-min={minimum_version}",
                "-Wl,-install_name,@rpath/libfixture.dylib",
                "-Wl,-headerpad,100",
                f"-Wl,{signing}",
                "dylib.c",
                "-o",
                name,
            ],
            cwd=directory,
            check=True,
        )


if __name__ == "__main__":
    main()
