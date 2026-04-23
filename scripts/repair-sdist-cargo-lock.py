#!/usr/bin/env python3
"""Fix a maturin-generated sdist tarball.

Two fixups are applied:

1. Prune Cargo.lock. Maturin copies the full workspace Cargo.lock into the
   sdist, but the sdist only contains a subset of workspace crates. This
   makes `cargo build --locked` fail because the lock file references packages
   not present in the sdist. `cargo update --workspace` prunes the lockfile to
   only the packages needed by the included crates without changing any pinned
   versions. See: https://github.com/astral-sh/uv/issues/18824

2. Inject rust-toolchain.toml. Maturin does not include the workspace-root
   rust-toolchain.toml in a crate-scoped sdist, so rustup falls back to the
   builder's preinstalled rustc — which may be below our MSRV. Copying the
   root toolchain file ensures the sdist builds the same way as the full uv
   sdist (which does ship it).
"""

import argparse
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.dirname(SCRIPT_DIR)
RUST_TOOLCHAIN_SRC = os.path.join(REPO_ROOT, "rust-toolchain.toml")


def fix_sdist_lockfile(sdist_path: str) -> None:
    sdist_path = os.path.abspath(sdist_path)
    if not tarfile.is_tarfile(sdist_path):
        print(f"Error: {sdist_path} is not a valid tar file", file=sys.stderr)
        sys.exit(1)

    if not os.path.exists(RUST_TOOLCHAIN_SRC):
        print(f"Error: {RUST_TOOLCHAIN_SRC} not found", file=sys.stderr)
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

        # Inject rust-toolchain.toml so `pip install` of the sdist uses the
        # pinned toolchain instead of whatever rustc the builder has installed.
        toolchain_dst = os.path.join(extracted_dir, "rust-toolchain.toml")
        print(f"Injecting rust-toolchain.toml into {top_level_name}...")
        shutil.copyfile(RUST_TOOLCHAIN_SRC, toolchain_dst)

        # Prune Cargo.lock to only packages needed by the included crates.
        # `cargo update --workspace` removes entries for missing workspace members
        # while preserving pinned versions for all remaining dependencies.
        print(f"Pruning Cargo.lock in {top_level_name}...")
        subprocess.run(
            ["cargo", "update", "--workspace"],
            cwd=extracted_dir,
            check=True,
        )

        # Verify it works with --locked
        print("Verifying Cargo.lock with --locked...")
        result = subprocess.run(
            ["cargo", "metadata", "--locked", "--format-version=1"],
            cwd=extracted_dir,
            capture_output=True,
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
