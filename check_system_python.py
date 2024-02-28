import argparse
import logging
import os
import subprocess
import tempfile

if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")

    parser = argparse.ArgumentParser(description="Check a Python interpreter.")
    parser.add_argument("python", help="Path to a Python interpreter.")
    parser.add_argument("--uv", help="Path to a uv binary.")
    args = parser.parse_args()

    python: str = args.python
    uv: str = os.path.abspath(args.uv) or "uv"

    # Create a temporary directory.
    temp_dir = tempfile.mkdtemp()

    # Ensure that the package (`black`) isn't installed.
    logging.info("Checking that `black` isn't installed.")
    code = subprocess.run(
        [python, "-m", "pip", "show", "black"],
        cwd=temp_dir,
        # stdout=subprocess.DEVNULL,
        # stderr=subprocess.DEVNULL,
    )
    if code.returncode == 0:
        raise Exception("The package `black` is installed.")

    # Install the package (`black`).
    logging.info("Installing the package `black`.")
    subprocess.run(
        [uv, "pip", "install", "black", "--python", python],
        cwd=temp_dir,
#         stdout=subprocess.DEVNULL,
#         stderr=subprocess.DEVNULL,
        check=True,
    )

    # Ensure that the package (`black`) isn't installed.
    logging.info("Checking that `black` is installed.")
    code = subprocess.run(
        [python, "-m", "pip", "show", "black"],
        cwd=temp_dir,
#         stdout=subprocess.DEVNULL,
#         stderr=subprocess.DEVNULL,
    )
    if code.returncode != 0:
        raise Exception("The package `black` isn't installed.")

    # Ensure that the package is in the path.
    logging.info("Checking that `black` is in the path.")
    code = subprocess.run(
        ["black", "--version"],
        cwd=temp_dir,
#         stdout=subprocess.DEVNULL,
#         stderr=subprocess.DEVNULL,
    )
    if code.returncode != 0:
        raise Exception("The package `black` isn't in the path.")

    # Uninstall the package (`black`).
    logging.info("Uninstalling the package `black`.")
    subprocess.run(
        [uv, "pip", "uninstall", "black", "--python", python],
        cwd=temp_dir,
#         stdout=subprocess.DEVNULL,
#         stderr=subprocess.DEVNULL,
        check=True,
    )

    # Ensure that the package (`black`) isn't installed.
    logging.info("Checking that `black` isn't installed.")
    code = subprocess.run(
        [python, "-m", "pip", "show", "black"],
        cwd=temp_dir,
#         stdout=subprocess.DEVNULL,
#         stderr=subprocess.DEVNULL,
    )
    if code.returncode == 0:
        raise Exception("The package `black` is installed.")
