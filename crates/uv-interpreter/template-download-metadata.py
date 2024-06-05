#!/usr/bin/env python3.12
"""
Generate static Rust code from Python version download metadata.

Generates the `downloads.inc` file from the `downloads.inc.mustache` template.

Usage:

    python template-download-metadata.py
"""

import sys
import logging
import argparse
import json
import subprocess
from pathlib import Path

CRATE_ROOT = Path(__file__).parent
WORKSPACE_ROOT = CRATE_ROOT.parent.parent
VERSION_METADATA = CRATE_ROOT / "download-metadata.json"
TEMPLATE = CRATE_ROOT / "src" / "downloads.inc.mustache"
TARGET = TEMPLATE.with_suffix("")


try:
    import chevron_blue
except ImportError:
    print(
        "missing requirement `chevron-blue`",
        file=sys.stderr,
    )
    exit(1)


def prepare_name(name: str) -> str:
    match name:
        case "cpython":
            return "CPython"
        case _:
            raise ValueError(f"Unknown implementation name: {name}")


def prepare_value(value: dict) -> dict:
    # Convert fields from snake case to camel case for enums
    for key in ["arch", "os", "libc"]:
        value[key] = value[key].title()
    value["name"] = prepare_name(value["name"])
    return value


def main():
    debug = logging.getLogger().getEffectiveLevel() <= logging.DEBUG

    data = {}
    data["generated_with"] = Path(__file__).relative_to(WORKSPACE_ROOT)
    data["generated_from"] = TEMPLATE.relative_to(WORKSPACE_ROOT)
    data["versions"] = [
        {"key": key, "value": prepare_value(value)}
        for key, value in json.loads(VERSION_METADATA.read_text()).items()
    ]

    # Render the template
    logging.info(f"Rendering `{TEMPLATE.name}`...")
    output = chevron_blue.render(
        template=TEMPLATE.read_text(), data=data, no_escape=True, warn=debug
    )

    # Update the file
    logging.info(f"Updating `{TARGET}`...")
    TARGET.write_text(output)
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
