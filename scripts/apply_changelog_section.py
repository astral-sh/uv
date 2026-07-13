#!/usr/bin/env python3
"""Validate and apply an editorialized changelog release section."""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

RELEASE_HEADING_PATTERN = re.compile(r"^## .+$", re.MULTILINE)
RELEASE_DATE_PATTERN = re.compile(r"^Released on .+\.$", re.MULTILINE)
URL_PATTERN = re.compile(r"https://[^\s)>]+")
PULL_REQUEST_LINK_PATTERN = re.compile(
    r"\[#(?P<number>\d+)\]\("
    r"(?P<url>https://github\.com/astral-sh/uv/pull/(?P=number))"
    r"\)"
)
PULL_REQUEST_NUMBER_PATTERN = re.compile(r"\[#\d+\]")
PULL_REQUEST_URL_PATTERN = re.compile(r"https://github\.com/astral-sh/uv/pull/\d+")


def apply_changelog_section(changelog: str, candidate: str) -> tuple[str, list[str]]:
    """Replace the newest release section after validating the candidate."""
    release_headings = list(RELEASE_HEADING_PATTERN.finditer(changelog))
    if len(release_headings) < 2:
        raise ValueError("the changelog must contain at least two release headings")

    first_heading = release_headings[0].group()
    first_release_start = release_headings[0].start()
    second_release_start = release_headings[1].start()
    original_section = changelog[first_release_start:second_release_start]

    candidate_headings = RELEASE_HEADING_PATTERN.findall(candidate)
    if candidate_headings != [first_heading]:
        raise ValueError(
            f"the candidate must contain exactly one release heading: {first_heading}"
        )
    if not candidate.startswith(f"{first_heading}\n"):
        raise ValueError("the candidate must begin with the newest release heading")
    if "```" in candidate:
        raise ValueError("the candidate must not contain a code fence")

    release_date = RELEASE_DATE_PATTERN.search(original_section)
    if release_date is None:
        raise ValueError("the newest release section must contain a release date")
    if RELEASE_DATE_PATTERN.findall(candidate) != [release_date.group()]:
        raise ValueError("the candidate changed or removed the release date")

    original_urls = set(URL_PATTERN.findall(original_section))
    candidate_urls = set(URL_PATTERN.findall(candidate))
    if unknown_urls := candidate_urls - original_urls:
        raise ValueError(
            f"the candidate contains new or modified URLs: {sorted(unknown_urls)}"
        )

    original_pull_requests = PULL_REQUEST_LINK_PATTERN.findall(original_section)
    candidate_pull_requests = PULL_REQUEST_LINK_PATTERN.findall(candidate)
    if len(candidate_pull_requests) != len(set(candidate_pull_requests)):
        raise ValueError("the candidate contains duplicate pull request links")
    if len(PULL_REQUEST_NUMBER_PATTERN.findall(candidate)) != len(
        candidate_pull_requests
    ) or len(PULL_REQUEST_URL_PATTERN.findall(candidate)) != len(
        candidate_pull_requests
    ):
        raise ValueError("the candidate contains a malformed pull request link")
    if original_pull_requests and not candidate_pull_requests:
        raise ValueError("the candidate must retain at least one pull request")
    if unknown_pull_requests := set(candidate_pull_requests) - set(
        original_pull_requests
    ):
        raise ValueError(
            "the candidate contains new or modified pull request links: "
            f"{sorted(unknown_pull_requests)}"
        )

    normalized_candidate = candidate.rstrip("\n") + "\n\n"
    updated_changelog = (
        changelog[:first_release_start]
        + normalized_candidate
        + changelog[second_release_start:]
    )
    candidate_pull_request_set = set(candidate_pull_requests)
    dropped_pull_requests = [
        number
        for number, url in original_pull_requests
        if (number, url) not in candidate_pull_request_set
    ]
    return updated_changelog, dropped_pull_requests


def main(argv: list[str] | None = None) -> int:
    """Validate and apply a candidate changelog section."""
    parser = argparse.ArgumentParser()
    parser.add_argument("changelog", type=Path)
    parser.add_argument("candidate", type=Path)
    args = parser.parse_args(argv)

    changelog = args.changelog.read_text(encoding="utf-8")
    candidate = args.candidate.read_text(encoding="utf-8")
    try:
        updated_changelog, dropped_pull_requests = apply_changelog_section(
            changelog, candidate
        )
    except ValueError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    args.changelog.write_text(updated_changelog, encoding="utf-8")
    if dropped_pull_requests:
        dropped = ", ".join(f"#{number}" for number in dropped_pull_requests)
        print(f"Dropped pull requests: {dropped}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
