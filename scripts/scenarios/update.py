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

        Override the default PyPI index for Puffin and update the scenarios

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
import packaging.requirements
from pathlib import Path


PACKSE_COMMIT = "a9d2f659117693b89cba8a487200fd01444468af"
TOOL_ROOT = Path(__file__).parent
TEMPLATE = TOOL_ROOT / "template.mustache"
PACKSE = TOOL_ROOT / "packse-scenarios"
REQUIREMENTS = TOOL_ROOT / "requirements.txt"
PROJECT_ROOT = TOOL_ROOT.parent.parent
TARGET = PROJECT_ROOT / "crates" / "puffin-cli" / "tests" / "pip_install_scenarios.rs"

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
            "packse",
            "inspect",
            "--short-names",
            scenarios_path,
        ],
    )
)

# Add generated metadata
data["generated_from"] = f"https://github.com/zanieb/packse/tree/{commit}/scenarios"
data["generated_with"] = " ".join(sys.argv)


# Add normalized names for tests
for scenario in data["scenarios"]:
    scenario["normalized_name"] = scenario["name"].replace("-", "_")

# Drop the example scenario
for index, scenario in enumerate(data["scenarios"]):
    if scenario["name"] == "example":
        data["scenarios"].pop(index)

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

# Convert the expected packages into a list for rendering
for scenario in data["scenarios"]:
    expected = scenario["expected"]
    expected["packages_list"] = []
    for key, value in expected["packages"].items():
        expected["packages_list"].append(
            {
                "package": key,
                "version": value,
                # Include a converted version of the package name to its Python module
                "package_module": key.replace("-", "_"),
            }
        )


# Convert the required packages into a list without versions
for scenario in data["scenarios"]:
    requires_packages = scenario["root"]["requires_packages"] = []
    for requirement in scenario["root"]["requires"]:
        package = packaging.requirements.Requirement(requirement).name
        requires_packages.append(
            {"package": package, "package_module": package.replace("-", "_")}
        )


# Include the Python module name of the prefix
for scenario in data["scenarios"]:
    scenario["prefix_module"] = scenario["prefix"].replace("-", "_")


# Render the template
print("Rendering template...", file=sys.stderr)
output = chevron_blue.render(template=TEMPLATE.read_text(), data=data, no_escape=True)

# Update the test file
print(f"Updating test file at `{TARGET.relative_to(PROJECT_ROOT)}`...", file=sys.stderr)
with open(TARGET, "wt") as target_file:
    target_file.write(output)

# Format
print("Formatting test file...", file=sys.stderr)
subprocess.check_call(["rustfmt", str(TARGET)])

# Update snapshots
print("Updating snapshots...\n", file=sys.stderr)
subprocess.call(
    [
        "cargo",
        "insta",
        "test",
        "--accept",
        "--test",
        TARGET.with_suffix("").name,
        "--features",
        "pypi,python",
    ],
    cwd=PROJECT_ROOT,
)

print("\nDone!", file=sys.stderr)
