# /// script
# requires-python = ">=3.12"
# dependencies = []
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Reject external default-feature requests in the uv workspace."""

from __future__ import annotations

import json
import subprocess
import sys
import tomllib
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent


def inspect(
    metadata: dict[str, Any], workspace_dependencies: dict[str, Any]
) -> list[str]:
    root = Path(metadata["workspace_root"]).resolve()
    member_ids = set(metadata["workspace_members"])
    members = [
        package for package in metadata["packages"] if package["id"] in member_ids
    ]
    member_paths = {
        Path(package["manifest_path"]).resolve().parent for package in members
    }
    errors: set[str] = set()

    def is_external(path: str | None) -> bool:
        return path is None or (root / path).resolve() not in member_paths

    # Metadata omits unused workspace dependency definitions.
    for name, specification in workspace_dependencies.items():
        dependency = {} if isinstance(specification, str) else specification
        if is_external(dependency.get("path")) and (
            dependency.get("default-features", True)
            or "default" in dependency.get("features", [])
        ):
            errors.add(
                f"Cargo.toml: workspace.dependencies.{name} requests default features"
            )

    for package in members:
        manifest = Path(package["manifest_path"]).resolve()
        location = (
            manifest.relative_to(root) if manifest.is_relative_to(root) else manifest
        )
        external_aliases = set()

        # Cargo has already applied workspace inheritance and target-specific rules.
        for dependency in package["dependencies"]:
            if not is_external(dependency.get("path")):
                continue
            alias = dependency.get("rename") or dependency["name"]
            external_aliases.add(alias)
            if (
                dependency["uses_default_features"]
                or "default" in dependency["features"]
            ):
                errors.add(
                    f"{location}: dependency {alias!r} requests default features"
                )

        for feature, activations in package["features"].items():
            for activation in activations:
                alias, separator, target = activation.partition("/")
                if (
                    separator
                    and target == "default"
                    and alias.removesuffix("?") in external_aliases
                ):
                    errors.add(
                        f"{location}: features.{feature} requests {activation!r}"
                    )

    return sorted(errors)


def main() -> int:
    try:
        metadata = json.loads(
            subprocess.check_output(
                [
                    "cargo",
                    "metadata",
                    "--locked",
                    "--offline",
                    "--no-deps",
                    "--format-version",
                    "1",
                ],
                cwd=ROOT,
                text=True,
            )
        )
    except subprocess.CalledProcessError as error:
        return error.returncode

    with (Path(metadata["workspace_root"]) / "Cargo.toml").open("rb") as manifest:
        workspace_dependencies = (
            tomllib.load(manifest).get("workspace", {}).get("dependencies", {})
        )

    errors = inspect(metadata, workspace_dependencies)
    for error in errors:
        print(f"error: {error}", file=sys.stderr)
    if errors:
        print(
            "Set default-features = false and request named features explicitly.",
            file=sys.stderr,
        )
        return 1

    print("Workspace external dependencies request no default features.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
