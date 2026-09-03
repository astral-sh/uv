# /// script
# requires-python = ">=3.12"
# dependencies = []
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Verify the signed executables in a macOS GitHub release archive.

Use `extract-github-release-binaries.py` to check the archive's contents and
checksum and extract its executables. Delegate byte and signature checks to
`verify-release-binaries-macos.py`.
"""

import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

BINARIES = ("uv", "uvx")


def verify_archive(signed: Path, archive: Path) -> None:
    """Extract the GitHub archive and invoke the shared macOS executable verifier."""
    with tempfile.TemporaryDirectory() as temporary:
        archive_binaries = Path(temporary)
        subprocess.run(
            [
                sys.executable,
                Path(__file__).with_name("extract-github-release-binaries.py"),
                "--output",
                archive_binaries,
                archive,
            ],
            check=True,
        )
        subprocess.run(
            [
                "uv",
                "run",
                Path(__file__).with_name("verify-release-binaries-macos.py"),
                signed,
                archive_binaries,
                *BINARIES,
            ],
            check=True,
        )


def main() -> None:
    """Verify a GitHub release archive against the signer's output."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("signed", type=Path)
    parser.add_argument("archive", type=Path)
    args = parser.parse_args()

    try:
        verify_archive(args.signed, args.archive)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
