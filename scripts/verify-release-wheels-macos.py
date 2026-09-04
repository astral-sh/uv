# /// script
# requires-python = ">=3.12"
# dependencies = ["cryptography==50.0.1"]
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Verify uv's macOS release wheels against the signing job's output.

Require the wheel executables to contain exactly the signed bytes.
For each wheel executable, `codesign` must accept its Apple-rooted signature and
report a signing timestamp. The embedded leaf certificate must match the
certificate pinned by the signing job.
"""

import argparse
import hashlib
import subprocess
import sys
import tempfile
from pathlib import Path

from cryptography import x509
from cryptography.hazmat.primitives import hashes

BINARIES = ("uv", "uvx", "uv-build")


def verify_wheels(signed: Path, wheels: Path) -> None:
    """Extract the release wheels, then verify their executables."""
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
        verify_binaries(signed, wheel_binaries)


def verify_binaries(signed: Path, wheel_binaries: Path) -> None:
    """Check packaged bytes, Apple trust, timestamps, and the signing certificate."""
    certificate = x509.load_pem_x509_certificate(
        (signed / "certificate.pem").read_bytes()
    )
    expected_certificate = certificate.fingerprint(hashes.SHA256())
    with tempfile.TemporaryDirectory() as temporary:
        for binary in BINARIES:
            path = wheel_binaries / binary
            if (signed / binary).read_bytes() != path.read_bytes():
                raise ValueError(
                    f"Wheel executable differs from signing output: {binary}"
                )
            subprocess.run(
                [
                    "codesign",
                    "--verify",
                    "--strict",
                    "-R",
                    "anchor apple generic",
                    path,
                ],
                check=True,
            )
            details = subprocess.check_output(
                ["codesign", "--display", "--verbose=4", path],
                stderr=subprocess.STDOUT,
                text=True,
            )
            if not any(line.startswith("Timestamp=") for line in details.splitlines()):
                raise ValueError(f"Missing signing timestamp: {binary}")

            prefix = Path(temporary) / f"{binary}-cert-"
            subprocess.run(
                ["codesign", "--display", f"--extract-certificates={prefix}", path],
                check=True,
            )
            actual_certificate = hashlib.sha256(
                Path(f"{prefix}0").read_bytes()
            ).digest()
            if actual_certificate != expected_certificate:
                raise ValueError(f"Signing certificate differs from expected: {binary}")


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
