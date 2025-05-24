#!/usr/bin/env python3
"""
Test `uv add` against multiple Python package registries.

This script looks for environment variables that configure registries for testing.
To configure a registry, set the following environment variables:

    UV_TEST_<registry_name>_URL         URL for the registry
    UV_TEST_<registry_name>_TOKEN       authentication token
    UV_TEST_<package_name>_PKG          private package to install

The username defaults to "__token__" but can be optionally set with:
    UV_TEST_<registry_name>_USERNAME

Keep in mind that some registries can fall back to PyPI internally, so make sure
you choose a package that only exists in the registry you are testing.

# /// script
# dependencies = ["colorama"]
# ///
"""

import argparse
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Dict

from colorama import Fore
from colorama import init as colorama_init

colorama_init(autoreset=True)

DEFAULT_TIMEOUT = 30


def get_registries() -> Dict[str, str]:
    pattern = re.compile(r"^UV_TEST_(.+)_URL$")
    registries: Dict[str, str] = {}

    for env_var, value in os.environ.items():
        match = pattern.match(env_var)
        if match:
            registry_name = match.group(1).lower()
            registries[registry_name] = value

    return registries


def setup_test_project(registry_name: str, registry_url: str, project_dir: str) -> Path:
    """Create a temporary project directory with a pyproject.toml"""
    pyproject_content = f"""[project]
name = "{registry_name}-test"
version = "0.1.0"
description = "Test registry"

[[tool.uv.index]]
name = "{registry_name}"
url = "{registry_url}"
default = true
"""

    pyproject_file = Path(project_dir) / "pyproject.toml"
    pyproject_file.write_text(pyproject_content, encoding="utf-8")


def run_test(
    registry_name: str,
    registry_url: str,
    package: str,
    username: str,
    token: str,
    verbosity: int,
    timeout: int = DEFAULT_TIMEOUT,
) -> bool:
    """Attempt to install package from this registry."""
    print(
        f"{registry_name} -- Running test for {registry_url} with username {username}"
    )
    print(f"\nAttempting to install {package}")
    os.environ[f"UV_INDEX_{registry_name.upper()}_USERNAME"] = username
    os.environ[f"UV_INDEX_{registry_name.upper()}_PASSWORD"] = token

    with tempfile.TemporaryDirectory() as project_dir:
        cmd = [
            "cargo",
            "run",
            "--",
            "add",
            package,
            "--index",
            registry_name,
            "--directory",
            project_dir,
        ]
        if verbosity >= 2:
            cmd.extend(["-vv"])
        elif verbosity == 1:
            cmd.extend(["-v"])

        setup_test_project(registry_name, registry_url, project_dir)

        try:
            result = subprocess.run(
                cmd, capture_output=True, text=True, timeout=timeout, check=False
            )

            if result.returncode != 0:
                error_msg = result.stderr.strip() if result.stderr else "Unknown error"
                print(f"{Fore.RED}{registry_name}: FAIL - {Fore.RESET} {error_msg}")
                return False

            success = False
            for line in result.stderr.strip().split("\n"):
                if line.startswith(f" + {package}=="):
                    success = True
            if success:
                print(f"{Fore.GREEN}{registry_name}: PASS")
                if verbosity > 0:
                    print(f"  stderr: {result.stderr.strip()}")
                return True
            else:
                print(
                    f"{Fore.RED}{registry_name}: FAIL{Fore.RESET} - Failed to install {package}."
                )
                if result.stderr:
                    print(f"{Fore.RED}  stderr:{Fore.RESET} {result.stderr.strip()}")
                return False

        except subprocess.TimeoutExpired:
            print(f"{Fore.RED}{registry_name}: TIMEOUT{Fore.RESET} (>{timeout}s)")
            return False
        except FileNotFoundError:
            print(
                f"{Fore.RED}{registry_name}: ERROR{Fore.RESET} - 'cargo' command not found"
            )
            return False
        except Exception as e:
            print(f"{Fore.RED}{registry_name}: ERROR{Fore.RESET} - {e}")
            return False


def parse_args() -> argparse.Namespace:
    """Parse command line arguments"""
    parser = argparse.ArgumentParser(
        description="Test uv add command against multiple registries",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=None,
        help=f"timeout in seconds for each test (default: {DEFAULT_TIMEOUT} or UV_TEST_TIMEOUT)",
    )
    parser.add_argument(
        "-v",
        "--verbose",
        action="count",
        default=0,
        help="increase verbosity (-v for debug, -vv for trace)",
    )
    return parser.parse_args()


def build_uv():
    cmd = ["cargo", "build"]
    result = subprocess.run(cmd, capture_output=True, text=True, check=False)

    if result.returncode != 0:
        error_msg = result.stderr.strip() if result.stderr else "Unknown error"
        print(f"{Fore.RED}Cargo failed to build{Fore.RESET}: {error_msg}")
        sys.exit(1)


def main() -> None:
    args = parse_args()

    # Determine timeout. Precedence: Command line arg > env var > default
    if args.timeout is not None:
        timeout = args.timeout
    else:
        timeout = int(os.getenv("UV_TEST_TIMEOUT", str(DEFAULT_TIMEOUT)))

    passed = 0
    failed = 0
    skipped = 0

    print("Building...")
    build_uv()

    print("Running tests...")
    for registry_name, registry_url in get_registries().items():
        print("----------------")

        token = os.getenv(f"UV_TEST_{registry_name.upper()}_TOKEN")
        if not token:
            print(
                f"{Fore.RED}{registry_name}: UV_TEST_{registry_name.upper()}_TOKEN contained no token. Skipping test"
            )
            skipped += 1
            continue

        # The private package we will test installing
        package = os.getenv(f"UV_TEST_{registry_name.upper()}_PKG")
        if not package:
            print(
                f"{Fore.RED}{registry_name}: UV_TEST_{registry_name.upper()}_PKG contained no private package name to install. Skipping test"
            )
            skipped += 1
            continue

        username = os.getenv(f"UV_TEST_{registry_name.upper()}_USERNAME") or "__token__"

        if run_test(
            registry_name, registry_url, package, username, token, args.verbose, timeout
        ):
            passed += 1
        else:
            failed += 1

    total = passed + failed

    print("----------------")
    print(f"\nResults: {passed}/{total} tests passed, {skipped} skipped")
    if total == 0:
        print("\nNo tests were run - have you defined at least one registry?")
        print("     * UV_TEST_<registry_name>_URL")
        print("     * UV_TEST_<registry_name>_TOKEN")
        print(
            "     * UV_TEST_<package_name>_PKG (the private package to test installing)"
        )
        print('     * UV_TEST_<registry_name>_USERNAME (defaults to "__token__")')
        sys.exit(1)

    sys.exit(0 if failed == 0 else 1)


if __name__ == "__main__":
    main()
