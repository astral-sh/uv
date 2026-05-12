"""Vendor select modules from `pypa/packaging`.

This script clones `pypa/packaging`, checks out a specific commit, copies the
vendored files into `crates/uv-python/python/packaging`, applies all
`*.patch` files in that directory, and regenerates the README.

Example:
    uv run ./scripts/vendor-packaging.py cc938f984bbbe43c5734b9656c9837ab3a28191f
"""

# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///

from __future__ import annotations

import argparse
import shutil
import subprocess
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
PACKAGING_DIR = REPO_ROOT / "crates" / "uv-python" / "python" / "packaging"
README_PATH = PACKAGING_DIR / "README.md"
UPSTREAM_REPOSITORY = "https://github.com/pypa/packaging.git"
UPSTREAM_SOURCE_DIR = "src"

VENDORED_FILES = (
    "__init__.py",
    "_elffile.py",
    "_manylinux.py",
    "_musllinux.py",
)

LICENSE_FILES = (
    "LICENSE.APACHE",
    "LICENSE.BSD",
)


def run(command: list[str], *, cwd: Path) -> None:
    subprocess.run(command, cwd=cwd, check=True)


def capture(command: list[str], *, cwd: Path) -> str:
    result = subprocess.run(
        command, cwd=cwd, check=True, capture_output=True, text=True
    )
    return result.stdout.strip()


def copy_vendored_files(*, upstream_root: Path, destination_root: Path) -> None:
    upstream_packaging_dir = upstream_root / UPSTREAM_SOURCE_DIR / "packaging"

    for file_name in VENDORED_FILES:
        source = upstream_packaging_dir / file_name
        destination = destination_root / file_name
        shutil.copy2(source, destination)

    for file_name in LICENSE_FILES:
        source = upstream_root / file_name
        destination = destination_root / file_name
        shutil.copy2(source, destination)


def install_staged_files(*, staging_root: Path) -> None:
    for file_name in VENDORED_FILES:
        shutil.copy2(staging_root / file_name, PACKAGING_DIR / file_name)

    for file_name in LICENSE_FILES:
        shutil.copy2(staging_root / file_name, PACKAGING_DIR / file_name)


def collect_patch_files() -> list[Path]:
    return sorted(PACKAGING_DIR.glob("*.patch"))


def write_readme(*, commit: str, patch_files: list[Path]) -> None:
    patch_lines = "\n".join(
        f"- [{patch_path.name}](./{patch_path.name})" for patch_path in patch_files
    )
    content = f"""# `pypa/packaging`

This directory contains vendored [pypa/packaging](https://github.com/pypa/packaging) modules as of
[{commit}](https://github.com/pypa/packaging/tree/{commit}/src/packaging).

The files are licensed under BSD-2-Clause OR Apache-2.0.

## Patches

The following patches have been applied:

{patch_lines}
"""
    README_PATH.write_text(content)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("commit", help="The pypa/packaging commit to vendor")
    parser.add_argument(
        "--repository",
        default=UPSTREAM_REPOSITORY,
        help=f"Git repository URL (default: {UPSTREAM_REPOSITORY})",
    )
    args = parser.parse_args()

    patch_files = collect_patch_files()
    if not patch_files:
        raise FileNotFoundError(f"No patch files found in: {PACKAGING_DIR}")

    with tempfile.TemporaryDirectory(prefix="uv-vendor-packaging-") as temp_dir:
        temp_path = Path(temp_dir)
        upstream_root = temp_path / "packaging"

        print(f"Cloning {args.repository}")
        run(
            ["git", "clone", "--quiet", args.repository, str(upstream_root)],
            cwd=temp_path,
        )

        print(f"Checking out {args.commit}")
        run(
            [
                "git",
                "-c",
                "advice.detachedHead=false",
                "checkout",
                "--quiet",
                args.commit,
            ],
            cwd=upstream_root,
        )

        resolved_commit = capture(["git", "rev-parse", "HEAD"], cwd=upstream_root)

        staging_dir = temp_path / "staging"
        staging_dir.mkdir()

        print("Copying vendored files into a staging directory")
        copy_vendored_files(upstream_root=upstream_root, destination_root=staging_dir)

        for patch_path in patch_files:
            print(f"Applying patch: {patch_path}")
            try:
                run(
                    ["git", "apply", "-p5", "--directory=staging", str(patch_path)],
                    cwd=temp_path,
                )
            except subprocess.CalledProcessError as error:
                raise RuntimeError(
                    f"Failed to apply {patch_path.name}. "
                    f"The patch likely needs to be updated for commit {resolved_commit}."
                ) from error

        print(f"Installing vendored files into {PACKAGING_DIR}")
        install_staged_files(staging_root=staging_dir)

    print(f"Regenerating {README_PATH}")
    write_readme(commit=resolved_commit, patch_files=patch_files)

    print("Done.")


if __name__ == "__main__":
    main()
