# /// script
# requires-python = ">=3.12"
# dependencies = []
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Create uv's macOS and Windows release archives from signed executables."""

import argparse
import hashlib
import tarfile
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile

MACOS_TARGET = "aarch64-apple-darwin"
WINDOWS_TARGET = "x86_64-pc-windows-msvc"


def archive_macos(signed: Path, destination: Path) -> None:
    """Archive uv and uvx under the macOS release directory, with executable access."""
    directory = f"uv-{MACOS_TARGET}"
    with tarfile.open(destination, "w:gz") as archive:
        root = tarfile.TarInfo(directory)
        root.type = tarfile.DIRTYPE
        root.mode = 0o755
        archive.addfile(root)

        for binary in ("uv", "uvx"):
            path = signed / binary
            member = archive.gettarinfo(str(path), arcname=f"{directory}/{binary}")
            member.mode = 0o755
            with path.open("rb") as source:
                archive.addfile(member, source)


def archive_windows(signed: Path, destination: Path) -> None:
    """Archive uv, uvx, and uvw at the root of the Windows release ZIP."""
    with ZipFile(destination, "w", compression=ZIP_DEFLATED) as archive:
        for binary in ("uv.exe", "uvx.exe", "uvw.exe"):
            archive.write(signed / binary, arcname=binary)


def write_checksum(archive: Path) -> None:
    """Write the checksum sidecar, referring to the archive by its filename."""
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_name(f"{archive.name}.sha256").write_text(
        f"{digest}  {archive.name}\n", encoding="utf-8"
    )


def main() -> None:
    """Build the two release archives alongside their repaired wheels."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("signed", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()

    macos = args.output / MACOS_TARGET / f"uv-{MACOS_TARGET}.tar.gz"
    windows = args.output / WINDOWS_TARGET / f"uv-{WINDOWS_TARGET}.zip"
    macos.parent.mkdir(parents=True, exist_ok=True)
    windows.parent.mkdir(parents=True, exist_ok=True)

    # uv-build is only distributed in wheels.
    archive_macos(args.signed / MACOS_TARGET, macos)
    archive_windows(args.signed / WINDOWS_TARGET, windows)
    for archive in (macos, windows):
        write_checksum(archive)


if __name__ == "__main__":
    main()
