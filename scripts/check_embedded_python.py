#!/usr/bin/env python3

"""Install `pylint` and `numpy` into an embedded Python."""

import argparse
import logging
import os
import subprocess
import sys
import tempfile


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")

    parser = argparse.ArgumentParser(description="Check a Python interpreter.")
    parser.add_argument("--uv", help="Path to a uv binary.")
    args = parser.parse_args()

    uv: str = os.path.abspath(args.uv) if args.uv else "uv"

    # Create a temporary directory.
    with tempfile.TemporaryDirectory() as temp_dir:
        # Create a virtual environment with `uv`.
        logging.info("Creating virtual environment with `uv`...")
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

        logging.info("Installing into `uv` virtual environment...")

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

        # Ensure that the package (`pylint`) is installed in the virtual environment.
        logging.info("Checking that `pylint` is installed.")
        code = subprocess.run(
            [executable, "-c", "import pylint"],
            cwd=temp_dir,
        )
        if code.returncode != 0:
            raise Exception(
                "The package `pylint` isn't installed in the virtual environment."
            )

        # Uninstall the package (`pylint`).
        logging.info("Uninstalling the package `pylint`.")
        subprocess.run(
            [uv, "pip", "uninstall", "pylint", "--verbose"],
            cwd=temp_dir,
            check=True,
            env=env,
        )

        # Ensure that the package (`pylint`) isn't installed in the virtual environment.
        logging.info("Checking that `pylint` isn't installed.")
        code = subprocess.run(
            [executable, "-m", "pip", "show", "pylint"],
            cwd=temp_dir,
        )
        if code.returncode == 0:
            raise Exception(
                "The package `pylint` is installed in the virtual environment (but shouldn't be)."
            )
