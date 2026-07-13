# /// script
# requires-python = ">=3.12"
# ///

"""Apply an editorialized changelog release section."""

from __future__ import annotations

import argparse
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("changelog", type=Path)
    parser.add_argument("candidate", type=Path)
    args = parser.parse_args()

    changelog = args.changelog.read_text(encoding="utf-8")
    candidate = args.candidate.read_text(encoding="utf-8").rstrip("\n")
    preamble, _, historical_releases = changelog.split("\n## ", maxsplit=2)
    args.changelog.write_text(
        f"{preamble}\n{candidate}\n\n## {historical_releases}",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
