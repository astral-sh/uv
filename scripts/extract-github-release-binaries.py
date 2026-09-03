# /// script
# requires-python = ">=3.12"
# dependencies = []
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Extract a uv GitHub release archive after checking its layout and checksum."""

import argparse
import hashlib
import tarfile
from pathlib import Path
from zipfile import ZipFile


def expected_members(path: Path) -> dict[str, str]:
    """Map uv's expected GitHub archive members to their executable names."""
    if path.name.endswith("-pc-windows-msvc.zip"):
        return {name: name for name in ("uv.exe", "uvx.exe", "uvw.exe")}
    if path.name.endswith("-apple-darwin.tar.gz"):
        directory = path.name.removesuffix(".tar.gz")
        return {f"{directory}/{name}": name for name in ("uv", "uvx")}
    raise ValueError(f"Unsupported release archive: {path.name}")


def read_binaries(path: Path) -> dict[str, bytes]:
    """Read every executable, requiring uv's exact GitHub release archive layout."""
    expected = expected_members(path)
    if path.suffix == ".zip":
        with ZipFile(path) as archive:
            if sorted(archive.namelist()) != sorted(expected):
                raise ValueError(f"Unexpected release archive contents: {path.name}")
            return {binary: archive.read(member) for member, binary in expected.items()}

    with tarfile.open(path, "r:gz") as archive:
        directory = path.name.removesuffix(".tar.gz")
        members = [
            member
            for member in archive.getmembers()
            if not (member.name == directory and member.isdir())
        ]
        if sorted(member.name for member in members) != sorted(expected) or any(
            not member.isfile() for member in members
        ):
            raise ValueError(f"Unexpected release archive contents: {path.name}")
        binaries = {}
        for member in members:
            source = archive.extractfile(member)
            if source is None:
                raise ValueError(f"Missing archive executable: {member.name}")
            with source:
                binaries[expected[member.name]] = source.read()
        return binaries


def extract_binaries(path: Path, output: Path) -> None:
    """Check the archive's sidecar and copy its executables into one directory."""
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    checksum = path.with_name(f"{path.name}.sha256").read_text(encoding="utf-8")
    # Windows' sha256sum marks binary-mode input with an asterisk.
    if checksum.replace(" *", "  ").split() != [digest, path.name]:
        raise ValueError(f"Archive checksum differs from input: {path.name}")

    output.mkdir(parents=True, exist_ok=True)
    for name, contents in read_binaries(path).items():
        binary = output / name
        binary.write_bytes(contents)
        binary.chmod(0o755)


def main() -> None:
    """Extract the given GitHub release archive using uv's known layout."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("archive", type=Path)
    args = parser.parse_args()
    extract_binaries(args.archive, args.output)


if __name__ == "__main__":
    main()
