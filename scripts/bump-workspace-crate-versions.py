# Naively increment the patch version of each crate in the workspace.
#
# This excludes crates which are versioned as binaries.
#
# After incrementing the version in each member `Cargo.toml`, it updates the version pins in the
# root `Cargo.toml` to match.

# /// script
# requires-python = ">=3.13"
# dependencies = []
# ///


import json
import pathlib
import subprocess
import tomllib

NO_BUMP_CRATES = {"uv", "uv-build", "uv-version"}


def main() -> None:
    # Pre-sync NO_BUMP_CRATES versions before running cargo metadata.
    #
    # Rooster updates crate versions but isn't workspace-aware, so it doesn't update
    # the workspace dependency pins in the root Cargo.toml. This can cause cargo metadata
    # to fail with version mismatches (e.g., when going from 0.9.x to 0.10.0).
    # We fix the pins first by reading the crate versions directly.
    script_dir = pathlib.Path(__file__).parent
    workspace_manifest = script_dir.parent / "Cargo.toml"
    workspace_manifest_contents = workspace_manifest.read_text()
    parsed_workspace_manifest = tomllib.loads(workspace_manifest_contents)

    for crate_name in NO_BUMP_CRATES:
        crate_manifest = script_dir.parent / "crates" / crate_name / "Cargo.toml"
        if not crate_manifest.exists():
            continue
        crate_version = tomllib.loads(crate_manifest.read_text())["package"]["version"]
        manifest_dependency = parsed_workspace_manifest["workspace"]["dependencies"].get(
            crate_name
        )
        if manifest_dependency is None:
            continue
        manifest_version = manifest_dependency["version"]
        if manifest_version != crate_version:
            workspace_manifest_contents = workspace_manifest_contents.replace(
                f'{crate_name} = {{ version = "{manifest_version}"',
                f'{crate_name} = {{ version = "{crate_version}"',
            )

    workspace_manifest.write_text(workspace_manifest_contents)

    # Now cargo metadata will succeed
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1"],
        capture_output=True,
        text=True,
        check=True,
    )
    content = json.loads(result.stdout)
    packages = {package["id"]: package for package in content["packages"]}

    workspace_manifest_contents = workspace_manifest.read_text()
    parsed_workspace_manifest = tomllib.loads(workspace_manifest_contents)

    version_changes = {}

    for workspace_member in content["workspace_members"]:
        manifest = pathlib.Path(packages[workspace_member]["manifest_path"])
        name = packages[workspace_member]["name"]

        # For the members we're not bumping, we'll still make sure that the version pinned in the
        # workspace manifest matches the version of the crate. This is done because Rooster isn't
        # Cargo workspace aware and won't otherwise bump these when updating the member `Cargo.toml`
        # files. We could make Rooster smarter instead of this.
        if name in NO_BUMP_CRATES:
            manifest_dependency = parsed_workspace_manifest["workspace"][
                "dependencies"
            ].get(name)
            if manifest_dependency is None:
                continue
            manifest_version = manifest_dependency["version"]
            metadata_version = packages[workspace_member]["version"]
            if manifest_version != metadata_version:
                version_changes[name] = (manifest_version, metadata_version)
            continue

        # For other members, bump the patch version
        version = packages[workspace_member]["version"]
        version_parts = [int(part) for part in version.split(".")]
        new_version = f"{version_parts[0]}.{version_parts[1]}.{version_parts[2] + 1}"

        contents = manifest.read_text()
        contents = contents.replace(
            'version = "' + version + '"',
            'version = "' + new_version + '"',
            1,
        )

        version_changes[name] = (version, new_version)
        manifest.write_text(contents)

    # Update all the pins in the workspace root
    for name, (old_version, new_version) in version_changes.items():
        workspace_manifest_contents = workspace_manifest_contents.replace(
            f'{name} = {{ version = "{old_version}"',
            f'{name} = {{ version = "{new_version}"',
        )

    workspace_manifest.write_text(workspace_manifest_contents)


if __name__ == "__main__":
    main()
