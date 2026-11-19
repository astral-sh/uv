"""Read static build declarations in an offline container, without extracting or executing code."""

import bz2
import collections
import gzip
import hashlib
import io
import json
import sys
import tarfile
import tomllib
import zipfile
from pathlib import Path, PurePosixPath

MAX_UNPACKED = 32 * 1024 * 1024
MAX_METADATA = 512 * 1024


def is_project_file(name):
    path = PurePosixPath(name)
    return (
        len(path.parts) <= 2
        and ".." not in path.parts
        and not path.is_absolute()
        and path.name == "pyproject.toml"
    )


def read_pyproject(path):
    if zipfile.is_zipfile(path):
        with zipfile.ZipFile(path) as archive:
            for member in archive.infolist():
                if is_project_file(member.filename):
                    with archive.open(member) as contents:
                        return contents.read(MAX_METADATA + 1)
        return None

    with path.open("rb") as stream:
        signature = stream.read(3)
    if signature.startswith(b"\x1f\x8b"):
        opener = gzip.open
    elif signature == b"BZh":
        opener = bz2.open
    else:
        raise ValueError("unsupported-format")

    with opener(path, "rb") as stream:
        data = stream.read(MAX_UNPACKED + 1)
    if len(data) > MAX_UNPACKED:
        raise ValueError("unpacked-limit")
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:") as archive:
        for member in archive:
            if member.isreg() and is_project_file(member.name):
                with archive.extractfile(member) as contents:
                    return contents.read(MAX_METADATA + 1)
    return None


def scan(row, directory):
    if row["status"] != "staged":
        return row
    try:
        digest = row["sha256"]
        path = directory / "files" / digest
        with path.open("rb") as source:
            if hashlib.file_digest(source, "sha256").hexdigest() != digest:
                raise ValueError("digest-mismatch")
        data = read_pyproject(path)
        if data is None:
            return row | {"status": "no-pyproject"}
        if len(data) > MAX_METADATA:
            raise ValueError("metadata-limit")
        info = tomllib.loads(data.decode("utf-8-sig"))
        build = info.get("build-system")
        if build is None:
            return row | {"status": "no-build-system"}
        if not isinstance(build, dict):
            raise ValueError("invalid-build-system")
        requires = build.get("requires")
        if not isinstance(requires, list) or not all(
            isinstance(dep, str) for dep in requires
        ):
            raise ValueError("invalid-requires")
        return row | {
            "status": "declared",
            "build": {
                key: build[key]
                for key in ("requires", "build-backend", "backend-path")
                if key in build
            },
        }
    except (
        ValueError,
        RecursionError,
        OSError,
        EOFError,
        tarfile.TarError,
        zipfile.BadZipFile,
    ) as exc:
        return row | {"status": "unreadable", "detail": str(exc)[:200]}


def main():
    directory = Path(sys.argv[1])
    counts = collections.Counter()
    with (directory / "manifest.jsonl").open() as stream:
        for line in stream:
            result = scan(json.loads(line), directory)
            counts[result["status"]] += 1
            print(json.dumps(result))
    print(json.dumps(counts, sort_keys=True), file=sys.stderr)


if __name__ == "__main__":
    main()
