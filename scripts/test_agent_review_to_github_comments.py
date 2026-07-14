"""Integration-style tests for the agent review to GitHub review converter."""

from __future__ import annotations

import json
import subprocess
import sys
import unittest
from pathlib import Path
from typing import Any

SCRIPT = Path(__file__).with_name("agent-review-to-github-comments.py")
COMMIT_ID = "0123456789abcdef0123456789abcdef01234567"


def convert(
    review: dict[str, Any], commit_id: str = COMMIT_ID
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(SCRIPT), "--commit-id", commit_id],
        input=json.dumps(review),
        text=True,
        capture_output=True,
        check=False,
    )


class AgentReviewToGitHubCommentsTest(unittest.TestCase):
    def test_converts_single_and_multi_line_findings(self) -> None:
        review = {
            "findings": [
                {
                    "title": "Handle an empty input",
                    "body": "The new indexing operation fails when the input is empty.",
                    "confidence_score": 0.99,
                    "priority": 1,
                    "code_location": {
                        "relative_file_path": "crates/example/src/lib.rs",
                        "side": "RIGHT",
                        "line_range": {"start": 12, "end": 12},
                    },
                },
                {
                    "title": "Preserve the removed fallback",
                    "body": "Removing this branch changes the documented fallback behavior.",
                    "confidence_score": 0.9,
                    "priority": 2,
                    "code_location": {
                        "relative_file_path": "crates/example/src/config.rs",
                        "side": "LEFT",
                        "line_range": {"start": 24, "end": 27},
                    },
                },
            ],
            "overall_correctness": "patch is incorrect",
            "overall_explanation": "The patch introduces two behavior regressions.",
            "overall_confidence_score": 0.97,
        }

        result = convert(review)

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            json.loads(result.stdout),
            {
                "commit_id": COMMIT_ID,
                "event": "COMMENT",
                "body": (
                    "**Automated review: patch is incorrect**\n\n"
                    "The patch introduces two behavior regressions.\n\n"
                    "Confidence: 0.97"
                ),
                "comments": [
                    {
                        "path": "crates/example/src/lib.rs",
                        "line": 12,
                        "side": "RIGHT",
                        "body": (
                            "**[P1] Handle an empty input**\n\n"
                            "The new indexing operation fails when the input is empty.\n\n"
                            "Confidence: 0.99"
                        ),
                    },
                    {
                        "path": "crates/example/src/config.rs",
                        "line": 27,
                        "side": "LEFT",
                        "start_line": 24,
                        "start_side": "LEFT",
                        "body": (
                            "**[P2] Preserve the removed fallback**\n\n"
                            "Removing this branch changes the documented fallback behavior.\n\n"
                            "Confidence: 0.9"
                        ),
                    },
                ],
            },
        )

    def test_converts_an_empty_review(self) -> None:
        result = convert(
            {
                "findings": [],
                "overall_correctness": "patch is correct",
                "overall_explanation": "No actionable issues were found.",
                "overall_confidence_score": 0.92,
            }
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["comments"], [])

    def test_rejects_invalid_locations_and_commit_ids(self) -> None:
        finding = {
            "title": "Invalid finding",
            "body": "This location cannot be attached to a pull request.",
            "confidence_score": 1,
            "priority": 1,
            "code_location": {
                "relative_file_path": "../outside.rs",
                "side": "RIGHT",
                "line_range": {"start": 4, "end": 4},
            },
        }
        review = {
            "findings": [finding],
            "overall_correctness": "patch is incorrect",
            "overall_explanation": "An invalid finding was produced.",
            "overall_confidence_score": 1,
        }

        invalid_path = convert(review)
        self.assertNotEqual(invalid_path.returncode, 0)
        self.assertIn("invalid repository-relative path", invalid_path.stderr)

        finding["code_location"]["relative_file_path"] = "src/lib.rs"
        finding["code_location"]["line_range"] = {"start": 9, "end": 4}
        invalid_range = convert(review)
        self.assertNotEqual(invalid_range.returncode, 0)
        self.assertIn("invalid line range", invalid_range.stderr)

        finding["code_location"]["line_range"] = {"start": 4, "end": 4}
        invalid_commit = convert(review, commit_id="not-a-sha")
        self.assertNotEqual(invalid_commit.returncode, 0)
        self.assertIn("commit ID must be a full Git commit SHA", invalid_commit.stderr)

    def test_neutralizes_mentions_in_review_feedback(self) -> None:
        result = convert(
            {
                "findings": [
                    {
                        "title": "Avoid notifying @maintainer",
                        "body": "The value for @astral-sh/team is not an email like uv@example.com.",
                        "confidence_score": 0.98,
                        "priority": 2,
                        "code_location": {
                            "relative_file_path": "src/lib.rs",
                            "side": "RIGHT",
                            "line_range": {"start": 8, "end": 8},
                        },
                    }
                ],
                "overall_correctness": "patch is incorrect",
                "overall_explanation": "Please inspect the branch mentioned by @reviewer.",
                "overall_confidence_score": 0.96,
            }
        )

        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(result.stdout)
        self.assertIn("@\u200bmaintainer", payload["comments"][0]["body"])
        self.assertIn("@\u200bastral-sh/team", payload["comments"][0]["body"])
        self.assertIn("uv@example.com", payload["comments"][0]["body"])
        self.assertIn("@\u200breviewer", payload["body"])


if __name__ == "__main__":
    unittest.main()
