"""
The Python API of `uv` is not guaranteed to be stable and may chance at any time. It is 
strongly discouraged to import from our `uv` Python module, but you can do so at your 
own risk.
"""
from uv.__main__ import detect_virtualenv, find_uv_bin

__all__ = ["detect_virtualenv", "find_uv_bin"]
