from __future__ import annotations

import shutil


def find_uv_bin() -> str:
    """Return the uv binary path."""
    
    path = shutil.which("uv")
    if path:
        return path

    raise FileNotFoundError("uv")


__all__ = [
    "find_uv_bin",
]
