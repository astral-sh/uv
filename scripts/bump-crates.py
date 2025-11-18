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
    print(content["workspace_members"])
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

        manifest.write_text(contents)


if __name__ == "__main__":
    main()
