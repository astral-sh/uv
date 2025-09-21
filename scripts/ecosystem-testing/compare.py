import argparse
import difflib
import sys
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("base", type=Path)
    parser.add_argument("branch", type=Path)
    args = parser.parse_args()

    total = 0
    differences = 0
    for package in args.base.iterdir():
        if not package.is_dir():
            continue
        package_branch = args.branch.joinpath(package.name)
        if not package_branch.is_dir():
            print(f"Package {package} not found in branch")
            continue
        stdout = package.joinpath("stdout.txt").read_text()
        stdout_branch = package_branch.joinpath("stdout.txt").read_text()
        if stdout != stdout_branch:
            differences += 1
            print("--------------------------------")
            print(f"Package {package}")
            sys.stdout.writelines(
                difflib.unified_diff(
                    stdout.splitlines(keepends=True),
                    stdout_branch.splitlines(keepends=True),
                    fromfile="base",
                    tofile="branch",
                )
            )
        total += 1

    print(f"Different packages: {differences}/{total}")


if __name__ == "__main__":
    main()
