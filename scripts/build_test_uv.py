#!/usr/bin/env python3
"""Build the uv executable used by integration tests."""

from __future__ import annotations

import json
import os
import shlex
import subprocess
from collections.abc import Sequence
from pathlib import Path

_CACHE_ENVIRONMENT_VARIABLES = (
    "CARGO_BUILD_TARGET",
    "CARGO_ENCODED_RUSTFLAGS",
    "CARGO_TARGET_DIR",
    "MACOSX_DEPLOYMENT_TARGET",
    "RUSTC",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "RUSTFLAGS",
    "RUSTUP_TOOLCHAIN",
)


def find_cached_uv(cargo: str, options: Sequence[str] = ()) -> Path | None:
    """Return a previously built uv executable if its inputs are unchanged."""
    if os.name == "nt":
        return None

    cache = _cache_path()
    try:
        metadata = json.loads(cache.read_text(encoding="utf-8"))
        checked_at = cache.stat().st_mtime_ns
    except (FileNotFoundError, json.JSONDecodeError, OSError):
        return None

    if metadata.get("signature") != _cache_signature(cargo, options):
        return None

    configuration_inputs = _configuration_inputs()
    if metadata.get("configuration-inputs") != [
        str(path) for path in configuration_inputs
    ]:
        return None

    try:
        executable = Path(metadata["executable"])
    except (KeyError, TypeError):
        return None
    dependency_file = executable.with_suffix(".d")
    try:
        dependency_inputs = _dependency_inputs(dependency_file)
    except (FileNotFoundError, OSError, ValueError):
        return None

    for path in [
        executable,
        dependency_file,
        *configuration_inputs,
        *dependency_inputs,
    ]:
        try:
            if path.stat().st_mtime_ns > checked_at:
                return None
        except OSError:
            return None

    return executable


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

    _write_cache(executable, cargo, options)
    return executable


def _cache_path() -> Path:
    return Path(os.environ.get("CARGO_TARGET_DIR", "target")) / ".uv-test-bin.json"


def _cache_signature(cargo: str, options: Sequence[str]) -> dict[str, object]:
    return {
        "cargo": cargo,
        "options": list(options),
        "workspace": str(Path.cwd().resolve()),
        "environment": {
            name: os.environ.get(name) for name in _CACHE_ENVIRONMENT_VARIABLES
        },
    }


def _configuration_inputs() -> list[Path]:
    inputs = [
        Path("Cargo.lock"),
        Path("Cargo.toml"),
        Path(".cargo/config"),
        Path(".cargo/config.toml"),
        Path("rust-toolchain"),
        Path("rust-toolchain.toml"),
        *Path("crates").glob("*/Cargo.toml"),
    ]
    return sorted(path for path in inputs if path.exists())


def _dependency_inputs(dependency_file: Path) -> list[Path]:
    _, separator, dependencies = dependency_file.read_text(encoding="utf-8").partition(
        ": "
    )
    if not separator:
        raise ValueError(f"Invalid dependency file: {dependency_file}")
    return [Path(dependency) for dependency in shlex.split(dependencies)]


def _write_cache(executable: Path, cargo: str, options: Sequence[str]) -> None:
    cache = _cache_path()
    cache.parent.mkdir(parents=True, exist_ok=True)
    temporary = cache.with_suffix(".tmp")
    temporary.write_text(
        json.dumps(
            {
                "executable": str(executable.resolve()),
                "signature": _cache_signature(cargo, options),
                "configuration-inputs": [str(path) for path in _configuration_inputs()],
            }
        ),
        encoding="utf-8",
    )
    temporary.replace(cache)
