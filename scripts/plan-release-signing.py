# /// script
# requires-python = ">=3.12"
# dependencies = []
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Build native verification matrices for signed release artifacts.

Every macOS and Windows target in `dist-workspace.toml` needs a runner to check
its signed wheels and GitHub archive. Fail if the runner table falls out of sync
so a new release target cannot bypass verification.
"""

import json
import tomllib
from pathlib import Path

MACOS_RUNNER = "namespace-profile-macos-15"
WINDOWS_X64_RUNNER = "namespace-profile-windows-2022-x86-64-16x32"
WINDOWS_ARM64_RUNNER = "github-windows-11-aarch64-8"

# Runner assignment is explicit: adding a release target needs a native verifier.
PLATFORMS = {
    "macos": {
        "aarch64-apple-darwin": (MACOS_RUNNER, "arm64"),
        "x86_64-apple-darwin": (MACOS_RUNNER, "x64"),
    },
    "windows": {
        "aarch64-pc-windows-msvc": (WINDOWS_ARM64_RUNNER, "arm64"),
        "i686-pc-windows-msvc": (WINDOWS_X64_RUNNER, "x86"),
        "x86_64-pc-windows-msvc": (WINDOWS_X64_RUNNER, "x64"),
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
            }
            for target, (runner, architecture) in platforms.items()
        ]
        for system, platforms in PLATFORMS.items()
    }


def main() -> None:
    """Print one GitHub Actions output for each signing matrix."""
    for system, platforms in signing_plan().items():
        print(f"{system}={json.dumps(platforms, separators=(',', ':'))}")


if __name__ == "__main__":
    main()
