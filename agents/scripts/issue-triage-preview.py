# /// script
# requires-python = ">=3.12"
# dependencies = ["httpx"]
# ///

"""Collect recent issues and update the issue-triage preview comment."""

import argparse
import json
import os
from pathlib import Path
from typing import Any

import httpx

MARKER = "<!-- issue-triage-preview -->"


def request(
    method: str,
    path: str,
    *,
    query: dict[str, str | int] | None = None,
    body: dict[str, str] | None = None,
) -> Any:
    response = httpx.request(
        method,
        f"{os.environ['GITHUB_API_URL']}/{path}",
        params=query,
        json=body,
        headers={
            "Accept": "application/vnd.github+json",
            "Authorization": f"Bearer {os.environ['GH_TOKEN']}",
            "X-GitHub-Api-Version": "2022-11-28",
        },
        timeout=30,
    )
    response.raise_for_status()
    return response.json()


def collect(limit: int) -> None:
    repository = os.environ["GITHUB_REPOSITORY"]
    result = request(
        "GET",
        "search/issues",
        query={
            "q": f"repo:{repository} is:issue",
            "sort": "created",
            "order": "desc",
            "per_page": limit,
        },
    )
    issues = [str(issue["number"]) for issue in result["items"]]

    with Path(os.environ["GITHUB_OUTPUT"]).open("a") as output:
        output.write(f"issues={json.dumps(issues, separators=(',', ':'))}\n")


def comment(pull_request: int) -> None:
    repository = os.environ["GITHUB_REPOSITORY"]
    comments_path = f"repos/{repository}/issues/{pull_request}/comments"
    comment_id = None
    page = 1

    while True:
        comments = request("GET", comments_path, query={"per_page": 100, "page": page})
        for issue_comment in comments:
            if (
                issue_comment["user"]["login"] == "github-actions[bot]"
                and MARKER in issue_comment["body"]
            ):
                comment_id = issue_comment["id"]
        if len(comments) < 100:
            break
        page += 1

    run_url = (
        f"{os.environ['GITHUB_SERVER_URL']}/{repository}/actions/runs/"
        f"{os.environ['GITHUB_RUN_ID']}"
    )
    body = {
        "body": (
            f"{MARKER}\n\n"
            "## Issue triage preview\n\n"
            "The issue triage preview is available in the "
            f"[workflow run summary]({run_url})."
        )
    }

    if comment_id is None:
        request("POST", comments_path, body=body)
    else:
        request("PATCH", f"repos/{repository}/issues/comments/{comment_id}", body=body)


def main() -> None:
    parser = argparse.ArgumentParser()
    commands = parser.add_subparsers(dest="command", required=True)

    collect_parser = commands.add_parser("collect")
    collect_parser.add_argument("--limit", type=int, default=5)

    comment_parser = commands.add_parser("comment")
    comment_parser.add_argument("--pull-request", type=int, required=True)

    args = parser.parse_args()
    if args.command == "collect":
        collect(args.limit)
    else:
        comment(args.pull_request)


if __name__ == "__main__":
    main()
