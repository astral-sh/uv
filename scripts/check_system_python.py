#!/usr/bin/env python3

"""Install `pylint` and `numpy` into the system Python.

To run locally, create a venv with seed packages.
"""

import argparse
import logging
import os
import subprocess
import sys
import tempfile


def numpy():
    """sys.version_info"""
    logging.info("Installing the package `numpy`.")
    subprocess.run(
        [uv, "pip", "install", "numpy", "--system"],
        cwd=temp_dir,
        check=True,
    )
    # Check that the native libraries of numpy work.
    logging.info("Checking that `numpy` can be imported.")
    code = subprocess.run(
        [sys.executable, "-c", "import numpy"],
        cwd=temp_dir,
    )
    if code.returncode != 0:
        raise Exception("Could not import numpy.")


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")

    parser = argparse.ArgumentParser(description="Check a Python interpreter.")
    parser.add_argument("--uv", help="Path to a uv binary.")
    parser.add_argument(
        "--externally-managed",
        action="store_true",
        help="Set if the Python installation has an EXTERNALLY-MANAGED marker.",
    )
    args = parser.parse_args()

    uv: str = os.path.abspath(args.uv) if args.uv else "uv"
    allow_externally_managed = (
        ["--break-system-packages"] if args.externally_managed else []
    )

    # Create a temporary directory.
    with tempfile.TemporaryDirectory() as temp_dir:
        # Ensure that the package (`pylint`) isn't installed.
        logging.info("Checking that `pylint` isn't installed.")
        code = subprocess.run(
            [sys.executable, "-m", "pip", "show", "pylint"],
            cwd=temp_dir,
        )
        if code.returncode == 0:
            raise Exception("The package `pylint` is installed (but shouldn't be).")

        # Install the package (`pylint`).
        logging.info("Installing the package `pylint`.")
        subprocess.run(
            [uv, "pip", "install", "pylint", "--system"] + allow_externally_managed,
            cwd=temp_dir,
            check=True,
        )

        # Ensure that the package (`pylint`) is installed.
        logging.info("Checking that `pylint` is installed.")
        code = subprocess.run(
            [sys.executable, "-m", "pip", "show", "pylint"],
            cwd=temp_dir,
        )
        if code.returncode != 0:
            raise Exception("The package `pylint` isn't installed (but should be).")

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
            [uv, "pip", "uninstall", "pylint", "--system"] + allow_externally_managed,
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
            raise Exception("The package `pylint` is installed (but shouldn't be).")

        # Create a virtual environment with `uv`.
        logging.info("Creating virtual environment...")
        subprocess.run(
            [uv, "venv", ".venv", "--seed", "--python", sys.executable],
            cwd=temp_dir,
            check=True,
        )

        if os.name == "nt":
            executable = os.path.join(temp_dir, ".venv", "Scripts", "python.exe")
        else:
            executable = os.path.join(temp_dir, ".venv", "bin", "python")

        logging.info("Querying virtual environment...")
        subprocess.run(
            [executable, "--version"],
            cwd=temp_dir,
            check=True,
        )

        logging.info("Installing into virtual environment...")

        # Disable the `CONDA_PREFIX` and `VIRTUAL_ENV` environment variables, so that
        # we only rely on virtual environment discovery via the `.venv` directory.
        # Our "system Python" here might itself be a Conda environment!
        env = os.environ.copy()
        env["CONDA_PREFIX"] = ""
        env["VIRTUAL_ENV"] = ""
        subprocess.run(
            [uv, "pip", "install", "pylint", "--verbose"],
            cwd=temp_dir,
            check=True,
            env=env,
        )

        # Ensure that the package (`pylint`) isn't installed globally.
        logging.info("Checking that `pylint` isn't installed.")
        code = subprocess.run(
            [sys.executable, "-m", "pip", "show", "pylint"],
            cwd=temp_dir,
        )
        if code.returncode == 0:
            raise Exception(
                "The package `pylint` is installed globally (but shouldn't be)."
            )

        # Ensure that the package (`pylint`) is installed in the virtual environment.
        logging.info("Checking that `pylint` is installed.")
        code = subprocess.run(
            [executable, "-m", "pip", "show", "pylint"],
            cwd=temp_dir,
        )
        if code.returncode != 0:
            raise Exception(
                "The package `pylint` isn't installed in the virtual environment."
            )

        # Numpy doesn't have wheels for python 3.13 (at the time of writing)
        if sys.version_info < (3, 13):
            numpy()
