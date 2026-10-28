# /// script
# requires-python = ">=3.11"
# dependencies = []
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# [tool.ty.environment]
# extra-paths = ["."]
# ///
"""Regression tests for the Cargo dependency cooldown gate."""

from __future__ import annotations

import io
import json
import subprocess
import tempfile
import unittest
from contextlib import chdir, redirect_stdout
from datetime import UTC, datetime, timedelta
from email.message import Message
from pathlib import Path
from unittest.mock import patch
from urllib.error import HTTPError, URLError

import check_cargo_cooldowns as cooldowns

NOW = datetime(2026, 10, 28, 12, tzinfo=UTC)
SOURCE = "registry+https://github.com/rust-lang/crates.io-index"
PACKAGE = cooldowns.Package("example", "1.2.3", SOURCE)
POLICY = cooldowns.Policy(timedelta(days=7), [])


def response(published: str = "2026-10-21T12:00:00Z") -> io.BytesIO:
    return io.BytesIO(
        json.dumps(
            {
                "version": {
                    "crate": "example",
                    "num": "1.2.3",
                    "created_at": published,
                    "updated_at": "2026-10-28T12:00:00Z",
                }
            }
        ).encode()
    )


class PublicationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.output = io.StringIO()
        self.enterContext(redirect_stdout(self.output))
        self.enterContext(patch.object(cooldowns.time, "sleep"))

    def test_exact_cooldown_boundary_passes_despite_recent_metadata_update(
        self,
    ) -> None:
        with patch.object(cooldowns, "urlopen", return_value=response()):
            self.assertEqual(
                cooldowns.check_packages({PACKAGE: {"Cargo.lock"}}, POLICY, NOW), 0
            )

    def test_recent_release_fails_with_the_eligibility_time(self) -> None:
        with patch.object(
            cooldowns, "urlopen", return_value=response("2026-10-21T12:00:01Z")
        ):
            self.assertEqual(
                cooldowns.check_packages({PACKAGE: {"Cargo.lock"}}, POLICY, NOW), 1
            )
        self.assertEqual(
            self.output.getvalue(),
            "::error::example 1.2.3 in 'Cargo.lock' was published at "
            "2026-10-21T12:00:01+00:00; its 7-day cooldown ends at "
            "2026-10-28T12:00:01+00:00. Re-run CI after that time or add a reviewed "
            "exclusion to .github/cargo-cooldowns.toml.\n",
        )

    def test_package_exclusion_does_not_need_registry_access(self) -> None:
        policy = cooldowns.Policy(
            timedelta(days=7),
            [cooldowns.Exclusion("example", None, "Maintained in-house")],
        )
        with patch.object(cooldowns, "urlopen") as request:
            self.assertEqual(
                cooldowns.check_packages({PACKAGE: {"Cargo.lock"}}, policy, NOW), 0
            )
        request.assert_not_called()

    def test_version_exclusion_does_not_allow_the_next_release(self) -> None:
        policy = cooldowns.Policy(
            timedelta(days=7), [cooldowns.Exclusion("example", "1.2.2", "Urgent fix")]
        )
        with patch.object(
            cooldowns, "urlopen", return_value=response("2026-10-28T11:00:00Z")
        ):
            self.assertEqual(
                cooldowns.check_packages({PACKAGE: {"Cargo.lock"}}, policy, NOW), 1
            )

    def test_matching_version_exclusion_passes(self) -> None:
        policy = cooldowns.Policy(
            timedelta(days=7), [cooldowns.Exclusion("example", "1.2.3", "Urgent fix")]
        )
        with patch.object(cooldowns, "urlopen") as request:
            self.assertEqual(
                cooldowns.check_packages({PACKAGE: {"Cargo.lock"}}, policy, NOW), 0
            )
        request.assert_not_called()

    def test_missing_publication_time_fails_closed(self) -> None:
        payload = io.BytesIO(b'{"version": {"crate": "example", "num": "1.2.3"}}')
        with patch.object(cooldowns, "urlopen", return_value=payload):
            self.assertEqual(
                cooldowns.check_packages({PACKAGE: {"Cargo.lock"}}, POLICY, NOW), 1
            )

    def test_naive_publication_time_fails_closed(self) -> None:
        with patch.object(
            cooldowns, "urlopen", return_value=response("2026-10-01T12:00:00")
        ):
            self.assertEqual(
                cooldowns.check_packages({PACKAGE: {"Cargo.lock"}}, POLICY, NOW), 1
            )

    def test_metadata_for_another_version_fails_closed(self) -> None:
        payload = io.BytesIO(
            b'{"version": {"crate": "example", "num": "1.2.2", '
            b'"created_at": "2026-01-01T00:00:00Z"}}'
        )
        with patch.object(cooldowns, "urlopen", return_value=payload):
            self.assertEqual(
                cooldowns.check_packages({PACKAGE: {"Cargo.lock"}}, POLICY, NOW), 1
            )

    def test_transient_registry_failure_is_retried(self) -> None:
        with patch.object(
            cooldowns, "urlopen", side_effect=[URLError("Connection reset"), response()]
        ) as request:
            self.assertEqual(
                cooldowns.check_packages({PACKAGE: {"Cargo.lock"}}, POLICY, NOW), 0
            )
        self.assertEqual(request.call_count, 2)

    def test_registry_outage_fails_closed_after_retries(self) -> None:
        with patch.object(
            cooldowns, "urlopen", side_effect=URLError("Unavailable")
        ) as request:
            self.assertEqual(
                cooldowns.check_packages({PACKAGE: {"Cargo.lock"}}, POLICY, NOW), 1
            )
        self.assertEqual(request.call_count, 3)

    def test_unknown_version_is_not_retried(self) -> None:
        with patch.object(
            cooldowns,
            "urlopen",
            side_effect=HTTPError(
                "https://crates.io", 404, "Not found", Message(), None
            ),
        ) as request:
            self.assertEqual(
                cooldowns.check_packages({PACKAGE: {"Cargo.lock"}}, POLICY, NOW), 1
            )
        self.assertEqual(request.call_count, 1)

    def test_unrecognized_registry_is_not_silently_accepted(self) -> None:
        package = cooldowns.Package("example", "1.2.3", "registry+https://example.com")
        with patch.object(cooldowns, "urlopen") as request:
            self.assertEqual(
                cooldowns.check_packages({package: {"Cargo.lock"}}, POLICY, NOW), 1
            )
        request.assert_not_called()


