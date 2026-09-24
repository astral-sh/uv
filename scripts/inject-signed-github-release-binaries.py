# /// script
# requires-python = ">=3.12"
# dependencies = []
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Replace the executables in one of uv's GitHub release archives.

The original archive must contain the same executables as the unsigned wheels.
Keep its layout and member metadata, replace the executable bytes, and write
a matching checksum sidecar.
"""

import argparse
import hashlib
import io
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path, PurePosixPath
from zipfile import ZipFile


def check_unsigned_archive(path: Path, unsigned: Path) -> None:
    """Require the archive to contain the same executables as the unsigned wheels."""
    with tempfile.TemporaryDirectory() as temporary:
        archive_binaries = Path(temporary)
        subprocess.run(
            [
                sys.executable,
                Path(__file__).with_name("extract-github-release-binaries.py"),
                "--output",
                archive_binaries,
                path,
            ],
            check=True,
        )
        for binary in archive_binaries.iterdir():
            if binary.read_bytes() != (unsigned / binary.name).read_bytes():
                raise ValueError(
                    f"Archive executable differs from input: {binary.name}"
                )


def replace_archive(path: Path, unsigned: Path, signed: Path, output: Path) -> None:
    """Replace the original archive's executables and write its new checksum."""
    check_unsigned_archive(path, unsigned)
    output.mkdir(parents=True, exist_ok=True)
    destination = output / path.name

    if path.suffix == ".zip":
        with ZipFile(path) as source, ZipFile(destination, "w") as archive:
            for member in source.infolist():
                archive.writestr(
                    member, (signed / PurePosixPath(member.filename).name).read_bytes()
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
                    contents = (signed / PurePosixPath(member.name).name).read_bytes()
                    member.size = len(contents)
                    archive.addfile(member, io.BytesIO(contents))

    digest = hashlib.sha256(destination.read_bytes()).hexdigest()
    destination.with_name(f"{destination.name}.sha256").write_text(
        f"{digest}  {destination.name}\n", encoding="utf-8"
    )


def main() -> None:
    """Replace an archive's executables with the signer's output."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("unsigned", type=Path)
    parser.add_argument("signed", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    replace_archive(args.archive, args.unsigned, args.signed, args.output)


if __name__ == "__main__":
    main()
