"""Exercise issue-context publication against a mocked GitHub API."""

from __future__ import annotations

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from typing import Any

MOCK_GITHUB = """#!/usr/bin/env python3
import json
import os
import sys

arguments = sys.argv[1:]
payload = None

if arguments[:2] == ["issue", "view"]:
    print(os.environ["MOCK_ISSUE"])
    raise SystemExit

if arguments[:2] == ["api", "graphql"]:
    query = arguments[arguments.index("--raw-field") + 1]
    if "mutation(" in query:
        response = {
            "name": arguments[arguments.index("name=issue/123")].split("=", 1)[1],
            "repository": {"nameWithOwner": "astral-sh/uv-dev"},
        }
    else:
        response = json.loads(os.environ["MOCK_CONTEXT"])
elif arguments[0] == "api":
    endpoint = arguments[3]
    payload = json.load(sys.stdin)
    if endpoint.endswith("/git/trees"):
        response = os.environ["MOCK_TREE_SHA"]
    elif endpoint.endswith("/git/commits"):
        response = os.environ["MOCK_COMMIT_SHA"]
    elif "/git/refs/heads/" in endpoint:
        response = ""
    else:
        raise SystemExit(f"unexpected GitHub API endpoint: {endpoint}")
else:
    raise SystemExit(f"unexpected GitHub CLI invocation: {arguments}")

with open(os.environ["MOCK_GH_LOG"], "a", encoding="utf-8") as log:
    log.write(json.dumps({"arguments": arguments, "payload": payload}) + "\\n")

if isinstance(response, str):
    print(response)
else:
    print(json.dumps(response))
"""


