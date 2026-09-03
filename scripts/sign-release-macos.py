# /// script
# requires-python = ">=3.12"
# dependencies = ["cryptography==50.0.1"]
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Sign uv's macOS binaries with Azure Key Vault, or verify their packaged copies."""

import argparse
import hashlib
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

from cryptography import x509
from cryptography.hazmat.primitives import hashes

BINARIES = ("uv", "uvx", "uv-build")


def run_signing_command(command: list[str | Path], description: str) -> None:
    """Run a signing command without logging private signing configuration."""
    try:
        subprocess.run(
            command, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
        )
    except subprocess.CalledProcessError:
        raise RuntimeError(f"{description} failed") from None


def verify_sha256(path: Path, expected: str) -> None:
    """Require a downloaded file to match its configured SHA-256 digest."""
    if hashlib.sha256(path.read_bytes()).hexdigest() != expected.lower():
        raise ValueError(f"SHA-256 mismatch: {path.name}")


def certificate_sha256(path: Path) -> str:
    """Return the SHA-256 fingerprint of a PEM certificate's DER encoding."""
    certificate = x509.load_pem_x509_certificate(path.read_bytes())
    return certificate.fingerprint(hashes.SHA256()).hex()


def sign_binaries(unsigned: Path, signed: Path) -> None:
    """Download the pinned signing tools and certificate, then sign uv's binaries."""
    required = (
        "STORAGE_ACCOUNT",
        "STORAGE_CONTAINER",
        "RCODESIGN_BLOB",
        "RCODESIGN_SHA256",
        "PKCS11_BLOB",
        "PKCS11_SHA256",
        "AZURE_KEYVAULT_NAME",
        "AZURE_KEYVAULT_KEY_VERSION",
        "KEY_NAME",
        "CERTIFICATE_SHA256",
    )
    for name in required:
        if not os.environ.get(name):
            raise ValueError(f"Missing signing configuration: {name}")

    with tempfile.TemporaryDirectory() as temporary:
        tools = Path(temporary)
        rcodesign = tools / "rcodesign"
        pkcs11 = tools / "libakv_pkcs11.so"
        certificate = tools / "certificate.pem"

        for name, path in (("RCODESIGN", rcodesign), ("PKCS11", pkcs11)):
            run_signing_command(
                [
                    "az",
                    "storage",
                    "blob",
                    "download",
                    "--auth-mode",
                    "login",
                    "--only-show-errors",
                    "--account-name",
                    os.environ["STORAGE_ACCOUNT"],
                    "--container-name",
                    os.environ["STORAGE_CONTAINER"],
                    "--name",
                    os.environ[f"{name}_BLOB"],
                    "--file",
                    path,
                ],
                f"Downloading {path.name}",
            )
            verify_sha256(path, os.environ[f"{name}_SHA256"])
        rcodesign.chmod(0o755)

        run_signing_command(
            [
                "az",
                "keyvault",
                "certificate",
                "download",
                "--only-show-errors",
                "--vault-name",
                os.environ["AZURE_KEYVAULT_NAME"],
                "--name",
                os.environ["KEY_NAME"],
                "--version",
                os.environ["AZURE_KEYVAULT_KEY_VERSION"],
                "--encoding",
                "PEM",
                "--file",
                certificate,
            ],
            "Downloading the public signing certificate",
        )
        if certificate_sha256(certificate) != os.environ["CERTIFICATE_SHA256"].lower():
            raise ValueError("Signing certificate SHA-256 mismatch")

        signed.mkdir()
        shutil.copyfile(certificate, signed / "certificate.pem")
        for binary in BINARIES:
            run_signing_command(
                [
                    rcodesign,
                    "sign",
                    "--config-file",
                    "/dev/null",
                    "--pkcs11-library",
                    pkcs11,
                    "--pkcs11-certificate-file",
                    certificate,
                    "--pkcs11-key-label",
                    os.environ["KEY_NAME"],
                    "--code-signature-flags",
                    "runtime",
                    "--timestamp-url",
                    "http://timestamp.apple.com/ts",
                    unsigned / binary,
                    signed / binary,
                ],
                f"Signing {binary}",
            )


def verify_binaries(signed: Path, wheel_binaries: Path, archive_binaries: Path) -> None:
    """Check packaged bytes, Apple trust, timestamps, and the signing certificate."""
    for binary in ("uv", "uvx"):
        if (signed / binary).read_bytes() != (archive_binaries / binary).read_bytes():
            raise ValueError(
                f"Archive executable differs from signing output: {binary}"
            )

    expected_certificate = certificate_sha256(signed / "certificate.pem")
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
            verify_sha256(Path(f"{prefix}0"), expected_certificate)


def main() -> None:
    """Sign in the protected job, or verify on a fresh macOS runner."""
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    sign = commands.add_parser("sign", help="Sign with the configured Azure identity")
    sign.add_argument("unsigned", type=Path)
    sign.add_argument("signed", type=Path)
    verify = commands.add_parser("verify", help="Verify packaged binaries on macOS")
    verify.add_argument("signed", type=Path)
    verify.add_argument("wheel_binaries", type=Path)
    verify.add_argument("archive_binaries", type=Path)
    args = parser.parse_args()

    try:
        if args.command == "sign":
            sign_binaries(args.unsigned, args.signed)
        else:
            verify_binaries(args.signed, args.wheel_binaries, args.archive_binaries)
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
