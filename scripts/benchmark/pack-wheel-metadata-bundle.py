#!/usr/bin/env python3
"""Pack cached wheel METADATA files into a single per-package benchmark bundle."""

import re
import struct
import sys
from pathlib import Path


MAGIC = b"uv-wheel-metadata-bundle-v1\0"
URL = re.compile(rb"https?://[^\x00\"\s]+?\.whl(?:\.metadata)?")


def pack(directory: Path) -> None:
    records = []
    for path in sorted(directory.glob("*.msgpack")):
        data = path.read_bytes()
        policy_length = int.from_bytes(data[-8:], "little")
        payload_end = len(data) - policy_length - 8
        if payload_end < 0:
            raise ValueError(f"invalid cache policy length in {path}")
        payload = data[:payload_end]
        policy = data[payload_end:-8]
        match = URL.search(policy)
        if match is None:
            raise ValueError(f"missing wheel request URL in {path}")
        key = path.stem.encode()
        url = match.group()
        records.append((key, url, payload))

    bundle = bytearray(MAGIC)
    bundle.extend(struct.pack("<I", len(records)))
    for key, url, payload in records:
        bundle.extend(struct.pack("<HHI", len(key), len(url), len(payload)))
        bundle.extend(key)
        bundle.extend(url)
        bundle.extend(payload)

    output = directory / "metadata.bundle.msgpack"
    output.write_bytes(bundle)
    print(f"{directory.name}: {len(records)} entries, {len(bundle)} bytes -> {output}")


if __name__ == "__main__":
    for argument in sys.argv[1:]:
        pack(Path(argument))
