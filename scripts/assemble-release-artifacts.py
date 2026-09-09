# /// script
# requires-python = ">=3.12"
# dependencies = []
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Assemble uv's complete release distributions from built and signed artifacts.

Every configured target contributes one wheel per package and one GitHub archive.
Use the signing job's replacements where signing is required; copy other builds
and the source distributions unchanged.
"""

import argparse
import hashlib
import json
import os
import shutil
from pathlib import Path

PACKAGES = ("uv", "uv_build")


def one_file(directory: Path, pattern: str) -> Path:
    """Require one matching distribution in a build directory."""
    files = sorted(directory.glob(pattern))
    if len(files) != 1:
        raise ValueError(f"Expected one {pattern} in {directory}, found {len(files)}")
    return files[0]


def copy_distribution(source: Path, directory: Path) -> None:
    """Copy a distribution without replacing another target's output."""
    destination = directory / source.name
    if destination.exists():
        raise ValueError(f"Duplicate release distribution: {source.name}")
    shutil.copyfile(source, destination)


def check_inventory(directory: Path, expected: set[str]) -> None:
    """Require a flat artifact directory to contain exactly the expected files."""
    actual = {path.name for path in directory.iterdir()}
    if actual != expected:
        raise ValueError(
            f"Unexpected files in {directory}: missing={sorted(expected - actual)}, "
            f"extra={sorted(actual - expected)}"
        )


def check_checksum(archive: Path, checksum: Path) -> None:
    """Check the sha256sum sidecar before forwarding a release archive."""
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    if checksum.read_text(encoding="utf-8").replace(" *", "  ").split() != [
        digest,
        archive.name,
    ]:
        raise ValueError(f"Archive checksum differs from input: {archive.name}")


def assemble(
    built: Path, signed_wheels: Path, signed_archives: Path, output: Path
) -> None:
    """Copy every expected distribution into its release artifact directory."""
    plan = json.loads(os.environ["RELEASE_PLAN"])
    targets = json.loads(os.environ["RELEASE_TARGETS"])
    signed_targets = {
        platform["target"]: system
        for system in ("macos", "windows")
        for platform in json.loads(os.environ[f"{system.upper()}_TARGETS"])
    }
    releases = [release for release in plan["releases"] if release["app_name"] == "uv"]
    if len(releases) != 1:
        raise ValueError("Expected one uv release in the cargo-dist plan")
    archives = {}
    for name in releases[0]["artifacts"]:
        artifact = plan["artifacts"][name]
        if artifact["kind"] != "executable-zip":
            continue
        (target,) = artifact["target_triples"]
        if target in archives:
            raise ValueError(f"Multiple GitHub archives for {target}")
        archives[target] = (name, artifact["checksum"])
    if set(archives) != set(targets) or not signed_targets.keys() <= archives.keys():
        raise ValueError(
            "GitHub archive targets differ from the release target inventory"
        )

    output.mkdir()
    selected_signed_wheels = set()
    for package in PACKAGES:
        wheels = output / "wheels" / package
        wheels.mkdir(parents=True)
        for target in targets:
            source = one_file(built / "wheels" / target, f"{package}-*.whl")
            if system := signed_targets.get(target):
                source = signed_wheels / system / target / source.name
                selected_signed_wheels.add(source)
            copy_distribution(source, wheels)

        sdists = output / "sdists" / package
        sdists.mkdir(parents=True)
        source = one_file(built / "sdists" / package, f"{package}-*.tar.gz")
        check_inventory(source.parent, {source.name})
        copy_distribution(source, sdists)

    for target in targets:
        directory = built / "wheels" / target
        check_inventory(
            directory,
            {one_file(directory, f"{package}-*.whl").name for package in PACKAGES},
        )
    if set(signed_wheels.rglob("*.whl")) != selected_signed_wheels:
        raise ValueError("Signed wheels differ from the signing target inventory")

    destination = output / "github-archives"
    destination.mkdir()
    expected_built = {name for names in archives.values() for name in names}
    expected_signed = {name for target in signed_targets for name in archives[target]}
    check_inventory(built / "github-archives", expected_built)
    check_inventory(signed_archives, expected_signed)
    for target, (archive, checksum) in archives.items():
        source = (
            signed_archives if target in signed_targets else built / "github-archives"
        )
        check_checksum(source / archive, source / checksum)
        for name in (archive, checksum):
            copy_distribution(source / name, destination)


def main() -> None:
    """Assemble release files from the calling workflow's prepared directories."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("built", type=Path)
    parser.add_argument("signed_wheels", type=Path)
    parser.add_argument("signed_archives", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    assemble(args.built, args.signed_wheels, args.signed_archives, args.output)


if __name__ == "__main__":
    main()
