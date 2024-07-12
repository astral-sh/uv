"""Update uv.json in schemastore.

This script will clone astral-sh/schemastore, update the schema and push the changes
to a new branch tagged with the uv git hash. You should see a URL to create the PR
to schemastore in the CLI.
"""

from __future__ import annotations

import json
from pathlib import Path
from subprocess import check_call, check_output
from tempfile import TemporaryDirectory

SCHEMASTORE_FORK = "git@github.com:astral-sh/schemastore.git"
SCHEMASTORE_UPSTREAM = "git@github.com:SchemaStore/schemastore.git"
UV_REPOSITORY = "https://github.com/astral-sh/uv"
UV_JSON_PATH = Path("schemas/json/uv.json")


def update_schemastore(schemastore: Path, *, root: Path) -> None:
    if not schemastore.is_dir():
        check_call(["git", "clone", SCHEMASTORE_FORK, schemastore])
        check_call(
            [
                "git",
                "remote",
                "add",
                "upstream",
                SCHEMASTORE_UPSTREAM,
            ],
            cwd=schemastore,
        )
    # Create a new branch tagged with the current uv commit up to date with the latest
    # upstream schemastore
    check_call(["git", "fetch", "upstream"], cwd=schemastore)
    current_sha = check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    branch = f"update-uv-{current_sha}"
    check_call(
        ["git", "switch", "-c", branch],
        cwd=schemastore,
    )
    check_call(
        ["git", "reset", "--hard", "upstream/master"],
        cwd=schemastore,
    )

    # Run npm install
    check_call(["npm", "install"], cwd=schemastore)

    src = schemastore.joinpath("src")

    # Update the schema and format appropriately
    schema = json.loads(root.joinpath("uv.schema.json").read_text())
    schema["$id"] = "https://json.schemastore.org/uv.json"
    src.joinpath(UV_JSON_PATH).write_text(
        json.dumps(dict(schema.items()), indent=2, ensure_ascii=False),
    )
    check_call(
        [
            "../node_modules/.bin/prettier",
            "--plugin",
            "prettier-plugin-sort-json",
            "--write",
            UV_JSON_PATH,
        ],
        cwd=src,
    )

    # Check if the schema has changed
    # https://stackoverflow.com/a/9393642/3549270
    if check_output(["git", "status", "-s"], cwd=schemastore).strip():
        # Schema has changed, commit and push
        commit_url = f"{UV_REPOSITORY}/commit/{current_sha}"
        commit_body = f"This updates uv's JSON schema to [{current_sha}]({commit_url})"
        # https://stackoverflow.com/a/22909204/3549270
        check_call(
            [
                "git",
                "commit",
                "-a",
                "-m",
                "Update uv's JSON schema",
                "-m",
                commit_body,
            ],
            cwd=schemastore,
        )
        # This should show the link to create a PR
        check_call(
            ["git", "push", "--set-upstream", "origin", branch],
            cwd=schemastore,
        )
    else:
        print("No changes")


def main() -> None:
    root = Path(
        check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip(),
    )

    schemastore = root.joinpath("schemastore")
    if schemastore.is_dir():
        update_schemastore(schemastore, root=root)
    else:
        with TemporaryDirectory() as temp_dir:
            update_schemastore(Path(temp_dir).joinpath("schemastore"), root=root)


if __name__ == "__main__":
    main()
