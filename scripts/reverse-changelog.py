"""
Reverse the ordering of versions in a changelog file, i.e., when archiving a changelog.
"""

import re
import sys


def parse_changelog(content):
    """Parse the changelog content into individual version blocks."""
    # Use regex to split the content by version headers
    version_pattern = r"(?=## \d+\.\d+\.\d+)"
    version_blocks = re.split(version_pattern, content)

    # First item in the list is the header, which we want to preserve
    header = version_blocks[0]
    version_blocks = version_blocks[1:]

    return header, version_blocks


def reverse_changelog(content):
    """Reverse the order of version blocks in the changelog."""
    header, version_blocks = parse_changelog(content)

    # Reverse the version blocks
    reversed_blocks = version_blocks[::-1]

    # Combine the header and reversed blocks
    reversed_content = header + "".join(reversed_blocks)

    return reversed_content


def main():
    if len(sys.argv) < 2:
        print("Usage: reverse-changelog.py <changelog-file>")
        sys.exit(1)

    # Read the input file
    name = sys.argv[1]
    with open(name, "r") as file:
        content = file.read()

    # Reverse the changelog
    reversed_content = reverse_changelog(content)

    # Write the output to a new file
    with open(name, "w") as file:
        file.write(reversed_content)

    print(f"Updated {name}")


if __name__ == "__main__":
    main()
