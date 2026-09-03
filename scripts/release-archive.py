# /// script
# requires-python = ">=3.12"
# dependencies = []
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Replace or verify the executables in uv's macOS and Windows release archives.

The original archive must contain the same executables as the unsigned wheels.
Keep its layout and member metadata, replace the executable bytes, and update
the checksum sidecar consumed by cargo-dist.
"""

import argparse
import hashlib
import io
import tarfile
from pathlib import Path
from zipfile import ZipFile


def expected_members(path: Path) -> dict[str, str]:
    """Map the expected archive members to their executable names."""
    if path.name.endswith("-pc-windows-msvc.zip"):
        return {name: name for name in ("uv.exe", "uvx.exe", "uvw.exe")}
    if path.name.endswith("-apple-darwin.tar.gz"):
        directory = path.name.removesuffix(".tar.gz")
        return {f"{directory}/{name}": name for name in ("uv", "uvx")}
    raise ValueError(f"Unsupported release archive: {path.name}")


def read_binaries(path: Path) -> dict[str, bytes]:
    """Read every executable, requiring uv's exact release archive layout."""
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


def checksum_line(path: Path) -> str:
    """Return a checksum sidecar line referring to the archive by filename."""
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    return f"{digest}  {path.name}\n"


def verify_archive(path: Path, binaries: Path) -> None:
    """Require every archive executable and its checksum to match their inputs."""
    for name, contents in read_binaries(path).items():
        if contents != (binaries / name).read_bytes():
            raise ValueError(f"Archive executable differs from input: {name}")
    checksum = path.with_name(f"{path.name}.sha256").read_text(encoding="utf-8")
    # Windows' sha256sum marks binary-mode input with an asterisk.
    if checksum.replace(" *", "  ").split() != checksum_line(path).split():
        raise ValueError(f"Archive checksum differs from input: {path.name}")


def replace_archive(path: Path, unsigned: Path, signed: Path, output: Path) -> None:
    """Replace the original archive's executables and write its new checksum."""
    verify_archive(path, unsigned)
    expected = expected_members(path)
    output.mkdir(parents=True, exist_ok=True)
    destination = output / path.name

    if path.suffix == ".zip":
        with ZipFile(path) as source, ZipFile(destination, "w") as archive:
            for member in source.infolist():
                archive.writestr(
                    member, (signed / expected[member.filename]).read_bytes()
                )
    else:
        with (
            tarfile.open(path, "r:gz") as source,
            tarfile.open(destination, "w:gz") as archive,
        ):
            for member in source.getmembers():
                if member.isdir():
                    archive.addfile(member)
                else:
                    contents = (signed / expected[member.name]).read_bytes()
                    member.size = len(contents)
                    archive.addfile(member, io.BytesIO(contents))

    destination.with_name(f"{destination.name}.sha256").write_text(
        checksum_line(destination), encoding="utf-8"
    )


def main() -> None:
    """Update an archive, or compare it with the signer's output."""
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    replace = commands.add_parser("replace")
    replace.add_argument("archive", type=Path)
    replace.add_argument("unsigned", type=Path)
    replace.add_argument("signed", type=Path)
    replace.add_argument("output", type=Path)
    verify = commands.add_parser("verify")
    verify.add_argument("archive", type=Path)
    verify.add_argument("binaries", type=Path)
    args = parser.parse_args()

    if args.command == "replace":
        replace_archive(args.archive, args.unsigned, args.signed, args.output)
    else:
        verify_archive(args.archive, args.binaries)


if __name__ == "__main__":
    main()
