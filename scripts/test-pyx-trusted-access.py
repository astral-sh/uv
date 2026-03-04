# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
import logging
import os
import subprocess
import tempfile
from argparse import ArgumentParser
from pathlib import Path


def setup_test_project(
    registry_name: str, registry_url: str, project_dir: str, requires_python: str
):
    """Create a temporary project directory with a pyproject.toml"""
    pyproject_content = f"""[project]
name = "{registry_name}-test"
version = "0.1.0"
description = "Test registry"
requires-python = ">={requires_python}"

[[tool.uv.index]]
name = "{registry_name}"
url = "{registry_url}"
default = true
"""
    pyproject_file = Path(project_dir) / "pyproject.toml"
    pyproject_file.write_text(pyproject_content)


def test_github_trusted_access_view(uv: Path) -> None:
    """
    Test that we can authenticate to a pyx view via Trusted Access on GitHub Actions.
    """

    # Check that we're running on GitHub Actions.
    if "GITHUB_ACTIONS" not in os.environ:
        logging.warning("Not running on GitHub Actions, skipping test.")
        return

    with tempfile.TemporaryDirectory() as project_dir:
        setup_test_project(
            registry_name="pyx",
            registry_url="https://api.pyx.dev/simple/astral-test/uv-e2e-test",
            project_dir=project_dir,
            requires_python="3.12",
        )

        command = [
            uv,
            "add",
            "hyperion",
            "--directory",
            project_dir,
            "--no-cache",
        ]
        subprocess.run(command, capture_output=True, text=True, check=True)


def main() -> None:
    logging.basicConfig(
        format="%(levelname)s [%(asctime)s] %(name)s - %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
        level=logging.INFO,
    )

    parser = ArgumentParser()
    parser.add_argument("--uv", required=True)
    args = parser.parse_args()

    uv = Path.cwd().joinpath(args.uv)
    assert uv.is_file(), f"`uv` not found: {uv}"

    test_github_trusted_access_view(uv)


if __name__ == "__main__":
    main()
