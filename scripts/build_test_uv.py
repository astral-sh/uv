#!/usr/bin/env python3
"""Build the uv executable used by integration tests."""

from __future__ import annotations

import json
import subprocess
from collections.abc import Sequence
from pathlib import Path


def build_uv(cargo: str, options: Sequence[str] = ()) -> Path:
    command = [
        cargo,
        "build",
        "--workspace",
        "--bin",
        "uv",
        "--message-format=json-render-diagnostics",
        "--features",
        "uv-publish/test",
        *options,
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
