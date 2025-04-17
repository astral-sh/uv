import sys

from hatchling.build import *  # noqa:F401,F403


def build_wheel(wheel_directory, config_settings, metadata_directory) -> str:
    print(
        "This package is a placeholder to prevent dependency confusion with `uvx`. "
        "Please refer to https://github.com/astral-sh/uv for installing uv and uvx.",
        file=sys.stderr,
    )
    sys.exit(1)


def prepare_metadata_for_build_wheel(metadata_directory, config_settings) -> str:
    print(
        "This package is a placeholder to prevent dependency confusion with `uvx`. "
        "Please refer to https://github.com/astral-sh/uv for installing uv and uvx.",
        file=sys.stderr,
    )
    sys.exit(1)
