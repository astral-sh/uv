# /// script
# requires-python = ">=3.12"
# dependencies = []
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Replace the executables in a uv or uv_build release wheel.

Each .data/scripts member must have a same-named file in the signing directory.
Other wheel contents and metadata are preserved, and RECORD is regenerated.
The trusted inputs must fit in memory. Signatures are verified separately.
The output must not exist and is created only after the complete wheel is written.
"""

import argparse
import base64
import copy
import csv
import hashlib
import io
import os
import tempfile
from pathlib import Path
from zipfile import ZipFile


def inject_signed_binaries(wheel: Path, output: Path, signed_binaries: Path) -> None:
    """Replace every executable and write a complete wheel without overwriting."""
    if wheel == output:
        raise ValueError("Input and output wheels must be different")

    completed = io.BytesIO()
    record = io.StringIO(newline="")
    record_writer = csv.writer(record, lineterminator="\n")
    replaced = False

    with (
        ZipFile(io.BytesIO(wheel.read_bytes())) as source,
        ZipFile(completed, "w") as destination,
    ):
        records = [
            member
            for member in source.infolist()
            if member.filename.endswith(".dist-info/RECORD")
        ]
        if len(records) != 1:
            raise ValueError("Expected exactly one RECORD file in the wheel")
        record_member = records[0]
        destination.comment = source.comment

        for member in source.infolist():
            name = member.filename
            if name == record_member.filename:
                continue

            _, scripts, binary = name.partition(".data/scripts/")
            if scripts:
                if not binary or "/" in binary or "\\" in binary:
                    raise ValueError(f"Unexpected executable wheel member: {name}")
                contents = (signed_binaries / binary).read_bytes()
                replaced = True
            else:
                contents = source.read(member)

            destination.writestr(copy.copy(member), contents)
            digest = base64.urlsafe_b64encode(hashlib.sha256(contents).digest())
            record_writer.writerow(
                (name, f"sha256={digest.rstrip(b'=').decode('ascii')}", len(contents))
            )

        if not replaced:
            raise ValueError("Wheel does not contain executable members")
        record_writer.writerow((record_member.filename, "", ""))
        destination.writestr(
            copy.copy(record_member), record.getvalue().encode("utf-8")
        )

    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=output.parent) as temporary:
        staged = Path(temporary) / output.name
        staged.write_bytes(completed.getvalue())
        os.link(staged, output)


def main() -> None:
    """Replace a wheel's executables with the signer's output."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--signed-binaries", type=Path, required=True)
    args = parser.parse_args()
    inject_signed_binaries(args.input, args.output, args.signed_binaries)


if __name__ == "__main__":
    main()
