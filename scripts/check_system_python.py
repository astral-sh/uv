#!/usr/bin/env python3

"""Install `pylint` into the system Python."""

import argparse
import logging
import os
import subprocess
import sys
import tempfile

if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")

    print("sys.executable:: %s" % sys.executable)

    parser = argparse.ArgumentParser(description="Check a Python interpreter.")
    parser.add_argument("--uv", help="Path to a uv binary.")
    args = parser.parse_args()

    uv: str = os.path.abspath(args.uv) if args.uv else "uv"

    # Create a temporary directory.
    with tempfile.TemporaryDirectory() as temp_dir:
        # Ensure that the package (`pylint`) isn't installed.
        logging.info("Checking that `pylint` isn't installed.")
        code = subprocess.run(
            [sys.executable, "-m", "pip", "show", "pylint"],
            cwd=temp_dir,
        )
        if code.returncode == 0:
            raise Exception("The package `pylint` is installed.")

        # Install the package (`pylint`).
        logging.info("Installing the package `pylint`.")
        subprocess.run(
            [uv, "pip", "install", "pylint", "--system", "--verbose"],
            cwd=temp_dir,
            check=True,
        )

        # Ensure that the package (`pylint`) isn't installed.
        logging.info("Checking that `pylint` is installed.")
        code = subprocess.run(
            [sys.executable, "-m", "pip", "show", "pylint"],
            cwd=temp_dir,
        )
        if code.returncode != 0:
            raise Exception("The package `pylint` isn't installed.")

        # TODO(charlie): Windows is failing to find the `pylint` binary, despite
        # confirmation that it's being written to the intended location.
        if os.name != "nt":
            logging.info("Checking that `pylint` is in the path.")
            code = subprocess.run(["which", "pylint"], cwd=temp_dir)
            if code.returncode != 0:
                raise Exception("The package `pylint` isn't in the path.")

        # Uninstall the package (`pylint`).
        logging.info("Uninstalling the package `pylint`.")
        subprocess.run(
            [uv, "pip", "uninstall", "pylint", "--system", "--verbose"],
            cwd=temp_dir,
            check=True,
        )

        # Ensure that the package (`pylint`) isn't installed.
        logging.info("Checking that `pylint` isn't installed.")
        code = subprocess.run(
            [sys.executable, "-m", "pip", "show", "pylint"],
            cwd=temp_dir,
        )
        if code.returncode == 0:
            raise Exception("The package `pylint` is installed.")
