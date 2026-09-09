# /// script
# requires-python = ">=3.12"
# dependencies = []
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Check a release directory against its inventory and print the files to publish."""

import argparse
import fnmatch
import json
from pathlib import Path


def check_files(directory: Path, files: list[str]) -> None:
    """Require distinct, flat filenames and an exact directory inventory."""
    expected = set(files)
    if len(expected) != len(files) or any(
        Path(name).name != name or "\\" in name for name in files
    ):
        raise ValueError("Release inventory must contain distinct filenames")
    actual = {path.name for path in directory.iterdir()}
    if actual != expected:
        raise ValueError(
            f"Release inventory differs: missing={sorted(expected - actual)}, "
            f"extra={sorted(actual - expected)}"
        )
    for name in files:
        if not (directory / name).is_file():
            raise ValueError(f"Release asset is not a file: {name}")


def main() -> None:
    """Print only the files named by the assembly or cargo-dist manifest."""
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    python = commands.add_parser("python", help="List a package's wheels and sdist")
    python.add_argument("inventory", type=Path)
    python.add_argument("directory", type=Path)
    python.add_argument("package", choices=("uv", "uv_build"))
    github = commands.add_parser("github", help="List cargo-dist's release assets")
    github.add_argument("manifest", type=Path)
    github.add_argument("directory", type=Path)
    github.add_argument("--include-manifest", action="store_true")
    github.add_argument("--attest", action="store_true")
    args = parser.parse_args()

    if args.command == "python":
        inventory = json.loads(args.inventory.read_text(encoding="utf-8"))
        files = inventory["wheels"][args.package] + inventory["sdists"][args.package]
    else:
        manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
        files = [
            name for release in manifest["releases"] for name in release["artifacts"]
        ]
        if args.include_manifest:
            files.append("dist-manifest.json")

    check_files(args.directory, files)
    if args.command == "github" and args.attest:
        files = [
            name
            for name in files
            if any(
                fnmatch.fnmatchcase(name, pattern)
                for pattern in manifest["github_attestations_filters"]
            )
        ]
    for name in sorted(files):
        print(args.directory / name)


if __name__ == "__main__":
    main()
