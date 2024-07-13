#!/usr/bin/env python3

"""
Install packages on multiple versions of uv to check for cache compatibility errors.
"""

from __future__ import annotations

import argparse
import logging
import os
import subprocess
import sys
import tempfile

DEFAULT_TEST_PACKAGES = [
    # anyio is used throughout our test suite as a minimal dependency
    "anyio",
    # flask is another standard test dependency for us, but bigger than anyio
    "flask",
]

if sys.platform == "linux":
    DEFAULT_TEST_PACKAGES += [
        # homeassistant has a lot of dependencies and should be built from source
        # this requires additional dependencies on macOS so we gate it to Linux
        "homeassistant",
    ]


def install_package(*, uv: str, package: str, flags: list[str]):
    """Install a package"""

    logging.info(f"Installing the package {package!r} with {uv!r}.")
    subprocess.run(
        [uv, "pip", "install", package, "--cache-dir", os.path.join(temp_dir, "cache")]
        + flags,
        cwd=temp_dir,
        check=True,
    )

    logging.info(f"Checking that `{package}` is available.")
    code = subprocess.run([uv, "pip", "show", package], cwd=temp_dir)
    if code.returncode != 0:
        raise Exception(f"Could not show {package}.")


def clean_cache(*, uv: str):
    subprocess.run(
        [uv, "cache", "clean", "--cache-dir", os.path.join(temp_dir, "cache")],
        cwd=temp_dir,
        check=True,
    )


def check_cache_with_package(
    *,
    uv_current: str,
    uv_previous: str,
    package: str,
):
    # The coverage here is rough and not particularly targeted — we're just performing various
    # operations in the hope of catching cache load issues. As cache problems are discovered in
    # the future, we should expand coverage with targeted cases.

    # First, install with the previous uv to populate the cache
    install_package(uv=uv_previous, package=package, flags=[])

    # Audit with the current uv, this shouldn't hit the cache but is fast
    install_package(uv=uv_current, package=package, flags=[])

    # Reinstall with the current uv
    install_package(uv=uv_current, package=package, flags=["--reinstall"])

    # Reinstall with the current uv and refresh a single entry
    install_package(
        uv=uv_current,
        package=package,
        flags=["--reinstall-package", package, "--refresh-package", package],
    )

    # Reinstall with the current uv post refresh
    install_package(uv=uv_current, package=package, flags=["--reinstall"])

    # Reinstall with the current uv post refresh
    install_package(uv=uv_previous, package=package, flags=["--reinstall"])

    # Clear the cache
    clean_cache(uv=uv_previous)

    # Install with the previous uv to populate the cache
    # Use `--no-binary` to force a local build of the wheel
    install_package(uv=uv_previous, package=package, flags=["--no-binary", package])

    # Reinstall with the current uv
    install_package(uv=uv_current, package=package, flags=["--reinstall"])


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")

    parser = argparse.ArgumentParser(description="Check a Python interpreter.")
    parser.add_argument(
        "-c", "--uv-current", help="Path to a current uv binary.", required=True
    )
    parser.add_argument(
        "-p", "--uv-previous", help="Path to a previous uv binary.", required=True
    )
    parser.add_argument(
        "-t",
        "--test-package",
        action="append",
        type=str,
        help="A package to test. May be provided multiple times.",
    )
    args = parser.parse_args()

    uv_current = os.path.abspath(args.uv_current)
    uv_previous = os.path.abspath(args.uv_previous)
    test_packages = args.test_package or DEFAULT_TEST_PACKAGES

    # Create a temporary directory.
    with tempfile.TemporaryDirectory() as temp_dir:
        logging.info("Creating a virtual environment.")
        code = subprocess.run(
            [uv_current, "venv"],
            cwd=temp_dir,
        )

        for package in test_packages:
            logging.info(f"Testing with {package!r}.")
            check_cache_with_package(
                uv_current=uv_current,
                uv_previous=uv_previous,
                package=package,
            )
