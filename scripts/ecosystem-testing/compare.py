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
    parser.add_argument(
        "--markdown",
        action="store_true",
        help="Output in markdown format with collapsible sections",
    )
    parser.add_argument(
        "--show-failed",
        action="store_true",
        help="Show failed packages",
    )
    args = parser.parse_args()

    redact_time = re.compile(r"(0\.)?(\d+)ms")

    total = 0
    successful = 0
    differences = []
    for package_dir in args.base.iterdir():
        if not package_dir.is_dir():
            continue
        package = package_dir.name
        package_branch = args.branch.joinpath(package)
        if not package_branch.is_dir():
            print(f"Package {package} not found in branch")
            continue

        total += 1

        stdout = package_dir.joinpath("stdout.txt").read_text()
        stderr = package_dir.joinpath("stderr.txt").read_text()
        stderr = redact_time.sub(r"[TIME]", stderr)
        summary = package_dir.joinpath("summary.json").read_text()
        stdout_branch = package_branch.joinpath("stdout.txt").read_text()
        stderr_branch = package_branch.joinpath("stderr.txt").read_text()
        stderr_branch = redact_time.sub(r"[TIME]", stderr_branch)

        if json.loads(summary)["exit_code"] == 0:
            successful += 1
        elif not args.show_failed:
            # Don't show differences in the error messages by default
            continue

        if stdout != stdout_branch or stderr != stderr_branch:
            differences.append((package, stdout, stdout_branch, stderr, stderr_branch))

    if args.markdown:
        print("# Ecosystem testing report")
        print(
            "Dataset: `uv pip compile` on each of the top 15k PyPI packages on Python 3.13 with `--no-build`, "
            "a handful of pathological cases filtered. "
            "Only success resolutions can be compared.\n"
        )
        print(f"Successfully resolved packages: {successful}/{total}\n")
        print(f"Different packages: {len(differences)}/{total}\n")

        for package, stdout, stdout_branch, stderr, stderr_branch in differences:
            print(f"\n<details>\n<summary>{package}</summary>\n")
            if stdout != stdout_branch:
                print("```diff")
                sys.stdout.writelines(
                    difflib.unified_diff(
                        stdout.splitlines(keepends=True),
                        stdout_branch.splitlines(keepends=True),
                        fromfile="base",
                        tofile="branch",
                        # Show the dependencies in full
                        n=999999,
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
                        n=999999,
                    )
                )
                print("```")
            print("</details>")
    else:
        for package, stdout, stdout_branch, stderr, stderr_branch in differences:
            print("--------------------------------")
            print(f"Package {package}")
            if stdout != stdout_branch:
                sys.stdout.writelines(
                    difflib.unified_diff(
                        stdout.splitlines(keepends=True),
                        stdout_branch.splitlines(keepends=True),
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
        print(f"Successfully resolved packages: {successful}/{total}")
        print(f"Different packages: {len(differences)}/{total}")


if __name__ == "__main__":
    main()
