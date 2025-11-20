# Naively increment the patch version of each crate in the workspace.
#
# /// script
# requires-python = ">=3.14"
# dependencies = []
# ///


import json
import pathlib
import subprocess

SKIP_MEMBERS = {"uv", "uv-build", "uv-version"}


def main() -> None:
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1"],
        capture_output=True,
        text=True,
        check=True,
    )
    content = json.loads(result.stdout)
    packages = {package["id"]: package for package in content["packages"]}
    version_changes = {}

    for workspace_member in content["workspace_members"]:
        manifest = pathlib.Path(packages[workspace_member]["manifest_path"])
        name = packages[workspace_member]["name"]

        if name in SKIP_MEMBERS:
            continue

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
    workspace_manifest = pathlib.Path(content["workspace_root"]) / "Cargo.toml"
    contents = workspace_manifest.read_text()
    for name, (old_version, new_version) in version_changes.items():
        contents = contents.replace(
            f'{name} = {{ version = "{old_version}"',
            f'{name} = {{ version = "{new_version}"',
        )

    workspace_manifest.write_text(contents)


if __name__ == "__main__":
    main()
