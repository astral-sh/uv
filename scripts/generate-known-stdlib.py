# /// script
# requires-python = ">=3.12"
# dependencies = ["stdlibs"]
# ///

"""Generate the normalized standard-library package lookup used by `uv add`."""

from __future__ import annotations

import re
from pathlib import Path

from stdlibs import stdlib_module_names

ROOT = Path(__file__).parent.parent
PATH = ROOT / "crates" / "uv-static" / "src" / "known_stdlib.rs"
VERSIONS = range(7, 16)


def normalize_package_name(module: str) -> str:
    """Normalize a Python module name like a package requirement name."""
    return re.sub(r"[-_.]+", "-", module).lower()


def render_match_arm(version: str, modules: set[str], *, first: bool = False) -> str:
    alternatives = "\n                | ".join(
        f'"{module}"' for module in sorted(modules)
    )
    prefix = "        (\n" if first else "        ) | (\n"
    return f"{prefix}            {version},\n            {alternatives}\n"


def main() -> None:
    modules_by_version: dict[int, set[str]] = {}
    for minor_version in VERSIONS:
        # Private module names cannot be expressed as normalized package requirements.
        modules_by_version[minor_version] = {
            normalize_package_name(module)
            for module in stdlib_module_names(f"3.{minor_version}")
            if module != "__future__" and not module.startswith("_")
        }

    ubiquitous_modules = set.intersection(*modules_by_version.values())

    output = """\
//! Generated with `uv run scripts/generate-known-stdlib.py`.

/// Return whether a normalized package name matches a Python standard-library module.
pub fn is_known_standard_library_package(minor_version: u8, package: &str) -> bool {
    matches!(
        (minor_version, package),
"""
    output += render_match_arm("_", ubiquitous_modules, first=True)
    for minor_version in VERSIONS:
        output += render_match_arm(
            str(minor_version), modules_by_version[minor_version] - ubiquitous_modules
        )
    output += """\
        )
    )
}
"""

    PATH.write_text(output)


if __name__ == "__main__":
    main()
