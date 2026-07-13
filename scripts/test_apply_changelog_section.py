"""Tests for applying an editorialized changelog release section."""

import unittest

from apply_changelog_section import apply_changelog_section

CHANGELOG = """# Changelog

<!-- prettier-ignore-start -->


## 1.2.0

Released on 2026-07-13.

### Enhancements

- Add a feature ([#12](https://github.com/astral-sh/uv/pull/12))

### Other changes

- Update CI ([#13](https://github.com/astral-sh/uv/pull/13))

## 1.1.0

Released on 2026-07-01.

### Bug fixes

- Fix a bug ([#11](https://github.com/astral-sh/uv/pull/11))

<!-- prettier-ignore-end -->

"""

CANDIDATE = """## 1.2.0

Released on 2026-07-13.

### Enhancements

- Improve the feature ([#12](https://github.com/astral-sh/uv/pull/12))
"""


class ApplyChangelogSectionTest(unittest.TestCase):
    def test_replaces_only_the_newest_release(self) -> None:
        updated, dropped = apply_changelog_section(CHANGELOG, CANDIDATE)

        prefix = CHANGELOG[: CHANGELOG.index("## 1.2.0")]
        historical_releases = CHANGELOG[CHANGELOG.index("## 1.1.0") :]
        self.assertEqual(dropped, ["13"])
        self.assertEqual(
            updated,
            prefix + CANDIDATE.rstrip("\n") + "\n\n" + historical_releases,
        )

    def test_rejects_an_additional_release(self) -> None:
        candidate = CANDIDATE + "\n## 1.1.0\n"

        with self.assertRaisesRegex(ValueError, "exactly one release heading"):
            apply_changelog_section(CHANGELOG, candidate)

    def test_rejects_a_changed_release_date(self) -> None:
        candidate = CANDIDATE.replace("2026-07-13", "2026-07-14")

        with self.assertRaisesRegex(ValueError, "changed or removed the release date"):
            apply_changelog_section(CHANGELOG, candidate)

    def test_rejects_a_new_url(self) -> None:
        candidate = CANDIDATE.replace(
            "https://github.com/astral-sh/uv/pull/12",
            "https://github.com/astral-sh/uv/issues/12",
        )

        with self.assertRaisesRegex(ValueError, "new or modified URLs"):
            apply_changelog_section(CHANGELOG, candidate)

    def test_rejects_a_new_plain_url(self) -> None:
        candidate = CANDIDATE + "\nSee https://example.com for details.\n"

        with self.assertRaisesRegex(ValueError, "new or modified URLs"):
            apply_changelog_section(CHANGELOG, candidate)

    def test_rejects_a_malformed_pull_request_link(self) -> None:
        candidate = CANDIDATE.replace("[#12]", "[#21]")

        with self.assertRaisesRegex(ValueError, "malformed pull request link"):
            apply_changelog_section(CHANGELOG, candidate)

    def test_rejects_a_duplicate_pull_request_link(self) -> None:
        entry = "- Improve the feature ([#12](https://github.com/astral-sh/uv/pull/12))"
        candidate = CANDIDATE.replace(entry, f"{entry}\n{entry}")

        with self.assertRaisesRegex(ValueError, "duplicate pull request links"):
            apply_changelog_section(CHANGELOG, candidate)


if __name__ == "__main__":
    unittest.main()
