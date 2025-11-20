# List all crates in the workspace.
#
# /// script
# requires-python = ">=3.14"
# dependencies = []
# ///


import json
import subprocess


def main() -> None:
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1"],
        capture_output=True,
        text=True,
        check=True,
    )
    content = json.loads(result.stdout)
    packages = {package["id"]: package for package in content["packages"]}

    for workspace_member in content["workspace_members"]:
        name = packages[workspace_member]["name"]
        print(name)


if __name__ == "__main__":
    main()
