# Update the version pins for workspace member crates in the root `Cargo.toml` to reflect the
# versions set in their indivuidual `Cargo.toml` files.
#
# This is intended to be used after `bump-workspace-crate-versions.py`.

# /// script
# requires-python = ">=3.14"
# dependencies = []
# ///


import json
import pathlib
import subprocess
import tomllib


def main() -> None:
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1"],
        capture_output=True,
        text=True,
        check=True,
    )
    content = json.loads(result.stdout)
    packages = {package["id"]: package for package in content["packages"]}
    versions = {}

    workspace_manifest = pathlib.Path(content["workspace_root"]) / "Cargo.toml"
    contents = workspace_manifest.read_text()
    parsed_workspace_manifest = tomllib.loads(contents)

    # Generate a mapping of the version in the workspace manifest to the
    # the current version of the crate
    for workspace_member in content["workspace_members"]:
        name = packages[workspace_member]["name"]

        new_version = packages[workspace_member]["version"]
        old_version = parsed_workspace_manifest["workspace"]["dependencies"]["name"][
            "version"
        ]

        versions[name] = (old_version, new_version)

    # Update all the pins in the workspace root
    for name, (old_version, new_version) in versions.items():
        if old_version == new_version:
            continue
        contents = contents.replace(
            f'{name} = {{ version = "{old_version}"',
            f'{name} = {{ version = "{new_version}"',
        )

    workspace_manifest.write_text(contents)


if __name__ == "__main__":
    main()