class LockfileTests(unittest.TestCase):
    def setUp(self) -> None:
        directory = self.enterContext(tempfile.TemporaryDirectory())
        self.enterContext(chdir(directory))
        self.git("init", "--quiet", "--initial-branch=main")
        self.git("config", "user.email", "test@example.com")
        self.git("config", "user.name", "Test")
        Path("Cargo.toml").write_text("[workspace]\nmembers = []\n")
        Path("Cargo.lock").write_text(
            'version = 4\n\n[[package]]\nname = "example"\nversion = "1.2.2"\n'
            f'source = "{SOURCE}"\n'
        )
        Path(".github").mkdir()
        Path(".github/cargo-cooldowns.toml").write_text(
            "cooldown-days = 7\nexclusions = []\n"
        )
        self.base = self.commit()

    def git(self, *arguments: str) -> str:
        return subprocess.check_output(["git", *arguments], text=True).strip()

    def commit(self) -> str:
        self.git("add", ".")
        self.git("commit", "--quiet", "-m", "Fixture")
        return self.git("rev-parse", "HEAD")

    def test_manual_transitive_update_is_checked_once_across_workspaces(self) -> None:
        # Only the resolved version changes; no manifest update is needed to trigger the gate.
        updated = Path("Cargo.lock").read_text().replace('"1.2.2"', '"1.2.3"')
        Path("Cargo.lock").write_text(updated)
        Path("crates/another workspace").mkdir(parents=True)
        Path("crates/another workspace/Cargo.lock").write_text(updated)
        head = self.commit()
        added = cooldowns.added_packages(self.base, head)
        self.assertEqual(
            added,
            {PACKAGE: {"Cargo.lock", "crates/another workspace/Cargo.lock"}},
        )
        with (
            patch.object(
                cooldowns, "urlopen", return_value=response("2026-10-28T00:00:00Z")
            ) as request,
            redirect_stdout(io.StringIO()),
        ):
            self.assertEqual(cooldowns.check_packages(added, POLICY, NOW), 1)
        request.assert_called_once()

    def test_new_lockfile_is_checked(self) -> None:
        Path("crates/new").mkdir(parents=True)
        Path("crates/new/Cargo.lock").write_text(
            Path("Cargo.lock").read_text().replace('"1.2.2"', '"1.2.3"')
        )
        head = self.commit()
        self.assertEqual(
            cooldowns.added_packages(self.base, head),
            {PACKAGE: {"crates/new/Cargo.lock"}},
        )

    def test_unchanged_versions_and_git_or_workspace_packages_need_no_lookup(
        self,
    ) -> None:
        with Path("Cargo.lock").open("a") as lockfile:
            lockfile.write(
                '\n[[package]]\nname = "workspace"\nversion = "1.0.0"\n'
                '\n[[package]]\nname = "git-dependency"\nversion = "2.0.0"\n'
                'source = "git+https://example.com/repo#012345"\n'
            )
        head = self.commit()
        with (
            patch.object(cooldowns, "urlopen") as request,
            redirect_stdout(io.StringIO()),
        ):
            self.assertEqual(
                cooldowns.check_packages(
                    cooldowns.added_packages(self.base, head), POLICY, NOW
                ),
                0,
            )
        request.assert_not_called()

    def test_removing_a_workspace_lockfile_fails(self) -> None:
        Path("Cargo.lock").unlink()
        head = self.commit()
        with self.assertRaisesRegex(ValueError, "missing its lockfile"):
            cooldowns.added_packages(self.base, head)

    def test_removing_an_entire_workspace_passes(self) -> None:
        Path("Cargo.lock").unlink()
        Path("Cargo.toml").unlink()
        head = self.commit()
        self.assertEqual(cooldowns.added_packages(self.base, head), {})

    def test_updates_from_the_base_branch_are_not_attributed_to_the_pull_request(
        self,
    ) -> None:
        self.git("checkout", "--quiet", "-b", "pull-request")
        Path("README.md").write_text("An unrelated change\n")
        self.commit()
        self.git("checkout", "--quiet", "main")
        Path("Cargo.lock").write_text(
            Path("Cargo.lock").read_text().replace('"1.2.2"', '"1.2.3"')
        )
        self.commit()
        self.git("merge", "--quiet", "--no-ff", "pull-request", "-m", "Merge fixture")
        self.assertEqual(cooldowns.added_packages("HEAD^1", "HEAD"), {})

    def test_policy_loads_package_and_version_exclusions(self) -> None:
        Path(".github/cargo-cooldowns.toml").write_text(
            "cooldown-days = 7\n"
            '[[exclusions]]\nname = "example"\nreason = "Maintained in-house"\n'
            '[[exclusions]]\nname = "another"\nversion = "2.0.0"\nreason = "Urgent fix"\n'
        )
        self.assertEqual(
            cooldowns.load_policy(Path(".github/cargo-cooldowns.toml")),
            cooldowns.Policy(
                timedelta(days=7),
                [
                    cooldowns.Exclusion("example", None, "Maintained in-house"),
                    cooldowns.Exclusion("another", "2.0.0", "Urgent fix"),
                ],
            ),
        )

    def test_misspelled_exclusion_version_is_rejected(self) -> None:
        # A typo must not turn a version-specific exclusion into a package-wide exemption.
        Path(".github/cargo-cooldowns.toml").write_text(
            'cooldown-days = 7\n[[exclusions]]\nname = "example"\n'
            'versions = "1.2.3"\nreason = "Urgent fix"\n'
        )
        with self.assertRaisesRegex(ValueError, "optional `version`"):
            cooldowns.load_policy(Path(".github/cargo-cooldowns.toml"))

    def test_cli_fails_on_malformed_policy_even_without_dependency_changes(
        self,
    ) -> None:
        Path(".github/cargo-cooldowns.toml").write_text(
            "cooldown-days = false\nexclusions = []\n"
        )
        with (
            patch("sys.argv", ["check_cargo_cooldowns.py", self.base]),
            redirect_stdout(io.StringIO()),
        ):
            self.assertEqual(cooldowns.main(), 1)


if __name__ == "__main__":
    unittest.main()
