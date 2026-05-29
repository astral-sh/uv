#!/usr/bin/env python3
"""Build uv, then run its integration tests against the resulting executable."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path


def build_options(nextest_options: list[str]) -> list[str]:
    options: list[str] = []
    translated_option: str | None = None

    for option in nextest_options:
        if translated_option is not None:
            options.append(translated_option)
            options.append(option)
            translated_option = None
        elif option == "--":
            break
        elif option == "--cargo-profile":
            translated_option = "--profile"
        elif option == "--build-jobs":
            translated_option = "--jobs"
        elif option in {
            "--config",
            "--features",
            "--manifest-path",
            "--target",
            "--target-dir",
            "-Z",
        }:
            translated_option = option
        elif option.startswith(
            (
                "--config=",
                "--features=",
                "--manifest-path=",
                "--target=",
                "--target-dir=",
            )
        ):
            options.append(option)
        elif option in {
            "--all-features",
            "--frozen",
            "--ignore-rust-version",
            "--locked",
            "--no-default-features",
            "--offline",
            "--release",
        }:
            options.append(option)

    if translated_option is not None:
        raise ValueError(f"{translated_option} requires a value")

    return options


def build_uv(cargo: str, nextest_options: list[str]) -> Path:
    command = [
        cargo,
        "build",
        "--workspace",
        "--bin",
        "uv",
        "--message-format=json-render-diagnostics",
        "--features",
        "uv-publish/test",
        *build_options(nextest_options),
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


def main() -> int:
    cargo = os.environ.get("CARGO", "cargo")
    nextest_options = sys.argv[1:]
    if not any(
        option == "--workspace"
        or option in {"-p", "--package"}
        or option.startswith("--package=")
        for option in nextest_options
    ):
        nextest_options = [
            "--package",
            "uv-integration",
            *nextest_options,
        ]

    executable = build_uv(cargo, nextest_options)
    environment = os.environ.copy()
    environment["UV_TEST_BIN"] = str(executable)
    return subprocess.run(
        [cargo, "nextest", "run", *nextest_options],
        env=environment,
        check=False,
    ).returncode


if __name__ == "__main__":
    raise SystemExit(main())
