#!/usr/bin/env python3

"""Install `pylint` and `numpy` into the system Python.

To run locally, create a venv with seed packages.
"""

import argparse
import logging
import os
import shutil
import subprocess
import sys
import tempfile


def install_package(*, uv: str, package: str, version: str = None):
    """Install a package into the system Python."""

    requirement = f"{package}=={version}" if version is not None else package

    logging.info(f"Installing the package `{requirement}`.")
    subprocess.run(
        [uv, "pip", "install", requirement, "--system"] + allow_externally_managed,
        cwd=temp_dir,
        check=True,
    )

    logging.info(f"Checking that `{package}` can be imported with `{sys.executable}`.")
    code = subprocess.run(
        [sys.executable, "-c", f"import {package}"],
        cwd=temp_dir,
    )
    if code.returncode != 0:
        raise Exception(f"Could not import {package}.")

    code = subprocess.run([uv, "pip", "show", package, "--system"])
    if code.returncode != 0:
        raise Exception(f"Could not show {package}.")


if __name__ == "__main__":
    logging.basicConfig(level=logging.INFO, format="%(levelname)s: %(message)s")

    parser = argparse.ArgumentParser(description="Check a Python interpreter.")
    parser.add_argument("--uv", help="Path to a uv binary.")
    parser.add_argument(
        "--externally-managed",
        action="store_true",
        help="Set if the Python installation has an EXTERNALLY-MANAGED marker.",
    )
    parser.add_argument(
        "--python",
        required=False,
        help="Set if the system Python version must be explicitly specified, e.g., for prereleases.",
    )
    parser.add_argument(
        "--check-python-version",
        required=False,
        help="Verify that this tool has been started with the specified python version. Omitting the patch number will match any patch number.",
    )
    parser.add_argument(
        "--check-path",
        required=False,
        action="store_true",
        help="Attempt to verify that the PATH is set up so that this tool's python will match the python version that uv would automatically pick up.",
    )
    args = parser.parse_args()

    uv: str = os.path.abspath(args.uv) if args.uv else "uv"
    allow_externally_managed = (
        ["--break-system-packages"] if args.externally_managed else []
    )
    python = ["--python", args.python] if args.python else []

    # Pin packages to the last versions that support older Python interpreters.
    if sys.version_info < (3, 7):
        pylint_version = "2.12.2"
        numpy_version = "1.19.5"
        pydantic_core_version = None
    elif sys.version_info < (3, 8):
        pylint_version = "2.17.7"
        numpy_version = "1.21.6"
        pydantic_core_version = "2.14.6"
    else:
        pylint_version = None
        numpy_version = None
        pydantic_core_version = None

    pylint_requirement = (
        f"pylint=={pylint_version}" if pylint_version is not None else "pylint"
    )

    if args.check_python_version:
        version = ".".join(map(str, sys.version_info[:3]))
        if args.check_python_version != version and not version.startswith(
            args.check_python_version + "."
        ):
            raise Exception(
                f"Expected to be running {args.check_python_version} but we are on {version}."
            )

    if args.check_path:
        process = subprocess.run(
            [
                "python",
                "-c",
                "import os, sys; sys.stdout.buffer.write(os.fsencode(sys.executable))",
            ],
            check=True,
            stdout=subprocess.PIPE,
        )
        system_python_path = os.path.normcase(
            os.path.normpath(os.fsdecode(process.stdout))
        )
        our_python_path = os.path.normcase(os.path.normpath(sys.executable))

        if our_python_path != system_python_path:
            raise Exception(
                f"Script was ran with {our_python_path} but `python` resolves to {system_python_path}"
            )

    # Ensure that pip is available (e.g., the Chainguard distroless image ships
    # Python but not pip).
    try:
        import pip  # noqa: F401
    except ModuleNotFoundError:
        logging.info("pip not found, running ensurepip...")
        subprocess.run(
            [sys.executable, "-m", "ensurepip"],
            check=True,
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
            [uv, "pip", "install", pylint_requirement, "--system", "--verbose"]
            + allow_externally_managed
            + python,
            cwd=temp_dir,
            check=True,
        )

        # Ensure that the package (`pylint`) is installed.
        logging.info(
            f"Checking that `pylint` is installed with `{sys.executable} -m pip`."
        )
        code = subprocess.run(
            [sys.executable, "-m", "pip", "show", "pylint"],
            cwd=temp_dir,
        )
        if code.returncode != 0:
            raise Exception("The package `pylint` isn't installed (but should be).")

        logging.info("Checking that `pylint` is in the path.")
        if shutil.which("pylint") is None:
            raise Exception("The package `pylint` isn't in the path.")

        # Uninstall the package (`pylint`).
        logging.info("Uninstalling the package `pylint`.")
        subprocess.run(
            [uv, "pip", "uninstall", "pylint", "--system"]
            + allow_externally_managed
            + python,
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
            [uv, "pip", "install", pylint_requirement, "--verbose"],
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

        # Attempt to install NumPy.
        # This ensures that we can successfully install a package with native libraries.
        #
        # NumPy doesn't distribute wheels for Python 3.13 or GraalPy (at time of writing).
        if sys.version_info < (3, 13) and sys.implementation.name != "graalpy":
            install_package(uv=uv, package="numpy", version=numpy_version)

        # Attempt to install `pydantic_core`.
        # This ensures that we can successfully install and recognize a package that may
        # be installed into `platlib`.
        #
        # `pydantic_core` doesn't distribute wheels for non-CPython interpreters, nor
        # for Python 3.13 (at time of writing).
        if (
            sys.version_info >= (3, 7)
            and sys.version_info < (3, 13)
            and sys.implementation.name == "cpython"
        ):
            install_package(
                uv=uv, package="pydantic_core", version=pydantic_core_version
            )

        # Next, create a virtual environment with `venv`, to ensure that `uv` can
        # interoperate with `venv` virtual environments.
        shutil.rmtree(os.path.join(temp_dir, ".venv"))
        logging.info("Creating virtual environment with `venv`...")
        subprocess.run(
            [sys.executable, "-m", "venv", ".venv"],
            cwd=temp_dir,
            check=True,
        )

        # Install the package (`pylint`) into the virtual environment.
        logging.info("Installing into `venv` virtual environment...")
        subprocess.run(
            [uv, "pip", "install", pylint_requirement, "--verbose"],
            cwd=temp_dir,
            check=True,
            env=env,
        )

        # Uninstall the package (`pylint`).
        logging.info("Uninstalling the package `pylint`.")
        subprocess.run(
            [uv, "pip", "uninstall", "pylint", "--verbose"],
            cwd=temp_dir,
            check=True,
            env=env,
        )
