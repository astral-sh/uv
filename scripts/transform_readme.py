"""Transform the README.md to support a specific deployment target.

By default, we assume that our README.md will be rendered on GitHub. However, different
targets have different strategies for rendering light- and dark-mode images. This script
adjusts the images in the README.md to support the given target.
"""

from __future__ import annotations

import argparse
import re
import tomllib
import urllib.parse
from pathlib import Path

# The benchmark image URL in the README. This SVG uses CSS media queries to adapt to dark/light
# mode. PyPI doesn't support this, so we replace it with a light-only version.
# See: https://github.com/pypi/warehouse/issues/11251
BENCHMARK_URL = "./assets/svg/Benchmark-Reactive.svg"
BENCHMARK_URL_LIGHT = "https://raw.githubusercontent.com/astral-sh/uv/refs/heads/main/assets/svg/Benchmark-Light.svg"


def main(target: str) -> None:
    """Modify the README.md to support the given target."""

    with Path("README.md").open(encoding="utf8") as fp:
        content = fp.read()

    if target == "pypi":
        # Replace the benchmark image URL with the light-only version for PyPI.
        if BENCHMARK_URL not in content:
            msg = "README.md is not in the expected format (benchmark image not found)."
            raise ValueError(msg)
        content = content.replace(BENCHMARK_URL, BENCHMARK_URL_LIGHT)
    else:
        msg = f"Unknown target: {target}"
        raise ValueError(msg)

    # Read the current version from the `pyproject.toml`.
    with Path("pyproject.toml").open(mode="rb") as fp:
        # Parse the TOML.
        pyproject = tomllib.load(fp)
        if "project" in pyproject and "version" in pyproject["project"]:
            version = pyproject["project"]["version"]
        else:
            raise ValueError("Version not found in pyproject.toml")

    # Replace the badges with versioned URLs.
    for existing, replacement in [
        (
            "https://img.shields.io/pypi/v/uv.svg",
            f"https://img.shields.io/pypi/v/uv/{version}.svg",
        ),
        (
            "https://img.shields.io/pypi/l/uv.svg",
            f"https://img.shields.io/pypi/l/uv/{version}.svg",
        ),
        (
            "https://img.shields.io/pypi/pyversions/uv.svg",
            f"https://img.shields.io/pypi/pyversions/uv/{version}.svg",
        ),
    ]:
        if existing not in content:
            raise ValueError(f"Badge not found in README.md: {existing}")
        content = content.replace(existing, replacement)

    # Replace any relative URLs (e.g., `[PIP_COMPATIBILITY.md`) with absolute URLs.
    def replace(match: re.Match) -> str:
        url = match.group(1)
        if not url.startswith("http"):
            url = urllib.parse.urljoin(
                f"https://github.com/astral-sh/uv/blob/{version}/README.md", url
            )
        return f"]({url})"

    content = re.sub(r"]\(([^)]+)\)", replace, content)

    with Path("README.md").open("w", encoding="utf8") as fp:
        fp.write(content)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Modify the README.md to support a specific deployment target.",
    )
    parser.add_argument(
        "--target",
        type=str,
        required=True,
        choices=("pypi", "mkdocs"),
    )
    args = parser.parse_args()

    main(target=args.target)
