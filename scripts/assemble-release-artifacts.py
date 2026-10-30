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
    """Return the only path matching `pattern` in `directory`.

    Reject missing or duplicate build outputs instead of choosing one implicitly.
    """
    files = sorted(directory.glob(pattern))
    if len(files) != 1:
        raise ValueError(f"Expected one {pattern} in {directory}, found {len(files)}")
    return files[0]


def copy_distribution(source: Path, directory: Path) -> None:
    """Copy `source` into an existing directory, preserving its name and bytes.

    Reject a filename collision rather than overwrite another target's output.
    """
    destination = directory / source.name
    if destination.exists():
        raise ValueError(f"Duplicate release distribution: {source.name}")
    shutil.copyfile(source, destination)


def check_inventory(directory: Path, expected: set[str]) -> None:
    """Require the immediate entries in `directory` to match `expected`.

    `expected` contains filenames, not paths. Report both missing and unexpected
    entries; subdirectories are not searched.
    """
    actual = {path.name for path in directory.iterdir()}
    if actual != expected:
        raise ValueError(
            f"Unexpected files in {directory}: missing={sorted(expected - actual)}, "
            f"extra={sorted(actual - expected)}"
        )


def check_checksum(archive: Path, checksum: Path) -> None:
    """Verify an archive's bytes and filename against its SHA-256 sidecar.

    Accept the text and binary forms written by `sha256sum`. Reject a different
    digest or recorded filename before forwarding the archive to publication.
    """
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    if checksum.read_text(encoding="utf-8").replace(" *", "  ").split() != [
        digest,
        archive.name,
    ]:
        raise ValueError(f"Archive checksum differs from input: {archive.name}")


def assemble(
    built: Path, signed_wheels: Path, signed_archives: Path, output: Path
) -> None:
    """Assemble every configured target into new publication directories.

    `built` contains `wheels/<target>`, `sdists/<package>`, and `github-archives`.
    `signed_wheels` contains `<system>/<target>` directories; `signed_archives`
    is flat. The JSON values in `RELEASE_PLAN`, `RELEASE_TARGETS`, `MACOS_TARGETS`,
    and `WINDOWS_TARGETS` identify the expected archives and signed replacements.

    Require complete inputs, use signed replacements for macOS and Windows, and
    copy the other distributions unchanged. Missing replacements never fall back
    to the original binaries. `output` must not exist; it receives
    `wheels/<package>`, `sdists/<package>`, and `github-archives` directories.
    """
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
    """Parse the four directory arguments and assemble the release.

    The calling signing workflow supplies the release plan and target matrices
    through the environment.
    """
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("built", type=Path)
    parser.add_argument("signed_wheels", type=Path)
    parser.add_argument("signed_archives", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    assemble(args.built, args.signed_wheels, args.signed_archives, args.output)


if __name__ == "__main__":
    main()
