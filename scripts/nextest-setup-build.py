#!/usr/bin/env python3
"""Build and expose the uv executable used by integration tests."""

from __future__ import annotations

import os
import shlex
from pathlib import Path

from build_test_uv import build_uv, find_cached_uv


def main() -> None:
    executable = os.environ.get("UV_TEST_BIN")
    if executable is None:
        options = shlex.split(os.environ.get("UV_TEST_UV_BUILD_ARGS", ""))
        cargo = os.environ.get("CARGO", "cargo")
        executable = str(find_cached_uv(cargo, options) or build_uv(cargo, options))

    with Path(os.environ["NEXTEST_ENV"]).open("a", encoding="utf-8") as environment:
        environment.write(f"UV_TEST_BIN={executable}\n")


if __name__ == "__main__":
    main()
