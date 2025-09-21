#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.13"
# ///

import argparse
import difflib
import json
import re
import sys
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("base", type=Path)
    parser.add_argument("branch", type=Path)
    parser.add_argument("--project", action="store_true")
    parser.add_argument(
        "--markdown",
        action="store_true",
    )
    parser.add_argument(
        "--limit",
        type=int,
        default=None,
    )
    args = parser.parse_args()

    # Supress noise from fluctuations in execution time
    redact_time = re.compile(r"(0\.)?(\d+)ms|(\d+).(\d+)s")

    parameters = json.loads(args.base.joinpath("parameters.json").read_text())

    total = 0
    successful = 0
    differences = []
    files = sorted(dir for dir in args.base.iterdir() if dir.is_dir())
    for package_dir in files[: args.limit]:
        package = package_dir.name
        package_branch = args.branch.joinpath(package)
        if not package_branch.is_dir():
            print(f"Package {package} not found in branch")
            continue

        total += 1

        summary = package_dir.joinpath("summary.json").read_text()
        if json.loads(summary)["exit_code"] == 0:
            successful += 1
        else:
            # Don't show differences in the error messages,
            # also `uv.lock` doesn't exist for failed resolutions
            continue

        if args.project:
            resolution = package_dir.joinpath("uv.lock").read_text()
            if package_dir.joinpath("stdout.txt").read_text().strip():
                raise RuntimeError(f"Stdout not empty (base): {package}")
        else:
            resolution = package_dir.joinpath("stdout.txt").read_text()
        stderr = package_dir.joinpath("stderr.txt").read_text()
        stderr = redact_time.sub(r"[TIME]", stderr)

        if args.project:
            resolution_branch = package_branch.joinpath("uv.lock").read_text()
            if package_branch.joinpath("stdout.txt").read_text().strip():
                raise RuntimeError(f"Stdout not empty (branch): {package}")
        else:
            resolution_branch = package_branch.joinpath("stdout.txt").read_text()
        stderr_branch = package_branch.joinpath("stderr.txt").read_text()
        stderr_branch = redact_time.sub(r"[TIME]", stderr_branch)

        if resolution != resolution_branch or stderr != stderr_branch:
            differences.append(
                (package, resolution, resolution_branch, stderr, stderr_branch)
            )

    if args.markdown:
        print("# Ecosystem testing report")
        print(
            f"Dataset: "
            f"`{'uv pip compile' if not parameters['project'] else 'uv lock'}` with `--no-build` "
            f"on each of the top 15k PyPI packages on Python {parameters['python']} "
            "pinned to the latest package version. "
            if parameters["latest"]
            else ". "
            "A handful of pathological cases were filtered out. "
            "Only success resolutions can be compared.\n"
        )
        print(f"Successfully resolved packages: {successful}/{total}\n")
        print(f"Different packages: {len(differences)}/{total}\n")

        for (
            package,
            resolution,
            resolution_branch,
            stderr,
            stderr_branch,
        ) in differences:
            if args.project:
                context_window = 3
            else:
                context_window = 999999
            print(f"\n<details>\n<summary>{package}</summary>\n")
            if resolution != resolution_branch:
                print("```diff")
                sys.stdout.writelines(
                    difflib.unified_diff(
                        resolution.splitlines(keepends=True),
                        resolution_branch.splitlines(keepends=True),
                        fromfile="base",
                        tofile="branch",
                        # Show the dependencies in full
                        n=context_window,
                    )
                )
                print("```")
            if stderr != stderr_branch:
                print("```diff")
                sys.stdout.writelines(
                    difflib.unified_diff(
                        stderr.splitlines(keepends=True),
                        stderr_branch.splitlines(keepends=True),
                        fromfile="base",
                        tofile="branch",
                        # Show the log in full
                        n=context_window,
                    )
                )
                print("```")
            print("</details>")
    else:
        for (
            package,
            resolution,
            resolution_branch,
            stderr,
            stderr_branch,
        ) in differences:
            print("--------------------------------")
            print(f"Package {package}")
            if resolution != resolution_branch:
                sys.stdout.writelines(
                    difflib.unified_diff(
                        resolution.splitlines(keepends=True),
                        resolution_branch.splitlines(keepends=True),
                        fromfile="base",
                        tofile="branch",
                    )
                )
            if stderr != stderr_branch:
                sys.stdout.writelines(
                    difflib.unified_diff(
                        stderr.splitlines(keepends=True),
                        stderr_branch.splitlines(keepends=True),
                        fromfile="base",
                        tofile="branch",
                    )
                )
        print(
            f"Successfully resolved packages: {successful}/{total} ({successful}/{total}:.0%)"
        )
        print(f"Different packages: {len(differences)}/{total}")


if __name__ == "__main__":
    main()
