"""Assemble and verify signed macOS release wheels and their archive."""

import argparse
import hashlib
import subprocess
import sys
import tarfile
from pathlib import Path
from zipfile import ZipFile

from check_uv_wheel_contents import check_uv_wheel

DISTRIBUTIONS = {
    "uv": ("uv", "uvx"),
    "uv_build": ("uv-build",),
}


def verify_signature(binary: Path) -> None:
    subprocess.run(
        ["codesign", "--verify", "--strict", "--verbose=4", str(binary)],
        check=True,
    )
    details = subprocess.run(
        ["codesign", "-dv", "--verbose=4", str(binary)],
        check=True,
        capture_output=True,
        text=True,
    )
    if "Signature=adhoc" in details.stderr:
        sys.exit(f"Expected an identity signature on '{binary}'.")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("target")
    args = parser.parse_args()

    root = args.root
    signed = root / "signed"
    dist = Path("dist")
    wheels = []

    for distribution in DISTRIBUTIONS:
        matches = list((root / "wheels").glob(f"{distribution}-*.whl"))
        if len(matches) != 1:
            sys.exit(f"Expected one {distribution} wheel.")
        wheels.append(matches[0])

    replace_wheel_binaries = Path(__file__).with_name("replace-wheel-binaries.py")
    subprocess.run(
        [
            sys.executable,
            str(replace_wheel_binaries),
            str(signed),
            str(dist / "wheels"),
            *map(str, wheels),
        ],
        check=True,
    )

    verified = root / "verified"
    verified.mkdir()

    for distribution, binaries in DISTRIBUTIONS.items():
        wheel_path = next((dist / "wheels").glob(f"{distribution}-*.whl"))
        check_uv_wheel(wheel_path)

        with ZipFile(wheel_path) as wheel:
            for binary in binaries:
                members = [
                    member
                    for member in wheel.namelist()
                    if member.startswith(f"{distribution}-")
                    and member.endswith(f".data/scripts/{binary}")
                ]
                if len(members) != 1:
                    sys.exit(f"Expected one signed {binary} wheel member.")

                destination = verified / binary
                destination.write_bytes(wheel.read(members[0]))
                destination.chmod(0o755)
                verify_signature(destination)

    archive_name = f"uv-{args.target}"
    archive_path = dist / f"{archive_name}.tar.gz"

    with tarfile.open(archive_path, "w:gz") as archive:
        for binary in DISTRIBUTIONS["uv"]:
            archive.add(signed / binary, arcname=f"{archive_name}/{binary}")

    checksum_path = dist / f"{archive_path.name}.sha256"
    digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
    checksum_path.write_text(f"{digest}  {archive_path.name}\n")

    subprocess.run(
        ["shasum", "-a", "256", "--check", checksum_path.name],
        cwd=dist,
        check=True,
    )

    with tarfile.open(archive_path) as archive:
        for binary in DISTRIBUTIONS["uv"]:
            member = archive.extractfile(f"{archive_name}/{binary}")
            if member is None:
                sys.exit(f"Expected {binary} in '{archive_path}'.")

            destination = verified / binary
            destination.write_bytes(member.read())
            destination.chmod(0o755)
            verify_signature(destination)


if __name__ == "__main__":
    main()
