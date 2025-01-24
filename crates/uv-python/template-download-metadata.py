# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "chevron-blue < 1",
# ]
# ///
"""
Generate static Rust code from Python version download metadata.

Generates the `downloads.inc` file from the `downloads.inc.mustache` template.

Usage:

    uv run -- crates/uv-python/template-download-metadata.py
"""

import argparse
import json
import logging
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

import chevron_blue

CRATE_ROOT = Path(__file__).parent
WORKSPACE_ROOT = CRATE_ROOT.parent.parent
VERSION_METADATA = CRATE_ROOT / "download-metadata.json"
TEMPLATE = CRATE_ROOT / "src" / "downloads.inc.mustache"
TARGET = TEMPLATE.with_suffix("")
PRERELEASE_PATTERN = re.compile(r"(a|b|rc)(\d+)")


def prepare_name(name: str) -> str:
    match name:
        case "cpython":
            return "CPython"
        case "pypy":
            return "PyPy"
        case _:
            raise ValueError(f"Unknown implementation name: {name}")


def prepare_libc(libc: str) -> str | None:
    if libc == "none":
        return None
    else:
        return libc.title()


def prepare_variant(variant: str | None) -> str | None:
    match variant:
        case None:
            return "PythonVariant::Default"
        case "freethreaded":
            return "PythonVariant::Freethreaded"
        case "debug":
            return "PythonVariant::Debug"
        case _:
            raise ValueError(f"Unknown variant: {variant}")


def prepare_arch(arch: dict) -> tuple[str, str]:
    match arch["family"]:
        # Special constructors
        case "i686":
            family = "X86_32(target_lexicon::X86_32Architecture::I686)"
        case "aarch64":
            family = "Aarch64(target_lexicon::Aarch64Architecture::Aarch64)"
        case "armv5tel":
            family = "Arm(target_lexicon::ArmArchitecture::Armv5te)"
        case "armv7":
            family = "Arm(target_lexicon::ArmArchitecture::Armv7)"
        case "riscv64":
            # The `gc` variant of riscv64 is the common base instruction set and
            # is the target in `python-build-standalone`
            # See https://github.com/astral-sh/python-build-standalone/issues/504
            family = "Riscv64(target_lexicon::Riscv64Architecture::Riscv64gc)"
        case value:
            family = value.capitalize()
    variant = (
        f"Some(ArchVariant::{arch['variant'].capitalize()})"
        if arch["variant"]
        else "None"
    )
    return family, variant


def prepare_os(os: str) -> str:
    match os:
        # Special constructors
        case "darwin":
            return "Darwin(None)"

    return os.title()


def prepare_prerelease(prerelease: str) -> str:
    if not prerelease:
        return "None"
    if not (match := PRERELEASE_PATTERN.match(prerelease)):
        raise ValueError(f"Invalid prerelease: {prerelease!r}")
    kind, number = match.groups()
    kind_mapping = {"a": "Alpha", "b": "Beta", "rc": "Rc"}
    return f"Some(Prerelease {{ kind: PrereleaseKind::{kind_mapping[kind]}, number: {number} }})"


def prepare_value(value: dict) -> dict:
    value["os"] = prepare_os(value["os"])
    value["arch_family"], value["arch_variant"] = prepare_arch(value["arch"])
    value["name"] = prepare_name(value["name"])
    value["libc"] = prepare_libc(value["libc"])
    value["prerelease"] = prepare_prerelease(value["prerelease"])
    value["variant"] = prepare_variant(value["variant"])
    return value


def main() -> None:
    debug = logging.getLogger().getEffectiveLevel() <= logging.DEBUG

    data: dict[str, Any] = {}
    data["generated_with"] = Path(__file__).relative_to(WORKSPACE_ROOT).as_posix()
    data["generated_from"] = TEMPLATE.relative_to(WORKSPACE_ROOT).as_posix()
    data["versions"] = [
        {"key": key, "value": prepare_value(value)}
        for key, value in json.loads(VERSION_METADATA.read_text()).items()
        # Exclude debug variants for now, we don't support them in the Rust side
        if value["variant"] != "debug"
    ]

    # Render the template
    logging.info(f"Rendering `{TEMPLATE.name}`...")
    output = chevron_blue.render(
        template=TEMPLATE.read_text(), data=data, no_escape=True, warn=debug
    )

    # Update the file
    logging.info(f"Updating `{TARGET}`...")
    TARGET.write_text("// DO NOT EDIT\n//\n" + output)
    subprocess.check_call(
        ["rustfmt", str(TARGET)],
        stderr=subprocess.STDOUT,
        stdout=sys.stderr if debug else subprocess.DEVNULL,
    )

    logging.info("Done!")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Generates Rust code for Python version metadata.",
    )
    parser.add_argument(
        "-v",
        "--verbose",
        action="store_true",
        help="Enable debug logging",
    )
    parser.add_argument(
        "-q",
        "--quiet",
        action="store_true",
        help="Disable logging",
    )

    args = parser.parse_args()
    if args.quiet:
        log_level = logging.CRITICAL
    elif args.verbose:
        log_level = logging.DEBUG
    else:
        log_level = logging.INFO

    logging.basicConfig(level=log_level, format="%(message)s")

    main()
