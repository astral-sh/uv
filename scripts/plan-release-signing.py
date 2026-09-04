# /// script
# requires-python = ">=3.12"
# dependencies = []
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Describe the native verification jobs for every signed release target."""

import json
import tomllib
from pathlib import Path

MACOS_RUNNER = "namespace-profile-macos-15"
WINDOWS_X64_RUNNER = "namespace-profile-windows-2022-x86-64-16x32"
WINDOWS_ARM64_RUNNER = "github-windows-11-aarch64-8"

# These checksums are for setup-uv's pinned uv 0.12.7 on the runner, not the
# architecture of the release binary being checked.
MACOS_UV_CHECKSUM = "127ebdda7ad953cdf198e964b570ea5771b85467ea93eb7cb6d6f8e6f55408f3"
WINDOWS_X64_UV_CHECKSUM = (
    "bf1518af459a3915511a11fdc6e2f43ef9a2afa138b9d498eeb9642fe9d85218"
)
WINDOWS_ARM64_UV_CHECKSUM = (
    "1611d0f4be72b0a354ad9a6ae954093dd4c91e93e36b8b490326a05a039ffe14"
)

PLATFORMS = {
    "macos": {
        "aarch64-apple-darwin": (MACOS_RUNNER, "arm64", MACOS_UV_CHECKSUM),
        "x86_64-apple-darwin": (MACOS_RUNNER, "x64", MACOS_UV_CHECKSUM),
    },
    "windows": {
        "aarch64-pc-windows-msvc": (
            WINDOWS_ARM64_RUNNER,
            "arm64",
            WINDOWS_ARM64_UV_CHECKSUM,
        ),
        "i686-pc-windows-msvc": (WINDOWS_X64_RUNNER, "x86", WINDOWS_X64_UV_CHECKSUM),
        "x86_64-pc-windows-msvc": (WINDOWS_X64_RUNNER, "x64", WINDOWS_X64_UV_CHECKSUM),
    },
}


def signing_plan() -> dict[str, list[dict[str, str]]]:
    """Require every macOS and Windows release target to have a verification job."""
    workspace = Path(__file__).resolve().parent.parent / "dist-workspace.toml"
    targets = tomllib.loads(workspace.read_text(encoding="utf-8"))["dist"]["targets"]
    expected = {
        target
        for target in targets
        if target.endswith(("-apple-darwin", "-pc-windows-msvc"))
    }
    configured = {target for platforms in PLATFORMS.values() for target in platforms}
    if configured != expected:
        raise ValueError(
            f"Signing targets differ from dist-workspace.toml: {configured ^ expected}"
        )
    return {
        system: [
            {
                "target": target,
                "runner": runner,
                "python-architecture": architecture,
                "uv-checksum": checksum,
            }
            for target, (runner, architecture, checksum) in platforms.items()
        ]
        for system, platforms in PLATFORMS.items()
    }


def main() -> None:
    """Print the signing matrices as one JSON workflow output."""
    print(json.dumps(signing_plan(), separators=(",", ":")))


if __name__ == "__main__":
    main()
