from __future__ import annotations

from ._find_uv import find_uv_bin

__all__ = ["find_uv_bin"]


def __getattr__(attr_name: str) -> object:
    if attr_name in {
        "build_sdist",
        "build_wheel",
        "build_editable",
        "get_requires_for_build_sdist",
        "get_requires_for_build_wheel",
        "prepare_metadata_for_build_wheel",
        "get_requires_for_build_editable",
        "prepare_metadata_for_build_editable",
    }:
        err = f"Using `uv.{attr_name}` is not allowed. The uv build backend requires preview mode to be enabled, e.g., via the `UV_PREVIEW=1` environment variable."
        raise AttributeError(err)

    err = f"module 'uv' has no attribute '{attr_name}'"
    raise AttributeError(err)
