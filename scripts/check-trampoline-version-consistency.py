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


def get_locked_windows_versions(lockfile_path: Path) -> list[str]:
    """Get all windows crate versions from a Cargo.lock file."""
    with open(lockfile_path, "rb") as f:
        lockfile = tomllib.load(f)

    versions = []
    for package in lockfile.get("package", []):
        if package.get("name") == "windows":
            if version := package.get("version"):
                versions.append(version)

    return versions


def main() -> int:
    main_lockfile = ROOT / "Cargo.lock"
    trampoline_lockfile = ROOT / "crates" / "uv-trampoline" / "Cargo.lock"

    main_versions = get_locked_windows_versions(main_lockfile)
    trampoline_versions = get_locked_windows_versions(trampoline_lockfile)

    print(f"workspace:       windows {main_versions}")
    print(f"uv-trampoline:   windows {trampoline_versions}")

    # uv-trampoline should have exactly one windows version
    if len(trampoline_versions) != 1:
        print(
            f"\n::error::uv-trampoline should have exactly one windows version, "
            f"found {len(trampoline_versions)}",
            file=sys.stderr,
        )
        return 1

    trampoline_version = trampoline_versions[0]

    # The trampoline's windows version must be present in the main workspace
    if trampoline_version not in main_versions:
        print(
            f"\n::error::windows crate version mismatch! "
            f"workspace has {main_versions} but uv-trampoline uses {trampoline_version}",
            file=sys.stderr,
        )
        return 1

    print("\nVersions match.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
