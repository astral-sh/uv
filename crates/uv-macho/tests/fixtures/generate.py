# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///

"""Regenerate the Mach-O fixtures with Apple's command-line tools on macOS.

Run `uv run crates/uv-macho/tests/fixtures/generate.py` from the repository root.
The dylibs export `uv_macho_fixture`, which returns 42, and contain no third-party code.

The Intel fixture targets macOS 10.9 to exercise SHA-1 and SHA-256 CodeDirectories.
Both ARM64 fixtures embed an Info.plist. The signed fixture also contains requirements,
XML and DER entitlements, and hardened-runtime metadata. Compiler and SDK versions
can change the binary layout.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path


def main() -> None:
    if sys.platform != "darwin":
        sys.exit("Fixture generation requires macOS and Apple's command-line tools")

    directory = Path(__file__).resolve().parent
    for architecture, minimum_version in [("arm64", "11.0"), ("x86_64", "10.9")]:
        command = [
            "xcrun",
            "clang",
            "-dynamiclib",
            "-arch",
            architecture,
            f"-mmacosx-version-min={minimum_version}",
            "-Wl,-install_name,@rpath/libfixture.dylib",
            "-Wl,-headerpad,100",
            "-Wl,-no_adhoc_codesign",
        ]
        if architecture == "arm64":
            command.append("-Wl,-sectcreate,__TEXT,__info_plist,info.plist")
        command.extend(["dylib.c", "-o", f"{architecture}.dylib"])
        subprocess.run(command, cwd=directory, check=True)

    shutil.copyfile(directory / "arm64.dylib", directory / "signed-arm64.dylib")
    subprocess.run(
        [
            "codesign",
            "--force",
            "--sign",
            "-",
            "--identifier",
            "org.astral.uv.fixture",
            "--options",
            "runtime",
            "--force-library-entitlements",
            "--entitlements",
            "entitlements.plist",
            "--requirements",
            '=designated => identifier "org.astral.uv.fixture"',
            "signed-arm64.dylib",
        ],
        cwd=directory,
        check=True,
    )


if __name__ == "__main__":
    main()
