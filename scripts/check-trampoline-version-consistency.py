"""Check that the `windows` crate version matches between workspaces.

The uv-trampoline crate is excluded from the main workspace (it requires nightly),
so this script verifies that the `windows` crate version is kept in sync by
comparing the locked versions in both Cargo.lock files.
"""

# /// script
# requires-python = ">=3.12"
# ///

from __future__ import annotations

import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).parent.parent


def get_locked_windows_version(lockfile_path: Path) -> str | None:
    """Get the windows crate version from a Cargo.lock file."""
    with open(lockfile_path, "rb") as f:
        lockfile = tomllib.load(f)

    for package in lockfile.get("package", []):
        if package.get("name") == "windows":
            return package.get("version")

    return None


def main() -> int:
    main_lockfile = ROOT / "Cargo.lock"
    trampoline_lockfile = ROOT / "crates" / "uv-trampoline" / "Cargo.lock"

    main_version = get_locked_windows_version(main_lockfile)
    trampoline_version = get_locked_windows_version(trampoline_lockfile)

    print(f"workspace:       windows {main_version}")
    print(f"uv-trampoline:   windows {trampoline_version}")

    if main_version != trampoline_version:
        print(
            f"\n::error::windows crate version mismatch! "
            f"workspace uses {main_version} but uv-trampoline uses {trampoline_version}",
            file=sys.stderr,
        )
        return 1

    print("\nVersions match.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
