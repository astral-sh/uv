# /// script
# requires-python = ">=3.13"
# dependencies = []
# ///

import json
import pathlib
import subprocess

TEMPLATE = """# uv

uv is a Python package and project manager.

See the [documentation](https://docs.astral.sh/uv/) or [repository](https://github.com/astral-sh/uv)
for more information.

This crate is the entry point to the uv command-line interface. The Rust API exposed here is not
considered public interface.

The following uv workspace members are also available:

{WORKSPACE_MEMBERS}

uv's workspace members are considered internal and will have frequent breaking changes.

See uv's [crate versioning policy](https://docs.astral.sh/uv/reference/policies/versioning/#crate-versioning) for details on versioning.
"""


def main() -> None:
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1"],
        capture_output=True,
        text=True,
        check=True,
    )
    content = json.loads(result.stdout)
    packages = {package["id"]: package for package in content["packages"]}

    workspace_root = pathlib.Path(content["workspace_root"])
    readme_path = workspace_root / "crates" / "uv" / "README.md"

    workspace_members = []
    for workspace_member in content["workspace_members"]:
        name = packages[workspace_member]["name"]
        if name != "uv":
            workspace_members.append(name)

    workspace_members.sort()

    members_list = "\n".join(
        f"- [{name}](https://crates.io/crates/{name})" for name in workspace_members
    )

    readme_content = TEMPLATE.format(WORKSPACE_MEMBERS=members_list)

    readme_path.write_text(readme_content)

    subprocess.run(
        ["npx", "prettier", "--write", "--prose-wrap", "always", str(readme_path)],
        check=True,
    )


if __name__ == "__main__":
    main()
