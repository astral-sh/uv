#!/usr/bin/env python3
"""
Test `uv add` against multiple Python package registries.

This script looks for environment variables that configure registries for testing.
To configure a registry, set the following environment variables:

    UV_TEST_<registry_name>_URL         URL for the registry
    UV_TEST_<registry_name>_TOKEN       authentication token

The package to install defaults to "astral-registries-test-pkg" but can be optionally
set with:
    UV_TEST_<registry_name>_PKG

The username defaults to "__token__" but can be optionally set with:
    UV_TEST_<registry_name>_USERNAME

Keep in mind that some registries can fall back to PyPI internally, so make sure
you choose a package that only exists in the registry you are testing.

# /// script
# requires-python = ">=3.12"
# dependencies = ["colorama>=0.4.6"]
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

cwd = Path(__file__).parent

DEFAULT_TIMEOUT = 30
DEFAULT_PKG_NAME = "astral-registries-test-pkg"

KNOWN_REGISTRIES = [
    "artifactory",
    "azure",
    "aws",
    "cloudsmith",
    "gcp",
    "gemfury",
    "gitlab",
]


def get_registries(env: Dict[str, str]) -> Dict[str, str]:
    pattern = re.compile(r"^UV_TEST_(.+)_URL$")
    registries: Dict[str, str] = {}

    for env_var, value in env.items():
        match = pattern.match(env_var)
        if match:
            registry_name = match.group(1).lower()
            registries[registry_name] = value

    return registries


def setup_test_project(registry_name: str, registry_url: str, project_dir: str):
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
    env: dict[str, str],
    uv: str,
    registry_name: str,
    registry_url: str,
    package: str,
    username: str,
    token: str,
    verbosity: int,
    timeout: int = DEFAULT_TIMEOUT,
) -> bool:
    print(uv)
    """Attempt to install a package from this registry."""
    print(
        f"{registry_name} -- Running test for {registry_url} with username {username}"
    )
    if package == DEFAULT_PKG_NAME:
        print(
            f"** Using default test package name: {package}. To choose a different package, set UV_TEST_{registry_name.upper()}_PKG"
        )
    print(f"\nAttempting to install {package}")
    env[f"UV_INDEX_{registry_name.upper()}_USERNAME"] = username
    env[f"UV_INDEX_{registry_name.upper()}_PASSWORD"] = token

    with tempfile.TemporaryDirectory() as project_dir:
        setup_test_project(registry_name, registry_url, project_dir)

        cmd = [
            uv,
            "add",
            package,
            "--index",
            registry_name,
            "--directory",
            project_dir,
        ]
        if verbosity:
            cmd.extend(["-" + "v" * verbosity])

        result = None
        try:
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=timeout,
                check=False,
                env=env,
            )

            if result.returncode != 0:
                error_msg = result.stderr.strip() if result.stderr else "Unknown error"
                print(f"{Fore.RED}{registry_name}: FAIL{Fore.RESET} \n\n{error_msg}")
                return False

            success = False
            for line in result.stderr.strip().split("\n"):
                if line.startswith(f" + {package}=="):
                    success = True
            if success:
                print(f"{Fore.GREEN}{registry_name}: PASS")
                if verbosity > 0:
                    print(f"  stdout: {result.stdout.strip()}")
                    print(f"  stderr: {result.stderr.strip()}")
                return True
            else:
                print(
                    f"{Fore.RED}{registry_name}: FAIL{Fore.RESET} - Failed to install {package}."
                )

        except subprocess.TimeoutExpired:
            print(f"{Fore.RED}{registry_name}: TIMEOUT{Fore.RESET} (>{timeout}s)")
        except FileNotFoundError:
            print(f"{Fore.RED}{registry_name}: ERROR{Fore.RESET} - uv not found")
        except Exception as e:
            print(f"{Fore.RED}{registry_name}: ERROR{Fore.RESET} - {e}")

        if result:
            if result.stdout:
                print(f"{Fore.RED} stdout:{Fore.RESET} {result.stdout.strip()}")
            if result.stderr:
                print(f"\n{Fore.RED} stderr:{Fore.RESET} {result.stderr.strip()}")
        return False


def parse_args() -> argparse.Namespace:
    """Parse command line arguments"""
    parser = argparse.ArgumentParser(
        description="Test uv add command against multiple registries",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--all",
        action="store_true",
        help="fail if any known registry was not tested",
    )
    parser.add_argument(
        "--uv",
        type=str,
        help="specify a path to the uv binary (default: uv command)",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=os.environ.get("UV_TEST_TIMEOUT", DEFAULT_TIMEOUT),
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


def main() -> None:
    args = parse_args()
    env = os.environ.copy()

    if args.uv:
        # We change the working directory for the subprocess calls, so we have to
        # absolutize the path.
        uv = Path.cwd().joinpath(args.uv)
    else:
        subprocess.run(["cargo", "build"])
        executable_suffix = ".exe" if os.name == "nt" else ""
        uv = cwd.parent.joinpath(f"target/debug/uv{executable_suffix}")

    passed = []
    failed = []
    skipped = []
    untested_registries = set(KNOWN_REGISTRIES)

    print("Running tests...")
    for registry_name, registry_url in get_registries(env).items():
        print("----------------")

        token = env.get(f"UV_TEST_{registry_name.upper()}_TOKEN")
        if not token:
            if args.all:
                print(
                    f"{Fore.RED}{registry_name}: UV_TEST_{registry_name.upper()}_TOKEN contained no token. Required by --all"
                )
                failed.append(registry_name)
            else:
                print(
                    f"{Fore.YELLOW}{registry_name}: UV_TEST_{registry_name.upper()}_TOKEN contained no token. Skipping test"
                )
                skipped.append(registry_name)
            continue

        # The private package we will test installing
        package = env.get(f"UV_TEST_{registry_name.upper()}_PKG", DEFAULT_PKG_NAME)
        username = env.get(f"UV_TEST_{registry_name.upper()}_USERNAME", "__token__")

        if run_test(
            env,
            uv,
            registry_name,
            registry_url,
            package,
            username,
            token,
            args.verbose,
            args.timeout,
        ):
            passed.append(registry_name)
        else:
            failed.append(registry_name)

        untested_registries.remove(registry_name)

    total = len(passed) + len(failed)

    print("----------------")
    if passed:
        print(f"\n{Fore.GREEN}Passed:")
        for registry_name in passed:
            print(f"     * {registry_name}")
    if failed:
        print(f"\n{Fore.RED}Failed:")
        for registry_name in failed:
            print(f"     * {registry_name}")
    if skipped:
        print(f"\n{Fore.YELLOW}Skipped:")
        for registry_name in skipped:
            print(f"     * {registry_name}")

    print(f"\nResults: {len(passed)}/{total} tests passed, {len(skipped)} skipped")

    if args.all and len(untested_registries) > 0:
        print(
            f"\n{Fore.RED}Failed to test all known registries (requested via --all).{Fore.RESET}\nMissing:"
        )
        for registry_name in untested_registries:
            print(f"     * {registry_name}")
        print("You must use the exact registry name as listed here")
        sys.exit(1)

    if total == 0:
        print("\nNo tests were run - have you defined at least one registry?")
        print("     * UV_TEST_<registry_name>_URL")
        print("     * UV_TEST_<registry_name>_TOKEN")
        print(
            "     * UV_TEST_<registry_name>_PKG (the private package to test installing)"
        )
        print('     * UV_TEST_<registry_name>_USERNAME (defaults to "__token__")')
        sys.exit(1)

    sys.exit(0 if failed == 0 else 1)


if __name__ == "__main__":
    main()
