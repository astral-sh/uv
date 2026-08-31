"""Check that the `windows` crate version matches between workspaces.

The launcher crates are excluded from the main workspace. Verify that their
locked `windows` versions stay in sync with the workspace dependency used by
`uv-windows`.
"""

# /// script
# requires-python = ">=3.12"
# [tool.uv]
# exclude-newer = "P7D"
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
    main_version = get_locked_windows_version(main_lockfile)
    print(f"workspace:       windows {main_version}")
    matches = True
    for crate in ("uv-trampoline", "uv-python-shim"):
        launcher_version = get_locked_windows_version(
            ROOT / "crates" / crate / "Cargo.lock"
        )
        print(f"{crate}: windows {launcher_version}")
        if main_version is None or main_version != launcher_version:
            print(
                f"\n::error::windows crate version mismatch! "
                f"workspace uses {main_version} but {crate} uses {launcher_version}",
                file=sys.stderr,
            )
            matches = False
    if not matches:
        return 1

    print("\nVersions match.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
