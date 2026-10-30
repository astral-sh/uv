# /// script
# requires-python = ">=3.11"
# dependencies = []
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Carry verified source timestamps forward with Hawk's Cargo target directory."""

from __future__ import annotations

import hashlib
import json
import os
import stat
import subprocess
import time
from pathlib import Path


def main() -> None:
    manifest = Path("target/hawk/source-times.json")
    try:
        previous = json.loads(manifest.read_bytes())
    except (FileNotFoundError, json.JSONDecodeError):
        previous = {}
    current: dict[str, tuple[int, str, int]] = {}
    now = time.time_ns()
    restored = 0

    def record(path: Path, info: os.stat_result, digest: str) -> None:
        nonlocal restored
        saved = previous.get(str(path))
        if saved and saved[:2] == [info.st_mode, digest]:
            mtime = saved[2]
            restored += 1
        else:
            # A changed input must be newer than the artifacts we just restored.
            mtime = now
        os.utime(path, ns=(info.st_atime_ns, mtime), follow_symlinks=False)
        current[str(path)] = (info.st_mode, digest, mtime)

    directories: set[Path] = set()
    tracked = subprocess.check_output(["git", "ls-files", "-z"])
    for name in tracked.split(b"\0")[:-1]:
        path = Path(os.fsdecode(name))
        directories.update(path.parents)
        try:
            info = path.lstat()
        except FileNotFoundError:
            continue
        if stat.S_ISREG(info.st_mode):
            with path.open("rb") as source:
                record(path, info, hashlib.file_digest(source, "sha256").hexdigest())

    # Cargo build scripts can watch whole directories. Include names, modes and
    # contents so additions, removals and renames keep the directory fresh.
    directories.discard(Path("."))
    for path in sorted(directories, key=lambda path: len(path.parts), reverse=True):
        if path.is_symlink() or not path.is_dir():
            continue
        children = sorted(path.iterdir())
        if all(str(child) in current for child in children):
            entries = [(child.name, *current[str(child)][:2]) for child in children]
            digest = hashlib.sha256(json.dumps(entries).encode()).hexdigest()
            record(path, path.stat(), digest)
        else:
            # A symlink or untracked input prevents verification of its ancestors.
            os.utime(path, ns=(path.stat().st_atime_ns, now))

    manifest.parent.mkdir(parents=True, exist_ok=True)
    manifest.write_text(json.dumps(current))
    print(f"Restored {restored} of {len(current)} source timestamps")


if __name__ == "__main__":
    main()
