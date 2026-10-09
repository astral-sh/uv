"""Regression coverage for release-note preservation."""

from __future__ import annotations

import runpy
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

SCRIPT = Path(__file__).resolve().parents[1] / "update-latest-changelog-section.py"
MODULE = runpy.run_path(str(SCRIPT))

ORIGINAL = """## 0.13.0

### Breaking changes

- **Change the default** ([#1](https://github.com/astral-sh/uv/pull/1))

  This explains the affected users.

  Set `UV_EXAMPLE=1` to opt out.
"""
HISTORY = "## 0.12.x\n\nSee the archived release notes.\n"


class ChangelogTests(unittest.TestCase):
    def test_preserves_migration_prose_and_adds_new_breaking_entries(self) -> None:
        candidate = """## 0.13.0

Released on 2026-10-09.

### Breaking changes

- Change the default ([#1](https://github.com/astral-sh/uv/pull/1))
- Another breaking change ([#2](https://github.com/astral-sh/uv/pull/2))

### Performance

- Improve cache reads.
"""
        result = MODULE["preserve_breaking_changes"](ORIGINAL, candidate)
        self.assertEqual(
            result,
            "## 0.13.0\n\nReleased on 2026-10-09.\n\n"
            + ORIGINAL.split("\n\n", 1)[1].rstrip()
            + "\n\n- Another breaking change ([#2](https://github.com/astral-sh/uv/pull/2))"
            + "\n\n### Performance\n\n- Improve cache reads.\n",
        )

    def test_restores_an_omitted_breaking_section(self) -> None:
        candidate = "## 0.13.0\n\n### Bug fixes\n\n- Fix a bug."
        self.assertEqual(
            MODULE["preserve_breaking_changes"](ORIGINAL, candidate),
            ORIGINAL.rstrip() + "\n\n### Bug fixes\n\n- Fix a bug.",
        )

    def test_does_not_carry_notes_into_another_release(self) -> None:
        candidate = "## 0.13.1\n\n### Bug fixes\n\n- Fix a bug."
        self.assertEqual(
            MODULE["preserve_breaking_changes"](ORIGINAL, candidate), candidate
        )

    def test_applies_editorial_rewrite_without_losing_history(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            changelog = Path(directory) / "CHANGELOG.md"
            candidate = Path(directory) / "candidate.md"
            changelog.write_text("# Changelog\n\n" + ORIGINAL + "\n" + HISTORY)
            candidate.write_text("## 0.13.0\n\n### Bug fixes\n\n- Fix a bug.\n")
            subprocess.run(
                [sys.executable, str(SCRIPT), str(changelog), str(candidate)],
                check=True,
            )
            self.assertEqual(
                changelog.read_text(),
                "# Changelog\n\n"
                + ORIGINAL.rstrip()
                + "\n\n### Bug fixes\n\n- Fix a bug.\n\n"
                + HISTORY,
            )

    def test_rewrites_new_entries_against_the_authored_baseline(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            changelog = Path(directory) / "CHANGELOG.md"
            baseline = Path(directory) / "before-release.md"
            candidate = Path(directory) / "candidate.md"
            baseline.write_text("# Changelog\n\n" + ORIGINAL + "\n" + HISTORY)
            generated_entry = (
                "\n- Raw PR title ([#2](https://github.com/astral-sh/uv/pull/2))\n"
            )
            changelog.write_text(
                "# Changelog\n\n" + ORIGINAL + generated_entry + "\n" + HISTORY
            )
            edited_entry = "- Explain the new behavior ([#2](https://github.com/astral-sh/uv/pull/2))"
            candidate.write_text("## 0.13.0\n\n### Breaking changes\n\n" + edited_entry)
            subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    str(changelog),
                    str(candidate),
                    "--preserve-breaking-from",
                    str(baseline),
                ],
                check=True,
            )
            self.assertEqual(
                changelog.read_text(),
                "# Changelog\n\n"
                + ORIGINAL.rstrip()
                + "\n\n"
                + edited_entry
                + "\n\n"
                + HISTORY,
            )

    def test_wraps_rooster_before_it_replaces_the_release(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            changelog = Path(directory) / "CHANGELOG.md"
            changelog.write_text("# Changelog\n\n" + ORIGINAL + "\n" + HISTORY)
            generated = (
                "# Changelog\n\n## 0.13.0\n\nReleased on 2026-10-09.\n\n" + HISTORY
            )
            with (
                patch(
                    "sys.argv",
                    [str(SCRIPT), str(changelog), "--rooster", "--version", "0.13.0"],
                ),
                patch(
                    "subprocess.run",
                    side_effect=lambda *args, **kwargs: changelog.write_text(generated),
                ) as run,
            ):
                MODULE["main"]()
            run.assert_called_once_with(
                ["rooster", "release", "--version", "0.13.0"], check=True
            )
            self.assertEqual(
                changelog.read_text(),
                "# Changelog\n\n## 0.13.0\n\nReleased on 2026-10-09.\n\n"
                + ORIGINAL.split("\n\n", 1)[1].rstrip()
                + "\n\n"
                + HISTORY,
            )


if __name__ == "__main__":
    unittest.main()
