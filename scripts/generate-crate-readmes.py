# /// script
# requires-python = ">=3.13"
# dependencies = []
# ///

import json
import pathlib
import subprocess

GENERATED_HEADER = "<!-- This file is generated. DO NOT EDIT -->"

UV_TEMPLATE = """
{GENERATED_HEADER}

# uv

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


MEMBER_TEMPLATE = """
{GENERATED_HEADER}

# {name}

This crate is an internal component of [uv](https://crates.io/uv). The Rust API exposed here is
unstable and will have frequent breaking changes.

See uv's [crate versioning
policy](https://docs.astral.sh/uv/reference/policies/versioning/#crate-versioning) for details on
versioning.
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
        package = packages[workspace_member]
        name = package["name"]
        # Skip the main uv crate
        if name == "uv":
            continue
        # Skip crates with publish = false
        if package.get("publish") == []:
            continue
        workspace_members.append(name)

    workspace_members.sort()

    members_list = "\n".join(
        f"- [{name}](https://crates.io/crates/{name})" for name in workspace_members
    )

    # Generate README for the main uv crate
    readme_content = UV_TEMPLATE.format(
        GENERATED_HEADER=GENERATED_HEADER, WORKSPACE_MEMBERS=members_list
    )
    readme_path.write_text(readme_content)

    # Track all generated README paths for formatting at the end
    generated_paths = [readme_path]

    # Generate READMEs for all workspace members
    for workspace_member in content["workspace_members"]:
        package = packages[workspace_member]
        name = package["name"]

        # Skip the main uv crate (already handled above)
        if name == "uv":
            continue

        # Determine the README path for this crate
        manifest_path = pathlib.Path(package["manifest_path"])
        crate_dir = manifest_path.parent
        member_readme_path = crate_dir / "README.md"

        # Check if README already exists
        if member_readme_path.exists():
            existing_content = member_readme_path.read_text()
            # Skip if it doesn't have the generated header
            if not existing_content.startswith(GENERATED_HEADER):
                print(f"Skipping {name}: existing README without generated header")
                continue

        # Generate the README content
        member_readme_content = MEMBER_TEMPLATE.format(
            GENERATED_HEADER=GENERATED_HEADER, name=name
        )
        member_readme_path.write_text(member_readme_content)
        generated_paths.append(member_readme_path)

        print(f"Generated README for {name}")

    # Format all generated READMEs once at the end
    subprocess.run(
        ["npx", "prettier", "--write", "--prose-wrap", "always"]
        + [str(path) for path in generated_paths],
        check=True,
    )


if __name__ == "__main__":
    main()
