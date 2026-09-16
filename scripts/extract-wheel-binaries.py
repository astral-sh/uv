# /// script
# requires-python = ">=3.12"
# dependencies = []
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Extract uv and uv_build's .data/scripts wheel members into one directory."""

import argparse
from pathlib import Path, PurePosixPath
from zipfile import ZipFile


def extract_binaries(wheel: Path, destination: Path) -> None:
    """Copy the executables from one of uv's wheels, preserving executable access."""
    with ZipFile(wheel) as archive:
        for name in archive.namelist():
            member = PurePosixPath(name)
            if (
                member.parent.name == "scripts"
                and member.parent.parent.suffix == ".data"
            ):
                binary = destination / member.name
                binary.write_bytes(archive.read(name))
                binary.chmod(0o755)


def main() -> None:
    """Extract the given wheels using uv's known executable layout."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("wheels", type=Path, nargs="+")
    args = parser.parse_args()

    args.output.mkdir(parents=True, exist_ok=True)
    for wheel in args.wheels:
        extract_binaries(wheel, args.output)


if __name__ == "__main__":
    main()
