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

PACKAGES = {
    "macos": {"uv": ["uv", "uvx"], "uv_build": ["uv-build"]},
    "windows": {
        "uv": ["uv.exe", "uvx.exe", "uvw.exe"],
        "uv_build": ["uv-build.exe"],
    },
}


def artifact_inputs(system: str, target: str) -> dict:
    """Declare uv's wheel packages and exact GitHub archive members once per target."""
    extension = "tar.gz" if system == "macos" else "zip"
    directory = f"uv-{target}"
    return {
        "system": system,
        "wheels": [
            {
                "package": package,
                "directory": f"wheel-artifacts/wheels_{package}-{target}",
                "binaries": binaries,
            }
            for package, binaries in PACKAGES[system].items()
        ],
        "github-archive": f"github-archives/{directory}.{extension}",
        "github-archive-members": [
            f"{directory}/{binary}" if system == "macos" else binary
            for binary in PACKAGES[system]["uv"]
        ],
    }


def signing_plan() -> dict[str, list[dict]]:
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
                **artifact_inputs(system, target),
            }
            for target, (runner, architecture) in platforms.items()
        ]
        for system, platforms in PLATFORMS.items()
    }


def main() -> None:
    """Print native verification matrices and their combined artifact declarations."""
    plan = signing_plan()
    for system, platforms in plan.items():
        print(f"{system}={json.dumps(platforms, separators=(',', ':'))}")
    artifacts = [target for platforms in plan.values() for target in platforms]
    print(f"artifacts={json.dumps(artifacts, separators=(',', ':'))}")


if __name__ == "__main__":
    main()
