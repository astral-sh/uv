#!/usr/bin/env python3
"""Build and expose the uv executable used by integration tests."""

from __future__ import annotations

import json
import os
import shlex
import subprocess
from pathlib import Path


def build_uv() -> Path:
    command = [
        os.environ.get("CARGO", "cargo"),
        "test",
        "--workspace",
        "--test",
        "build_uv",
        "--no-run",
        "--message-format=json-render-diagnostics",
        "--features",
        "uv-publish/test",
        *shlex.split(os.environ.get("UV_TEST_UV_BUILD_ARGS", "")),
    ]
    process = subprocess.Popen(command, stdout=subprocess.PIPE, text=True)
    executable: Path | None = None

    if process.stdout is None:
        raise RuntimeError("Cargo stdout is not available")
    for line in process.stdout:
        message = json.loads(line)
        if (
            message.get("reason") == "compiler-artifact"
            and message["target"]["name"] == "uv"
            and "bin" in message["target"]["kind"]
            and message.get("executable")
        ):
            executable = Path(message["executable"])

    if process.wait() != 0:
        raise subprocess.CalledProcessError(process.returncode, command)
    if executable is None:
        raise RuntimeError("Cargo did not report a uv executable")

    return executable


def main() -> None:
    executable = os.environ.get("UV_TEST_BIN")
    if executable is None:
        executable = str(build_uv())

    with Path(os.environ["NEXTEST_ENV"]).open("a", encoding="utf-8") as environment:
        environment.write(f"UV_TEST_BIN={executable}\n")


if __name__ == "__main__":
    main()
