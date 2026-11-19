# /// script
# requires-python = ">=3.11"
# dependencies = []
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Reject new externally sourced Cargo packages in pull requests from forks."""

from __future__ import annotations

import os
import subprocess
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class LockedPackage:
    name: str
    version: str
    source: str
    checksum: str | None


def git(*arguments: str) -> bytes:
    return subprocess.check_output(["git", *arguments])


def packages(lockfile: bytes) -> set[LockedPackage]:
    return {
        LockedPackage(
            name=package["name"],
            version=package["version"],
            source=package["source"],
            checksum=package.get("checksum"),
        )
        for package in tomllib.loads(lockfile.decode()).get("package", [])
        if "source" in package
    }


def main() -> int:
    base, head = sys.argv[1:]
    changes = git(
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--no-renames",
        "--name-status",
        "-z",
        base,
        head,
        "--",
        ":(glob)**/Cargo.lock",
    ).split(b"\0")[:-1]

    failed = False
    for index in range(0, len(changes), 2):
        status = changes[index].decode("ascii")
        lockfile = os.fsdecode(changes[index + 1])

        if status == "D":
            if Path(lockfile).with_name("Cargo.toml").is_file():
                print(f"::error::Cargo workspace is missing its lockfile: {lockfile!r}")
                failed = True
            continue

        before = set() if status == "A" else packages(git("show", f"{base}:{lockfile}"))
        after = packages(git("show", f"{head}:{lockfile}"))
        for package in sorted(
            after - before,
            key=lambda package: (
                package.name or "",
                package.version or "",
                package.source or "",
                package.checksum or "",
            ),
        ):
            print(
                f"::error::{lockfile!r} introduces {package.name!r} {package.version!r} "
                f"(source: {package.source!r}, checksum: {package.checksum!r})."
            )
            failed = True

    if failed:
        print(
            "::error::Cargo.lock changes in pull requests from forks must be submitted "
            f"from a {os.environ['GH_REPOSITORY']} branch."
        )

    return int(failed)


if __name__ == "__main__":
    sys.exit(main())
