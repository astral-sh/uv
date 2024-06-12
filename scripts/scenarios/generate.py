#!/usr/bin/env python3
"""
Generates and updates snapshot test cases from packse scenarios.

Important:

    This script is the backend called by `./scripts/sync_scenarios.sh`, consider using that
    if not developing scenarios.

Requirements:

    $ uv pip install -r scripts/scenarios/requirements.txt

    Uses `git`, `rustfmt`, and `cargo insta test` requirements from the project.

Usage:

    Regenerate the scenario test files using the given scenarios:

        $ ./scripts/scenarios/generate.py <path>

    Scenarios can be developed locally with the following workflow:

        Serve scenarios on a local index using packse

        $ packse serve --no-hash <path to scenarios>

        Override the uv package index and update the tests

        $ UV_TEST_INDEX_URL="http://localhost:3141/simple/" ./scripts/scenarios/generate.py <path to scenarios>

        If an editable version of packse is installed, this script will use its bundled scenarios by default.

"""

import argparse
import importlib.metadata
import logging
import os
import re
import subprocess
import sys
import textwrap
from pathlib import Path

TOOL_ROOT = Path(__file__).parent
TEMPLATES = TOOL_ROOT / "templates"
INSTALL_TEMPLATE = TEMPLATES / "install.mustache"
COMPILE_TEMPLATE = TEMPLATES / "compile.mustache"
LOCK_TEMPLATE = TEMPLATES / "lock.mustache"
PACKSE = TOOL_ROOT / "packse-scenarios"
REQUIREMENTS = TOOL_ROOT / "requirements.txt"
PROJECT_ROOT = TOOL_ROOT.parent.parent
TESTS = PROJECT_ROOT / "crates" / "uv" / "tests"
INSTALL_TESTS = TESTS / "pip_install_scenarios.rs"
COMPILE_TESTS = TESTS / "pip_compile_scenarios.rs"
LOCK_TESTS = TESTS / "lock_scenarios.rs"
TESTS_COMMON_MOD_RS = TESTS / "common/mod.rs"

try:
    import packse
    import packse.inspect
except ImportError:
    print(
        f"missing requirement `packse`: install the requirements at {REQUIREMENTS.relative_to(PROJECT_ROOT)}",
        file=sys.stderr,
    )
    exit(1)

try:
    import chevron_blue
except ImportError:
    print(
        f"missing requirement `chevron-blue`: install the requirements at {REQUIREMENTS.relative_to(PROJECT_ROOT)}",
        file=sys.stderr,
    )
    exit(1)


