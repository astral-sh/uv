"""Transform the README.md to support a specific deployment target.

By default, we assume that our README.md will be rendered on GitHub. However, different
targets have different strategies for rendering light- and dark-mode images. This script
adjusts the images in the README.md to support the given target.
"""

from __future__ import annotations

import argparse
import re
import urllib.parse
from pathlib import Path

import tomllib

# To be kept in sync with: `docs/index.md`
URL = "https://github.com/astral-sh/uv/assets/1309177/{}"
URL_LIGHT = URL.format("629e59c0-9c6e-4013-9ad4-adb2bcf5080d")
URL_DARK = URL.format("03aa9163-1c79-4a87-a31d-7a9311ed9310")

# https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#specifying-the-theme-an-image-is-shown-to
GITHUB = f"""
<p align="center">
  <picture align="center">
    <source media="(prefers-color-scheme: dark)" srcset="{URL_DARK}">
    <source media="(prefers-color-scheme: light)" srcset="{URL_LIGHT}">
    <img alt="Shows a bar chart with benchmark results." src="{URL_LIGHT}">
  </picture>
</p>
"""

# https://github.com/pypi/warehouse/issues/11251
PYPI = f"""
<p align="center">
  <img alt="Shows a bar chart with benchmark results." src="{URL_LIGHT}">
</p>
"""


def main(target: str) -> None:
    """Modify the README.md to support the given target."""

    # Replace the benchmark images based on the target.
    with Path("README.md").open(encoding="utf8") as fp:
        content = fp.read()
        if GITHUB not in content:
            msg = "README.md is not in the expected format."
            raise ValueError(msg)

    if target == "pypi":
        content = content.replace(GITHUB, PYPI)
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
