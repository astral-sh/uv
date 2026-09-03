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

# These checksums are for setup-uv's pinned uv 0.12.10 on the runner, not the
# architecture of the release binary being checked.
MACOS_UV_CHECKSUM = "51c6170e8e3a01cef9f33b94f582b7b81ac65046f55d40afb35f9cff5a68c179"
WINDOWS_X64_UV_CHECKSUM = (
    "f65744f94072152b1f86ba2aace4d01f1124d9a8ecb235805039e3718c36cac2"
)
WINDOWS_ARM64_UV_CHECKSUM = (
    "ee985c51c0c9c1f82267a5d80f959b34a7ff888c109182bd3b2b35c4661bbcde"
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
