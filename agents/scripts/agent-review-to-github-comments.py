#!/usr/bin/env -S uv run --script
#
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///

"""Convert a structured agent review into a GitHub pull request review payload."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import PurePosixPath
from typing import Any, TextIO


def without_mentions(text: str) -> str:
    return re.sub(r"(?<![A-Za-z0-9_.])@(?=[A-Za-z0-9])", "@\u200b", text)


def review_payload(review: dict[str, Any], commit_id: str) -> dict[str, Any]:
    if not re.fullmatch(r"[0-9a-fA-F]{40}", commit_id):
        msg = "commit ID must be a full Git commit SHA"
        raise ValueError(msg)

    comments = []
    for finding in review["findings"]:
        location = finding["code_location"]
        path = PurePosixPath(location["relative_file_path"])
        if path.is_absolute() or ".." in path.parts or str(path) in {"", "."}:
            msg = f"invalid repository-relative path: {path}"
            raise ValueError(msg)

        side = location["side"]
        if side not in {"LEFT", "RIGHT"}:
            msg = f"invalid diff side: {side}"
            raise ValueError(msg)

        line_range = location["line_range"]
        start = line_range["start"]
        end = line_range["end"]
        if start < 1 or end < start:
            msg = f"invalid line range: {start}-{end}"
            raise ValueError(msg)

        comment: dict[str, Any] = {
            "path": str(path),
            "line": end,
            "side": side,
            "body": without_mentions(
                f"**[P{finding['priority']}] {finding['title']}**\n\n"
                f"{finding['body']}\n\n"
                f"Confidence: {finding['confidence_score']}"
            ),
        }
        if start != end:
            comment["start_line"] = start
            comment["start_side"] = side
        comments.append(comment)

    return {
        "commit_id": commit_id,
        "event": "COMMENT",
        "body": without_mentions(f"**Automated review**\n\n{review['summary']}"),
        "comments": comments,
    }


def main(stdin: TextIO = sys.stdin, stdout: TextIO = sys.stdout) -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--commit-id", required=True, help="the pull request head commit SHA"
    )
    args = parser.parse_args()

    review = json.load(stdin)
    json.dump(review_payload(review, args.commit_id), stdout, indent=2)
    stdout.write("\n")


if __name__ == "__main__":
    main()
