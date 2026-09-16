# /// script
# requires-python = ">=3.12"
# dependencies = []
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Verify uv's macOS release wheels against the signing job's output.

Extract the executables with `extract-wheel-binaries.py`, then delegate byte
and signature checks to `verify-release-binaries-macos.py`.
"""

import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

BINARIES = ("uv", "uvx", "uv-build")


def verify_wheels(signed: Path, wheels: Path) -> None:
    """Extract the wheels and invoke the shared macOS executable verifier."""
    with tempfile.TemporaryDirectory() as temporary:
        wheel_binaries = Path(temporary)
        subprocess.run(
            [
                sys.executable,
                Path(__file__).with_name("extract-wheel-binaries.py"),
                "--output",
                wheel_binaries,
                *sorted(wheels.glob("*.whl")),
            ],
            check=True,
        )
        subprocess.run(
            [
                "uv",
                "run",
                Path(__file__).with_name("verify-release-binaries-macos.py"),
                signed,
                wheel_binaries,
                *BINARIES,
            ],
            check=True,
        )


def main() -> None:
    """Verify release wheels against the signer's output."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("signed", type=Path)
    parser.add_argument("wheels", type=Path)
    args = parser.parse_args()

    try:
        verify_wheels(args.signed, args.wheels)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