class IssueContextTests(unittest.TestCase):
    """Verify orphaned creation, safe updates, and input validation."""

    def setUp(self) -> None:
        runner_temporary = os.environ.get("RUNNER_TEMP")
        scratch_root = (
            Path(runner_temporary)
            if runner_temporary is not None
            else Path.home() / "code" / "tmp"
        )
        scratch_root.mkdir(parents=True, exist_ok=True)
        directory = tempfile.TemporaryDirectory(
            prefix="uv-issue-context-", dir=scratch_root
        )
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)

        github = self.root / "gh"
        github.write_text(MOCK_GITHUB, encoding="utf-8")
        github.chmod(0o755)

        self.issue: dict[str, Any] = {
            "id": "issue-node-id",
            "number": 123,
            "title": "Example issue",
            "body": "Issue details",
            "author": {"login": "reporter"},
            "url": "https://github.com/astral-sh/uv/issues/123",
        }
        self.triage: dict[str, Any] = {
            "related": {"items": [], "search_scope": "Existing issues"},
            "summary": "No existing issue was found.",
            "type": "bug",
            "type_reason": "The behavior is incorrect.",
        }
        self.context: dict[str, Any] = {
            "issue": {
                "id": "issue-node-id",
                "number": 123,
                "repository": {"nameWithOwner": "astral-sh/uv"},
                "linkedBranches": {"nodes": []},
            },
            "repository": {"id": "repository-node-id", "ref": None},
        }

        (self.root / "issue.json").write_text(
            json.dumps(self.issue, indent=2) + "\n", encoding="utf-8"
        )
        (self.root / "triage.json").write_text(
            json.dumps(self.triage, indent=2) + "\n", encoding="utf-8"
        )

        self.environment = {
            **os.environ,
            "GH_TOKEN": "test-token",
            "GITHUB_OUTPUT": str(self.root / "outputs"),
            "GITHUB_REPOSITORY": "astral-sh/uv",
            "GITHUB_STEP_SUMMARY": str(self.root / "summary"),
            "ISSUE": "123",
            "ISSUE_NODE_ID": "issue-node-id",
            "ISSUE_NUMBER": "123",
            "MOCK_COMMIT_SHA": "c" * 40,
            "MOCK_CONTEXT": json.dumps(self.context),
            "MOCK_GH_LOG": str(self.root / "calls.jsonl"),
            "MOCK_ISSUE": json.dumps(self.issue),
            "MOCK_TREE_SHA": "d" * 40,
            "PATH": f"{self.root}{os.pathsep}{os.environ['PATH']}",
            "RUNNER_TEMP": str(self.root),
            "TRIAGE_RESULT": json.dumps(self.triage),
        }

    def run_step(self, name: str) -> subprocess.CompletedProcess[str]:
        """Execute a workflow step using the fake GitHub CLI."""
        workflow = (
            Path(__file__).resolve().parents[2] / ".github/workflows/issue-triage.yml"
        )
        lines = workflow.read_text(encoding="utf-8").splitlines(keepends=True)
        step_index = lines.index(f'      - name: "{name}"\n')
        run_index = lines.index("        run: |\n", step_index)
        script = []
        for line in lines[run_index + 1 :]:
            if line.startswith("        ") and not line.startswith("          "):
                break
            script.append(line.removeprefix("          "))

        return subprocess.run(
            ["bash", "-e", "-o", "pipefail", "-c", "".join(script)],
            check=False,
            capture_output=True,
            env=self.environment,
            text=True,
        )

    def github_calls(self) -> list[dict[str, Any]]:
        """Return the GitHub API calls performed by the workflow step."""
        log = self.root / "calls.jsonl"
        if not log.exists():
            return []
        return [
            json.loads(line) for line in log.read_text(encoding="utf-8").splitlines()
        ]

    def existing_context(self, *, linked: bool = True) -> None:
        """Model a previously created issue-context branch."""
        self.context["repository"]["ref"] = {
            "name": "issue/123",
            "target": {"oid": "a" * 40, "tree": {"oid": "b" * 40}},
        }
        if linked:
            self.context["issue"]["linkedBranches"]["nodes"] = [
                {
                    "ref": {
                        "name": "issue/123",
                        "repository": {
                            "id": "repository-node-id",
                            "nameWithOwner": "astral-sh/uv-dev",
                        },
                    }
                }
            ]
        self.environment["MOCK_CONTEXT"] = json.dumps(self.context)

    def test_creates_parentless_context_branch_with_only_context_files(self) -> None:
        result = self.run_step("Persist issue context")
        self.assertEqual(result.returncode, 0, result.stderr)

        calls = self.github_calls()
        tree = next(
            call
            for call in calls
            if any("/git/trees" in argument for argument in call["arguments"])
        )
        self.assertNotIn("base_tree", tree["payload"])
        self.assertEqual(
            [entry["path"] for entry in tree["payload"]["tree"]],
            ["issue.json", "triage.json"],
        )

        commit = next(
            call
            for call in calls
            if any("/git/commits" in argument for argument in call["arguments"])
        )
        self.assertEqual(commit["payload"]["parents"], [])

        mutation = next(
            call
            for call in calls
            if call["arguments"][:2] == ["api", "graphql"]
            and "mutation(" in call["arguments"][3]
        )
        self.assertIn("name=issue/123", mutation["arguments"])
        self.assertIn("repositoryId=repository-node-id", mutation["arguments"])
        self.assertIn(f"oid={'c' * 40}", mutation["arguments"])

    def test_updates_existing_linked_context_without_force(self) -> None:
        self.existing_context()
        result = self.run_step("Persist issue context")
        self.assertEqual(result.returncode, 0, result.stderr)

        calls = self.github_calls()
        tree = next(
            call
            for call in calls
            if any("/git/trees" in argument for argument in call["arguments"])
        )
        self.assertEqual(tree["payload"]["base_tree"], "b" * 40)

        commit = next(
            call
            for call in calls
            if any("/git/commits" in argument for argument in call["arguments"])
        )
        self.assertEqual(commit["payload"]["parents"], ["a" * 40])

        update = next(call for call in calls if "PATCH" in call["arguments"])
        self.assertEqual(update["payload"], {"sha": "c" * 40, "force": False})
        self.assertFalse(
            any(
                call["arguments"][:2] == ["api", "graphql"]
                and "mutation(" in call["arguments"][3]
                for call in calls
            )
        )

    def test_does_not_commit_unchanged_context(self) -> None:
        self.existing_context()
        self.environment["MOCK_TREE_SHA"] = "b" * 40
        result = self.run_step("Persist issue context")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("unchanged", result.stdout)
        self.assertFalse(
            any(
                "/git/commits" in argument
                for call in self.github_calls()
                for argument in call["arguments"]
            )
        )

    def test_rejects_existing_branch_linked_to_another_issue(self) -> None:
        self.existing_context(linked=False)
        result = self.run_step("Persist issue context")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("not linked to the expected issue", result.stderr)
        self.assertFalse(
            any(
                "/git/trees" in argument
                for call in self.github_calls()
                for argument in call["arguments"]
            )
        )

    def test_rejects_invalid_issue_number_before_api_access(self) -> None:
        self.environment["ISSUE_NUMBER"] = "123/another-branch"
        result = self.run_step("Persist issue context")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.github_calls(), [])

    def test_validates_issue_and_triage_result_before_token_exchange(self) -> None:
        result = self.run_step("Validate issue context")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(
            json.loads((self.root / "triage.json").read_text()), self.triage
        )
        self.assertIn("number=123", (self.root / "outputs").read_text())

    def test_rejects_triage_results_with_unexpected_fields(self) -> None:
        self.environment["TRIAGE_RESULT"] = json.dumps(
            {**self.triage, "unexpected": True}
        )
        result = self.run_step("Validate issue context")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("does not match the expected schema", result.stderr)


if __name__ == "__main__":
    unittest.main()
