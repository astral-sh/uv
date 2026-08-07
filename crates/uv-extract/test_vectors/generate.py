#!/usr/bin/env -S uv run --script
#
# /// script
# requires-python = ">=3.14"
# dependencies = [
#     "blake3>=1.0.9",
# ]
# ///

# The output of this script is checked in as "test_vectors.json" in this
# directory: `./generate.py > test_vectors.json`. If you change this script,
# rerun that. We test for mismatches in CI.

from blake3 import blake3
import json

type Dir = dict[str, FileOrDir]
type FileOrDir = str | Dir

# Some interfaces, like `dirhash_path`, can handle both filepaths and directory
# paths. Other interfaces, like `DirhashTree`, expect to represent a directory.
# Most archive formats work similarly, where the assumption is that their root
# is a directory and not the recursive base case of "just the bytes of a
# nameless file". (Apparently the NAR format from Nix is a rare exception, but
# certainly Tar and Zip work this way.) To avoid overcomplicating the tests
# that read this list of vectors, don't include any cases that are "just the
# bytes of a nameless file". The dirhash of a file is its ordinary BLAKE3 hash,
# so there's not a lot of dirhash-specific code that needs testing in these
# cases anyway.
CASES: list[Dir] = [
    # an empty dir
    {},
    # a non-empty dir
    {"a": "hello"},
    # Three files. Note that this script's output preserves the order of these
    # keys, and they're deliberately arranged in non-alphabetical order here to
    # test that the caller sorts them.
    {"b": "world", "a": "hello", "c": "!"},
    # a nested empty dir
    {"a": {"b": {}}},
    # a mixed hierarchy, again in non-alphabetical order
    {"b": {"c": "world", "!": {}}, "a": "hello"},
]


def dirhash(input: FileOrDir) -> bytes:
    # The dirhash of a file is the regular BLAKE3 hash of its bytes.
    if isinstance(input, str):
        return blake3(input.encode()).digest()
    # The dirhash of a directory is the `blake3::derive_key` of its sorted
    # items, with the context string "directory". The name of each item is
    # encoded in UTF-8 with a 0xff terminator, and the value of each item is
    # its 32-byte dirhash (recursive).
    assert isinstance(input, dict)
    sorted_items = sorted(input.items())
    dir_hasher = blake3(derive_key_context="directory")
    for name, file_or_dir in sorted_items:
        dir_hasher.update(name.encode("utf8"))
        dir_hasher.update(b"\xff")
        dir_hasher.update(dirhash(file_or_dir))  # Recurse!
    return dir_hasher.digest()


def main():
    output = []
    for case in CASES:
        output.append(
            {
                "input": case,
                "dirhash": dirhash(case).hex(),
            }
        )
    print(json.dumps(output, indent=4))


if __name__ == "__main__":
    main()
