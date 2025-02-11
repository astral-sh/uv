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
        err = (
            f"Using `uv.{attr_name}` is not allowed. Consider using `uv_build` instead."
        )
        raise AttributeError(err)