def main(scenarios: list[Path], snapshot_update: bool = True):
    # Fetch packse version
    packse_version = importlib.metadata.version("packse")

    debug = logging.getLogger().getEffectiveLevel() <= logging.DEBUG

    update_common_mod_rs(packse_version)

    if not scenarios:
        if packse_version == "0.0.0":
            path = packse.__development_base_path__ / "scenarios"
            if path.exists():
                logging.info(
                    "Detected development version of packse, using scenarios from %s",
                    path,
                )
                scenarios = path.glob("*.json")
            else:
                logging.error(
                    "No scenarios provided. Found development version of packse but is missing scenarios. Is it installed as an editable?"
                )
                sys.exit(1)
        else:
            logging.error("No scenarios provided, nothing to do.")
            return

    targets = []
    for target in scenarios:
        if target.is_dir():
            targets.extend(target.glob("**/*.json"))
            targets.extend(target.glob("**/*.toml"))
        else:
            targets.append(target)

    logging.info("Loading scenario metadata...")
    data = packse.inspect.inspect(
        targets=targets,
        no_hash=True,
    )

    data["scenarios"] = [
        scenario
        for scenario in data["scenarios"]
        # Drop example scenarios
        if not scenario["name"].startswith("example")
    ]

    # Wrap the description onto multiple lines
    for scenario in data["scenarios"]:
        scenario["description_lines"] = textwrap.wrap(scenario["description"], width=80)

    # Wrap the expected explanation onto multiple lines
    for scenario in data["scenarios"]:
        expected = scenario["expected"]
        expected["explanation_lines"] = (
            textwrap.wrap(expected["explanation"], width=80)
            if expected["explanation"]
            else []
        )

    # Hack to track which scenarios require a specific Python patch version
    for scenario in data["scenarios"]:
        if "patch" in scenario["name"]:
            scenario["python_patch"] = True
        else:
            scenario["python_patch"] = False

    # We don't yet support local versions that aren't expressed as direct dependencies.
    for scenario in data["scenarios"]:
        expected = scenario["expected"]

        if scenario["name"] in (
            "local-less-than-or-equal",
            "local-simple",
            "local-transitive-confounding",
            "local-used-without-sdist",
        ):
            expected["satisfiable"] = False
            expected["explanation"] = (
                "We do not have correct behavior for local version identifiers yet"
            )

    # Split scenarios into `install`, `compile` and `lock` cases
    install_scenarios = []
    compile_scenarios = []
    lock_scenarios = []

    for scenario in data["scenarios"]:
        resolver_options = scenario["resolver_options"] or {}
        if resolver_options.get("universal"):
            print(scenario["name"])
            lock_scenarios.append(scenario)
        elif resolver_options.get("python") is not None:
            compile_scenarios.append(scenario)
        else:
            install_scenarios.append(scenario)

    for template, tests, scenarios in [
        (INSTALL_TEMPLATE, INSTALL_TESTS, install_scenarios),
        (COMPILE_TEMPLATE, COMPILE_TESTS, compile_scenarios),
        (LOCK_TEMPLATE, LOCK_TESTS, lock_scenarios),
    ]:
        data = {"scenarios": scenarios}

        ref = "HEAD" if packse_version == "0.0.0" else packse_version

        # Add generated metadata
        data["generated_from"] = (
            f"https://github.com/astral-sh/packse/tree/{ref}/scenarios"
        )
        data["generated_with"] = "./scripts/sync_scenarios.sh"
        data["vendor_links"] = (
            f"https://raw.githubusercontent.com/astral-sh/packse/{ref}/vendor/links.html"
        )

        data["index_url"] = os.environ.get(
            "UV_TEST_INDEX_URL",
            f"https://astral-sh.github.io/packse/{ref}/simple-html/",
        )

        # Render the template
        logging.info(f"Rendering template {template.name}")
        output = chevron_blue.render(
            template=template.read_text(), data=data, no_escape=True, warn=True
        )

        # Update the test files
        logging.info(
            f"Updating test file at `{tests.relative_to(PROJECT_ROOT)}`...",
        )
        with open(tests, "wt") as test_file:
            test_file.write(output)

        # Format
        logging.info(
            "Formatting test file...",
        )
        subprocess.check_call(
            ["rustfmt", str(tests)],
            stderr=subprocess.STDOUT,
            stdout=sys.stderr if debug else subprocess.DEVNULL,
        )

        # Update snapshots
        if snapshot_update:
            logging.info("Updating snapshots...")
            env = os.environ.copy()
            env["UV_TEST_PYTHON_PATH"] = str(PROJECT_ROOT / "bin")
            subprocess.call(
                [
                    "cargo",
                    "insta",
                    "test",
                    "--features",
                    "pypi,python,python-patch",
                    "--accept",
                    "--test-runner",
                    "nextest",
                    "--test",
                    tests.with_suffix("").name,
                ],
                cwd=PROJECT_ROOT,
                stderr=subprocess.STDOUT,
                stdout=sys.stderr if debug else subprocess.DEVNULL,
                env=env,
            )
        else:
            logging.info("Skipping snapshot update")

    logging.info("Done!")


def update_common_mod_rs(packse_version: str):
    """Update the value of `BUILD_VENDOR_LINKS_URL` used in non-scenario tests."""
    test_common = TESTS_COMMON_MOD_RS.read_text()
    url_before_version = "https://raw.githubusercontent.com/astral-sh/packse/"
    url_after_version = "/vendor/links.html"
    build_vendor_links_url = f"{url_before_version}{packse_version}{url_after_version}"
    if build_vendor_links_url in test_common:
        logging.info(f"Up-to-date: {TESTS_COMMON_MOD_RS}")
    else:
        logging.info(f"Updating: {TESTS_COMMON_MOD_RS}")
        url_matcher = re.compile(
            re.escape(url_before_version) + "[^/]+" + re.escape(url_after_version)
        )
        assert (
            len(url_matcher.findall(test_common)) == 1
        ), f"BUILD_VENDOR_LINKS_URL not found in {TESTS_COMMON_MOD_RS}"
        test_common = url_matcher.sub(build_vendor_links_url, test_common)
        TESTS_COMMON_MOD_RS.write_text(test_common)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Generates and updates snapshot test cases from packse scenarios.",
    )
    parser.add_argument(
        "scenarios",
        type=Path,
        nargs="*",
        help="The scenario files to use",
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

    parser.add_argument(
        "--no-snapshot-update",
        action="store_true",
        help="Disable automatic snapshot updates",
    )

    args = parser.parse_args()
    if args.quiet:
        log_level = logging.CRITICAL
    elif args.verbose:
        log_level = logging.DEBUG
    else:
        log_level = logging.INFO

    logging.basicConfig(level=log_level, format="%(message)s")

    main(args.scenarios, snapshot_update=not args.no_snapshot_update)
