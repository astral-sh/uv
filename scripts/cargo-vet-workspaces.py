# /// script
# requires-python = ">=3.11"
# dependencies = []
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Select Cargo workspaces to vet in CI."""

import json
import os
import subprocess
import sys
from pathlib import Path

LOCKFILE_PATHS = ("Cargo.lock", ":(glob)crates/**/Cargo.lock")


def git(*arguments: str) -> bytes:
    return subprocess.check_output(["git", *arguments])


def lockfiles(*arguments: str) -> list[Path]:
    return [
        Path(os.fsdecode(path))
        for path in git(*arguments, "--", *LOCKFILE_PATHS).split(b"\0")
        if path
    ]


def main() -> int:
    event = os.environ["GITHUB_EVENT_NAME"]
    reference = os.environ["GITHUB_REF"]
    if os.environ["GITHUB_REPOSITORY_OWNER"] != "astral-sh" or (
        event == "pull_request"
        and os.environ["HEAD_REPOSITORY"] != os.environ["GITHUB_REPOSITORY"]
    ):
        print("workspaces=[]")
        return 0

    if event == "push":
        comparison_base = os.environ["PUSH_BEFORE_SHA"]
    elif reference.startswith("refs/pull/") and reference.endswith("/merge"):
        comparison_base = "HEAD^1"
    else:
        base_sha = os.environ["BASE_SHA"] or "origin/main"
        comparison_base = git("merge-base", base_sha, "HEAD").decode().strip()

    changed = lockfiles(
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--no-renames",
        "--name-only",
        "-z",
        comparison_base,
        "HEAD",
    )
    if not changed:
        print("workspaces=[]")
        return 0

    for lockfile in changed:
        if not lockfile.is_file() and lockfile.with_name("Cargo.toml").is_file():
            print(
                f"::error::Cannot vet a Cargo workspace without its lockfile: {lockfile}",
                file=sys.stderr,
            )
            return 1

    workspaces = [str(lockfile.parent) for lockfile in lockfiles("ls-files", "-z")]
    print(f"workspaces={json.dumps(workspaces)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
