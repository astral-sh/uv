#!/usr/bin/env -S uv run --script
#
# /// script
# requires-python = ">=3.14"
# dependencies = [
#     "blake3>=1.0.9",
# ]
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///

"""Independent Python implementation of ``uv_extract::dirhash``.

The Rust ``test_vectors_json`` test case exercises this implementation in CI.
"""

import sys
from pathlib import Path

from blake3 import blake3


def dirhash(path: Path) -> bytes:
    """Compute the dirhash of a file or directory tree."""
    # The dirhash of a file is the regular BLAKE3 hash of its bytes.
    if not path.is_dir():
        return blake3(path.read_bytes()).digest()

    # The dirhash of a directory is the `blake3::derive_key` of its sorted
    # items, with the context string "directory". The name of each item is
    # encoded in UTF-8 with a 0xff terminator, and the value of each item is
    # its 32-byte dirhash (recursive).
    hasher = blake3(derive_key_context="directory")
    for child in sorted(path.iterdir(), key=lambda child: child.name):
        hasher.update(child.name.encode("utf8"))
        hasher.update(b"\xff")
        hasher.update(dirhash(child))  # Recurse!
    return hasher.digest()


def main() -> None:
    for arg in sys.argv[1:]:
        print(f"{dirhash(Path(arg)).hex()}  {arg}")


if __name__ == "__main__":
    main()
