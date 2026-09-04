#!/usr/bin/env python3
"""Replace executable wheel members with signed binaries and update RECORD."""

import argparse
import base64
import csv
import hashlib
import io
import os
import shutil
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path


def replace_wheel(
    wheel: Path, signed_directory: Path, output_directory: Path, temporary: Path
) -> None:
    staged = temporary / wheel.stem
    output = output_directory / wheel.name

    if output.exists():
        sys.exit(f"Output wheel '{output}' already exists.")

    with zipfile.ZipFile(wheel) as archive:
        names = archive.namelist()
        records = [name for name in names if name.endswith(".dist-info/RECORD")]
        binaries = [
            name
            for name in names
            if ".data/scripts/" in name and not name.endswith("/")
        ]

        if len(records) != 1 or not binaries:
            sys.exit(f"Expected one RECORD and at least one executable in '{wheel}'.")

        record_member = records[0]
        rows = list(csv.reader(io.StringIO(archive.read(record_member).decode())))

    for member in binaries:
        binary_path = staged / member
        signed_binary = signed_directory / binary_path.name

        if not signed_binary.is_file():
            sys.exit(f"Signed executable '{signed_binary}' does not exist.")

        binary_path.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(signed_binary, binary_path)
        binary_path.chmod(0o755)

        digest = hashlib.sha256(binary_path.read_bytes()).digest()
        digest = base64.urlsafe_b64encode(digest).decode().rstrip("=")
        matches = [row for row in rows if row and row[0] == member]

        if len(matches) != 1:
            sys.exit(f"Expected one RECORD entry for '{member}' in '{wheel}'.")

        matches[0][1:] = [f"sha256={digest}", str(binary_path.stat().st_size)]

    record_path = staged / record_member
    record_path.parent.mkdir(parents=True, exist_ok=True)

    with record_path.open("w", newline="") as record:
        csv.writer(record, lineterminator="\n").writerows(rows)

    shutil.copyfile(wheel, output)
    subprocess.run(
        ["zip", "-q", "-X", str(output), *binaries, record_member],
        cwd=staged,
        check=True,
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("signed_directory", type=Path)
    parser.add_argument("output_directory", type=Path)
    parser.add_argument("wheels", type=Path, nargs="+")
    args = parser.parse_args()

    output_directory = args.output_directory.resolve()
    output_directory.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(dir=os.environ["RUNNER_TEMP"]) as temporary:
        for wheel in args.wheels:
            replace_wheel(
                wheel, args.signed_directory, output_directory, Path(temporary)
            )


if __name__ == "__main__":
    main()
