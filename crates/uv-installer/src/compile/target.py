"""Identify settings compatible with serc's timestamp-based, unoptimized bytecode."""

import importlib.util
import json
import os
import sys

invalidation_mode = os.environ.get("PYC_INVALIDATION_MODE")
if invalidation_mode is None:
    invalidation_mode = (
        "CHECKED_HASH" if "SOURCE_DATE_EPOCH" in os.environ else "TIMESTAMP"
    )

if sys.implementation.name != "cpython":
    sys.exit(f"serc does not support {sys.implementation.name}; expected CPython")
if sys.flags.optimize != 0:
    sys.exit("serc does not support optimized bytecode (PYTHONOPTIMIZE)")
if getattr(sys, "pycache_prefix", None) is not None:
    sys.exit(
        "serc does not support a custom bytecode cache prefix (PYTHONPYCACHEPREFIX)"
    )
if sys._xoptions.get("no_debug_ranges", False) or os.environ.get("PYTHONNODEBUGRANGES"):
    sys.exit("serc does not support omitting debug ranges (PYTHONNODEBUGRANGES)")
if invalidation_mode != "TIMESTAMP":
    sys.exit(f"serc does not support bytecode invalidation mode {invalidation_mode}")

print(
    json.dumps(
        {
            "python_version": f"{sys.version_info.major}.{sys.version_info.minor}",
            "cache_tag": sys.implementation.cache_tag,
            "magic_number": list(importlib.util.MAGIC_NUMBER),
        }
    )
)
