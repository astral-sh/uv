#!/usr/bin/env python3
"""
Test `uv add` against multiple Python package registries.

This script looks for environment variables that configure registries for testing.
To configure a registry, set the following environment variables:

    `UV_TEST_<registry_name>_URL`         URL for the registry
    `UV_TEST_<registry_name>_TOKEN`       authentication token

The username defaults to "__token__" but can be optionally set with:
    `UV_TEST_<registry_name>_USERNAME`

The package to install defaults to "astral-registries-test-pkg" but can be optionally
set with:
    `UV_TEST_<registry_name>_PKG`

Keep in mind that some registries can fall back to PyPI internally, so make sure
you choose a package that only exists in the registry you are testing.

You can also use the 1Password CLI to fetch registry credentials from a vault by passing
the `--use-op` flag. For each item in the vault named `UV_TEST_XXX`, the script will set
env vars for any of the following fields, if present:
    `UV_TEST_<registry_name>_USERNAME` from the `username` field
    `UV_TEST_<registry_name>_TOKEN` from the `password` field
    `UV_TEST_<registry_name>_URL` from a field with the label `url`
    `UV_TEST_<registry_name>_PKG` from a field with the label `pkg`

# /// script
# requires-python = ">=3.12"
# dependencies = ["colorama>=0.4.6"]
# ///
"""

import argparse
import json
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


def fetch_op_items(vault_name: str, env: Dict[str, str]) -> Dict[str, str]:
    """Fetch items from the specified 1Password vault and add them to the environment.

    For each item named UV_TEST_XXX in the vault:
    - Set `UV_TEST_XXX_USERNAME` to the `username` field
    - Set `UV_TEST_XXX_TOKEN` to the `password` field
    - Set `UV_TEST_XXX_URL` to the `url` field

    Raises exceptions for any 1Password CLI errors so they can be handled by the caller.
    """
    # Run 'op item list' to get all items in the vault
    result = subprocess.run(
        ["op", "item", "list", "--vault", vault_name, "--format", "json"],
        capture_output=True,
        text=True,
        check=True,
    )

    items = json.loads(result.stdout)
    updated_env = env.copy()

    for item in items:
        item_id = item["id"]
        item_title = item["title"]

        # Only process items that match the registry naming pattern
        if item_title.startswith("UV_TEST_"):
            # Extract the registry name (e.g., "AWS" from "UV_TEST_AWS")
            registry_name = item_title[8:]  # Remove "UV_TEST_" prefix

            # Get the item details
            item_details = subprocess.run(
                ["op", "item", "get", item_id, "--format", "json"],
                capture_output=True,
                text=True,
                check=True,
            )

            item_data = json.loads(item_details.stdout)

            username = None
            password = None
            url = None
            pkg = None

            if "fields" in item_data:
                for field in item_data["fields"]:
                    if field.get("id") == "username":
                        username = field.get("value")
                    elif field.get("id") == "password":
                        password = field.get("value")
                    elif field.get("label") == "url":
                        url = field.get("value")
                    elif field.get("label") == "pkg":
                        pkg = field.get("value")
            if username:
                updated_env[f"UV_TEST_{registry_name}_USERNAME"] = username
            if password:
                updated_env[f"UV_TEST_{registry_name}_TOKEN"] = password
            if url:
                updated_env[f"UV_TEST_{registry_name}_URL"] = url
            if pkg:
                updated_env[f"UV_TEST_{registry_name}_PKG"] = pkg

            print(f"Added 1Password credentials for {registry_name}")

    return updated_env


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
    parser.add_argument(
        "--use-op",
        action="store_true",
        help="use 1Password CLI to fetch registry credentials from the specified vault",
    )
    parser.add_argument(
        "--op-vault",
        type=str,
        default="RegistryTests",
        help="name of the 1Password vault to use (default: RegistryTests)",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    env = os.environ.copy()

    # If using 1Password, fetch credentials from the vault
    if args.use_op:
        print(f"Fetching credentials from 1Password vault '{args.op_vault}'...")
        try:
            env = fetch_op_items(args.op_vault, env)
        except Exception as e:
            print(f"{Fore.RED}Error accessing 1Password: {e}{Fore.RESET}")
            print(
                f"{Fore.YELLOW}Hint: If you're not authenticated, run 'op signin' first.{Fore.RESET}"
            )
            sys.exit(1)

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
