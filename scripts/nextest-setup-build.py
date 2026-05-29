#!/usr/bin/env python3
"""Build and expose the uv executable used by integration tests."""

from __future__ import annotations

import os
from pathlib import Path

from build_test_uv import build_uv


def main() -> None:
    executable = os.environ.get("UV_TEST_BIN")
    if executable is None:
        executable = str(build_uv(os.environ.get("CARGO", "cargo")))

    with Path(os.environ["NEXTEST_ENV"]).open("a", encoding="utf-8") as environment:
        environment.write(f"UV_TEST_BIN={executable}\n")


if __name__ == "__main__":
    main()
