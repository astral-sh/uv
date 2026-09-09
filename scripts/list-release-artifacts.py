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


def check_names(files: list[str]) -> set[str]:
    """Require distinct, flat release filenames."""
    expected = set(files)
    if len(expected) != len(files) or any(
        Path(name).name != name or any(character in name for character in "\\\r\n")
        for name in files
    ):
        raise ValueError("Release inventory must contain distinct filenames")
    return expected


def check_files(directory: Path, files: list[str]) -> None:
    """Require an exact directory inventory of regular release files."""
    expected = check_names(files)
    actual = {path.name for path in directory.iterdir()}
    if actual != expected:
        raise ValueError(
            f"Release inventory differs: missing={sorted(expected - actual)}, "
            f"extra={sorted(actual - expected)}"
        )
    for name in files:
        if not (directory / name).is_file():
            raise ValueError(f"Release asset is not a file: {name}")


def github_files(manifest: dict) -> list[str]:
    """Read the release asset names selected by cargo-dist."""
    return [name for release in manifest["releases"] for name in release["artifacts"]]


def check_github_build(manifest: dict, inventory: dict) -> list[str]:
    """Require the assembled archives and global build to cover cargo-dist's plan."""
    global_paths = [Path(path) for path in manifest["upload_files"]]
    actual = check_names(
        inventory["github-archives"] + [path.name for path in global_paths]
    )
    files = github_files(manifest)
    expected = check_names(files)
    if actual != expected:
        raise ValueError(
            f"GitHub build inventory differs: missing={sorted(expected - actual)}, "
            f"extra={sorted(actual - expected)}"
        )
    for path in global_paths:
        if not path.is_file():
            raise ValueError(f"Global release asset is not a file: {path}")
    return files


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
    github_build = commands.add_parser(
        "github-build", help="Check the completed GitHub build"
    )
    github_build.add_argument("manifest", type=Path)
    github_build.add_argument("inventory", type=Path)
    args = parser.parse_args()

    if args.command == "github-build":
        manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
        inventory = json.loads(args.inventory.read_text(encoding="utf-8"))
        for name in sorted(check_github_build(manifest, inventory)):
            print(name)
        return

    if args.command == "python":
        inventory = json.loads(args.inventory.read_text(encoding="utf-8"))
        files = inventory["wheels"][args.package] + inventory["sdists"][args.package]
    else:
        manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
        files = github_files(manifest)
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
