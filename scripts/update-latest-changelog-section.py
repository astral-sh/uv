# /// script
# requires-python = ">=3.12"
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///

"""Update the latest release while retaining its authored breaking-change notes."""

from __future__ import annotations

import argparse
import re
import subprocess
from pathlib import Path


def split_latest_release(changelog: str) -> tuple[str, str, str]:
    headings = list(re.finditer(r"^## ", changelog, re.MULTILINE))
    if not headings:
        raise ValueError("Changelog has no release heading")
    start = headings[0].start()
    end = headings[1].start() if len(headings) > 1 else len(changelog)
    return changelog[:start], changelog[start:end].rstrip(), changelog[end:]


def preserve_breaking_changes(original: str, candidate: str) -> str:
    if original.partition("\n")[0] != candidate.partition("\n")[0]:
        return candidate

    pattern = re.compile(
        r"^### Breaking changes\n.*?(?=^### |\Z)", re.MULTILINE | re.DOTALL
    )
    original_breaking = pattern.search(original)
    if original_breaking is None:
        return candidate

    preserved = original_breaking.group().rstrip()
    candidate_breaking = pattern.search(candidate)
    if candidate_breaking is not None:
        # Generated entries link their pull requests; authored entries can expand on those changes.
        pull_request = re.compile(r"https://github\.com/[^/\s)]+/[^/\s)]+/pull/\d+")
        covered = set(pull_request.findall(preserved))
        original_items = re.split(r"(?m)(?=^- )", preserved)
        for item in re.split(r"(?m)(?=^- )", candidate_breaking.group()):
            if not item.startswith("- "):
                continue
            references = set(pull_request.findall(item))
            if references and references <= covered:
                continue
            if item.strip() in {entry.strip() for entry in original_items}:
                continue
            preserved += "\n\n" + item.rstrip()
        start, end = candidate_breaking.span()
    else:
        next_section = re.search(r"^### ", candidate, re.MULTILINE)
        start = end = next_section.start() if next_section else len(candidate)

    before = candidate[:start].rstrip()
    after = candidate[end:].lstrip("\n")
    return "\n\n".join(part for part in (before, preserved, after) if part)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("changelog", type=Path)
    parser.add_argument("candidate", type=Path, nargs="?")
    parser.add_argument("--preserve-breaking-from", type=Path)
    parser.add_argument(
        "--rooster",
        nargs=argparse.REMAINDER,
        help="Generate a release with Rooster, forwarding the remaining arguments",
    )
    args = parser.parse_args()
    if (args.candidate is None) == (args.rooster is None):
        parser.error("provide either a candidate file or --rooster")

    original = args.changelog.read_text(encoding="utf-8")
    authored = (
        args.preserve_breaking_from.read_text(encoding="utf-8")
        if args.preserve_breaking_from is not None
        else original
    )
    _, original_release, _ = split_latest_release(authored)
    if args.rooster is not None:
        subprocess.run(["rooster", "release", *args.rooster], check=True)
        generated = args.changelog.read_text(encoding="utf-8")
        preamble, candidate, historical_releases = split_latest_release(generated)
    else:
        preamble, _, historical_releases = split_latest_release(original)
        candidate = args.candidate.read_text(encoding="utf-8").rstrip("\n")

    candidate = preserve_breaking_changes(original_release, candidate)
    args.changelog.write_text(
        f"{preamble}{candidate}\n\n{historical_releases}", encoding="utf-8"
    )


if __name__ == "__main__":
    main()
