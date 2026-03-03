"""Update the Python version constants in the test common module.

This script reads the download-metadata.json file and extracts the latest
patch version for each minor version (3.15, 3.14, 3.13, 3.12, 3.11, 3.10).
It then updates the LATEST_PYTHON_X_Y constants in crates/uv-test/src/lib.rs.

For minor versions with stable releases, it uses the latest stable version.
For minor versions with only prereleases, it uses the latest prerelease.

This is called by the sync-python-releases workflow to keep the test constants
in sync with the latest available Python versions.
"""

# /// script
# requires-python = ">=3.12"
# dependencies = ["packaging"]
# ///

from __future__ import annotations

import json
import re
from pathlib import Path

from packaging.version import Version

SELF_DIR = Path(__file__).parent
ROOT = SELF_DIR.parent


def main() -> None:
    # Read the download metadata
    metadata_path = ROOT / "crates" / "uv-python" / "download-metadata.json"
    with open(metadata_path) as f:
        metadata = json.load(f)

    # Collect all versions per minor, separating stable and prerelease
    stable_versions: dict[str, str] = {}
    prerelease_versions: dict[str, str] = {}

    for info in metadata.values():
        if info["name"] != "cpython":
            continue
        if info.get("variant"):
            continue

        minor = f"3.{info['minor']}"
        prerelease = info.get("prerelease", "")

        version = f"{info['major']}.{info['minor']}.{info['patch']}"
        if prerelease:
            version += prerelease

        if prerelease:
            if minor not in prerelease_versions or Version(version) > Version(
                prerelease_versions[minor]
            ):
                prerelease_versions[minor] = version
        else:
            if minor not in stable_versions or Version(version) > Version(
                stable_versions[minor]
            ):
                stable_versions[minor] = version

    # Use stable if available, otherwise prerelease
    latest_versions: dict[str, str] = {}
    for minor in stable_versions:
        latest_versions[minor] = stable_versions[minor]
    for minor in prerelease_versions:
        if minor not in latest_versions:
            latest_versions[minor] = prerelease_versions[minor]

    # Update the constants in uv-test/src/lib.rs
    lib_path = ROOT / "crates" / "uv-test" / "src" / "lib.rs"
    content = lib_path.read_text()

    # Extract old values first
    old_versions: dict[str, str] = {}
    for minor in ["3.15", "3.14", "3.13", "3.12", "3.11", "3.10"]:
        const_name = f"LATEST_PYTHON_{minor.replace('.', '_')}"
        match = re.search(rf'pub const {const_name}: &str = "([^"]+)";', content)
        if match:
            old_versions[minor] = match.group(1)

    for minor in ["3.15", "3.14", "3.13", "3.12", "3.11", "3.10"]:
        if minor not in latest_versions:
            continue
        const_name = f"LATEST_PYTHON_{minor.replace('.', '_')}"
        old_pattern = rf'pub const {const_name}: &str = "[^"]+";'
        new_value = f'pub const {const_name}: &str = "{latest_versions[minor]}";'
        content = re.sub(old_pattern, new_value, content)

    lib_path.write_text(content)

    updates = []
    for minor in ["3.15", "3.14", "3.13", "3.12", "3.11", "3.10"]:
        if minor not in latest_versions:
            continue
        new_version = latest_versions[minor]
        old_version = old_versions.get(minor)
        if old_version != new_version:
            updates.append(f"  {old_version} -> {new_version}")

    if updates:
        print("Updated Python version constants:")
        for update in updates:
            print(update)
    else:
        print("Python version constants are up to date")


if __name__ == "__main__":
    main()
