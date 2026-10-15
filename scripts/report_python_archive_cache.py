"""Report managed Python archive cache entries without reading archive contents."""

from __future__ import annotations

import os
import stat
from pathlib import Path


def main() -> None:
    cache_dir = os.environ.get("UV_PYTHON_CACHE_DIR")
    if not cache_dir:
        print("Python archive cache is not configured")
        return

    print(f"Python archive cache: {cache_dir}")
    try:
        entries = sorted(Path(cache_dir).iterdir())
    except FileNotFoundError:
        entries = []

    archives = []
    for entry in entries:
        if entry.name.startswith("."):
            continue
        try:
            metadata = entry.lstat()
        except FileNotFoundError:
            continue
        if stat.S_ISREG(metadata.st_mode):
            archives.append((entry.name, metadata.st_size))

    for filename, size in archives:
        print(f"{size:>12}  {filename}")
    print(
        f"Total archives: {len(archives)}; total bytes: {sum(size for _, size in archives)}"
    )


if __name__ == "__main__":
    main()
