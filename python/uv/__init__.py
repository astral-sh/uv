from __future__ import annotations

import os

if os.environ.get("UV_PREVIEW"):
    from ._build_backend import *
from ._find_uv import find_uv_bin

if os.environ.get("UV_PREVIEW"):
    __all__ = [
        "find_uv_bin",
        # PEP 517 hook `build_sdist`.
        "build_sdist",
        # PEP 517 hook `build_wheel`.
        "build_wheel",
        # PEP 660 hook `build_editable`.
        "build_editable",
        # PEP 517 hook `get_requires_for_build_sdist`.
        "get_requires_for_build_sdist",
        # PEP 517 hook `get_requires_for_build_wheel`.
        "get_requires_for_build_wheel",
        # PEP 517 hook `prepare_metadata_for_build_wheel`.
        "prepare_metadata_for_build_wheel",
        # PEP 660 hook `get_requires_for_build_editable`.
        "get_requires_for_build_editable",
        # PEP 660 hook `prepare_metadata_for_build_editable`.
        "prepare_metadata_for_build_editable",
    ]
else:
    __all__ = ["find_uv_bin"]
