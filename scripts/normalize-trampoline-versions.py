#!/usr/bin/env -S uv run --locked --script
# /// script
# requires-python = ">=3.12"
# dependencies = ["tomli-w"]
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///

import tomllib
from pathlib import Path

import tomli_w


def main() -> None:
    workspace = Path("/workspace")

    for crate in ("uv-trampoline", "uv-windows"):
        path = workspace / "crates" / crate / "Cargo.toml"
        manifest = tomllib.loads(path.read_text(encoding="utf-8"))
        manifest["package"]["version"] = "0.0.0"
        path.write_text(tomli_w.dumps(manifest), encoding="utf-8")

    lockfile = workspace / "crates/uv-trampoline/Cargo.lock"
    lock = tomllib.loads(lockfile.read_text(encoding="utf-8"))

    for package in lock["package"]:
        if "source" not in package:
            package["version"] = "0.0.0"

    lockfile.write_text(tomli_w.dumps(lock), encoding="utf-8")


if __name__ == "__main__":
    main()
