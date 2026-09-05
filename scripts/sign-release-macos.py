# /// script
# requires-python = ">=3.12"
# dependencies = ["cryptography==50.0.1"]
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Sign uv's macOS binaries with Azure Key Vault."""

import argparse
import hashlib
import os
import shutil
import subprocess
import tempfile
from enum import Enum
from pathlib import Path

from cryptography import x509
from cryptography.hazmat.primitives import hashes

BINARIES = ("uv", "uvx", "uv-build")


class SigningComponent(Enum):
    """The components needed to sign uv's macOS executables."""

    RCODESIGN = "RCODESIGN"
    PKCS11 = "PKCS11"

    def path(self, directory: Path) -> Path:
        """Return the component's local path in the download directory."""
        match self:
            case SigningComponent.RCODESIGN:
                return directory / "rcodesign"
            case SigningComponent.PKCS11:
                return directory / "libakv_pkcs11.so"

    def is_executable(self) -> bool:
        """Return whether the downloaded component needs executable permissions."""
        match self:
            case SigningComponent.RCODESIGN:
                return True
            case SigningComponent.PKCS11:
                return False


def run_azure_command(command: list[str | Path], description: str) -> None:
    """Run an Azure download without logging private account or vault details."""
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


def download_signing_component(component: SigningComponent, directory: Path) -> Path:
    """Download a pinned signing component and return its ready-to-use path."""
    path = component.path(directory)
    run_azure_command(
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
            os.environ[f"COMPONENT_{component.value}_BLOB"],
            "--file",
            path,
        ],
        f"Downloading {path.name}",
    )
    verify_sha256(path, os.environ[f"COMPONENT_{component.value}_SHA256"])
    if component.is_executable():
        path.chmod(0o755)
    return path


def certificate_sha256(path: Path) -> str:
    """Return the SHA-256 fingerprint of a PEM certificate's DER encoding."""
    certificate = x509.load_pem_x509_certificate(path.read_bytes())
    return certificate.fingerprint(hashes.SHA256()).hex()


def sign_binaries(unsigned: Path, signed: Path) -> None:
    """Download the pinned signing components and certificate, then sign uv's binaries."""
    required = (
        "STORAGE_ACCOUNT",
        "STORAGE_CONTAINER",
        "COMPONENT_RCODESIGN_BLOB",
        "COMPONENT_RCODESIGN_SHA256",
        "COMPONENT_PKCS11_BLOB",
        "COMPONENT_PKCS11_SHA256",
        "AZURE_KEYVAULT_NAME",
        "AZURE_KEYVAULT_KEY_VERSION",
        "KEY_NAME",
        "CERTIFICATE_SHA256",
    )
    for name in required:
        if not os.environ.get(name):
            raise ValueError(f"Missing signing configuration: {name}")

    with tempfile.TemporaryDirectory() as temporary:
        components = Path(temporary)
        certificate = components / "certificate.pem"

        rcodesign = download_signing_component(SigningComponent.RCODESIGN, components)
        pkcs11 = download_signing_component(SigningComponent.PKCS11, components)

        run_azure_command(
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
            try:
                subprocess.run(
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
                    check=True,
                )
            except subprocess.CalledProcessError:
                raise RuntimeError(f"Signing {binary} failed") from None


def main() -> None:
    """Sign using the Azure identity and settings from the protected release job."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("unsigned", type=Path)
    parser.add_argument("signed", type=Path)
    args = parser.parse_args()

    try:
        sign_binaries(args.unsigned, args.signed)
    except (OSError, ValueError, RuntimeError) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
