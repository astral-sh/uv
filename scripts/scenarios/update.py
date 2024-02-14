#!/usr/bin/env python3
"""
Generates and updates snapshot test cases from packse scenarios.

Usage:

    Regenerate the scenario test file:
    
        $ ./scripts/scenarios/update.py

    Scenarios are pinned to a specific commit. Change the `PACKSE_COMMIT` constant to update them.

    Scenarios can be developed locally with the following workflow:

        Install the local version of packse

        $ pip install -e <path to packse>

        From the packse repository, build and publish the scenarios to a local index

        $ packse index up --bg
        $ packse build scenarios/*
        $ packse publish dist/* --index-url http://localhost:3141/packages/local --anonymous

        Override the default PyPI index for uv and update the scenarios

        $ PUFFIN_INDEX_URL="http://localhost:3141/packages/all/+simple" ./scripts/scenarios/update.py

Requirements:

    Requires `packse` and `chevron-blue`.

        $ pip install -r scripts/scenarios/requirements.txt

    Also supports a local, editable requirement on `packse`.

    Uses `git`, `rustfmt`, and `cargo insta test` requirements from the project.
"""

import json
import shutil
import subprocess
import sys
import textwrap
from pathlib import Path


PACKSE_COMMIT = "c35c57f5b4ab3381658661edbd0cd955680f9cda"
TOOL_ROOT = Path(__file__).parent
TEMPLATES = TOOL_ROOT / "templates"
INSTALL_TEMPLATE = TEMPLATES / "install.mustache"
COMPILE_TEMPLATE = TEMPLATES / "compile.mustache"
PACKSE = TOOL_ROOT / "packse-scenarios"
REQUIREMENTS = TOOL_ROOT / "requirements.txt"
PROJECT_ROOT = TOOL_ROOT.parent.parent
TESTS = PROJECT_ROOT / "crates" / "puffin" / "tests"
INSTALL_TESTS = TESTS / "pip_install_scenarios.rs"
COMPILE_TESTS = TESTS / "pip_compile_scenarios.rs"

CUTE_NAMES = {
    "a": "albatross",
    "b": "bluebird",
    "c": "crow",
    "d": "duck",
    "e": "eagle",
    "f": "flamingo",
    "g": "goose",
    "h": "heron",
}

try:
    import packse
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


if packse.__development_base_path__.name != "packse":
    # Not a local editable installation, download latest scenarios
    if PACKSE.exists():
        shutil.rmtree(PACKSE)

    print("Downloading scenarios from packse repository...", file=sys.stderr)
    # Perform a sparse checkout where we only grab the `scenarios` folder
    subprocess.check_call(
        [
            "git",
            "clone",
            "-n",
            "--depth=1",
            "--filter=tree:0",
            "https://github.com/zanieb/packse",
            str(PACKSE),
        ],
        cwd=TOOL_ROOT,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.STDOUT,
    )
    subprocess.check_call(
        ["git", "sparse-checkout", "set", "--no-cone", "scenarios"],
        cwd=PACKSE,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.STDOUT,
    )
    subprocess.check_call(
        ["git", "checkout", PACKSE_COMMIT],
        cwd=PACKSE,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.STDOUT,
    )
    scenarios_path = str(PACKSE / "scenarios")
    commit = PACKSE_COMMIT

else:
    print(
        f"Using scenarios in packse repository at {packse.__development_base_path__}",
        file=sys.stderr,
    )
    scenarios_path = str(packse.__development_base_path__ / "scenarios")

    # Get the commit from the repository
    commit = (
        subprocess.check_output(
            ["git", "show", "-s", "--format=%H", "HEAD"],
            cwd=packse.__development_base_path__,
        )
        .decode()
        .strip()
    )

    if commit != PACKSE_COMMIT:
        print(f"WARNING: Expected commit {PACKSE_COMMIT!r} but found {commit!r}.")

print("Loading scenario metadata...", file=sys.stderr)
data = json.loads(
    subprocess.check_output(
        [
            sys.executable,
            "-m",
            "packse",
            "inspect",
            "--short-names",
            scenarios_path,
        ],
    )
)


data["scenarios"] = [
    scenario
    for scenario in data["scenarios"]
    # Drop the example scenario
    if scenario["name"] != "example"
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

# Generate cute names for each scenario
for scenario in data["scenarios"]:
    for package in scenario["packages"]:
        package["cute_name"] = CUTE_NAMES[package["name"][0]]


# Split scenarios into `install` and `compile` cases
install_scenarios = []
compile_scenarios = []

for scenario in data["scenarios"]:
    if (scenario["resolver_options"] or {}).get("python") is not None:
        compile_scenarios.append(scenario)
    else:
        install_scenarios.append(scenario)

for template, tests, scenarios in [
    (INSTALL_TEMPLATE, INSTALL_TESTS, install_scenarios),
    (COMPILE_TEMPLATE, COMPILE_TESTS, compile_scenarios),
]:
    data = {"scenarios": scenarios}

    # Add generated metadata
    data["generated_from"] = f"https://github.com/zanieb/packse/tree/{commit}/scenarios"
    data["generated_with"] = " ".join(sys.argv)

    # Render the template
    print(f"Rendering template {template.name}", file=sys.stderr)
    output = chevron_blue.render(
        template=template.read_text(), data=data, no_escape=True, warn=True
    )

    # Update the test files
    print(
        f"Updating test file at `{tests.relative_to(PROJECT_ROOT)}`...",
        file=sys.stderr,
    )
    with open(tests, "wt") as test_file:
        test_file.write(output)

    # Format
    print(
        "Formatting test file...",
        file=sys.stderr,
    )
    subprocess.check_call(["rustfmt", str(tests)])

    # Update snapshots
    print("Updating snapshots...\n", file=sys.stderr)
    subprocess.call(
        [
            "cargo",
            "insta",
            "test",
            "--features",
            "pypi,python",
            "--accept",
            "--test-runner",
            "nextest",
            "--test",
            tests.with_suffix("").name,
        ],
        cwd=PROJECT_ROOT,
    )

print("\nDone!", file=sys.stderr)
