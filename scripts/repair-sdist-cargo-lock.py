#!/usr/bin/env python3
"""Fix the Cargo.lock inside a maturin-generated sdist tarball.

Maturin copies the full workspace Cargo.lock into the sdist, but the sdist
only contains a subset of workspace crates. This makes `cargo build --locked`
fail because the lock file references packages not present in the sdist.

This script extracts the sdist, runs `cargo update --workspace` to prune
the lockfile to the packages needed by the included crates, checks that no
new third-party packages were added, and repacks the tarball.

See: https://github.com/astral-sh/uv/issues/18824
"""

import argparse
import os
import subprocess
import sys
import tarfile
import tempfile
import tomllib


def third_party_packages(lockfile: str) -> set[tuple[str, str, str, str | None]]:
    with open(lockfile, "rb") as file:
        packages = tomllib.load(file)["package"]
    return {
        (
            package["name"],
            package["version"],
            package["source"],
            package.get("checksum"),
        )
        for package in packages
        if "source" in package
    }


def fix_sdist_lockfile(sdist_path: str) -> None:
    sdist_path = os.path.abspath(sdist_path)
    if not tarfile.is_tarfile(sdist_path):
        print(f"Error: {sdist_path} is not a valid tar file", file=sys.stderr)
        sys.exit(1)

    with tempfile.TemporaryDirectory() as tmpdir:
        # Extract
        with tarfile.open(sdist_path, "r:gz") as tar:
            tar.extractall(tmpdir)

        # Find the extracted directory (e.g., uv_build-0.10.12)
        entries = os.listdir(tmpdir)
        if len(entries) != 1:
            print(
                f"Error: expected one top-level directory, found: {entries}",
                file=sys.stderr,
            )
            sys.exit(1)
        extracted_dir = os.path.join(tmpdir, entries[0])
        top_level_name = entries[0]

        # Check for Cargo.lock
        cargo_lock = os.path.join(extracted_dir, "Cargo.lock")
        if not os.path.exists(cargo_lock):
            print(
                f"Error: no Cargo.lock found in sdist {top_level_name}", file=sys.stderr
            )
            sys.exit(1)

        locked_packages = third_party_packages(cargo_lock)

        # Prune Cargo.lock to only packages needed by the included crates.
        print(f"Pruning Cargo.lock in {top_level_name}...")
        subprocess.run(
            ["cargo", "update", "--workspace"],
            cwd=extracted_dir,
            check=True,
        )

        # `cargo update --workspace` can also add dependencies, so verify the
        # result contains no new third-party packages.
        added_packages = third_party_packages(cargo_lock) - locked_packages
        if added_packages:
            print(
                f"Error: pruning Cargo.lock added dependencies: {sorted(added_packages)}",
                file=sys.stderr,
            )
            sys.exit(1)

        # Verify it works with --locked
        print("Verifying Cargo.lock with --locked...")
        result = subprocess.run(
            ["cargo", "metadata", "--locked", "--format-version=1"],
            cwd=extracted_dir,
            capture_output=True,
            check=False,
        )
        if result.returncode != 0:
            print(
                f"Error: Cargo.lock still out of sync after pruning:\n{result.stderr.decode()}",
                file=sys.stderr,
            )
            sys.exit(1)
        print("Cargo.lock is consistent.")

        # Repack the tarball
        print(f"Repacking {sdist_path}...")
        with tarfile.open(sdist_path, "w:gz") as tar:
            tar.add(extracted_dir, arcname=top_level_name)

    print("Done.")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Fix Cargo.lock in a maturin-generated sdist"
    )
    parser.add_argument("sdist", help="Path to the sdist .tar.gz file")
    args = parser.parse_args()
    fix_sdist_lockfile(args.sdist)


if __name__ == "__main__":
    main()
