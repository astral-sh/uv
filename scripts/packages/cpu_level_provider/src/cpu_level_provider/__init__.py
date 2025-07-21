from __future__ import annotations

import os
import platform
import sys
from dataclasses import dataclass

namespace: str = "cpu_level"
dynamic: bool = False


@dataclass
class VariantFeatureConfigType:
    name: str
    values: list[str]


@dataclass
class VariantPropertyType:
    name: str
    values: list[str]


def get_supported_configs(
    known_properties: frozenset[VariantPropertyType] | None,
) -> list[VariantFeatureConfigType]:
    if known_properties is not None:
        raise ValueError("known_properties are unsupported")
    if override := os.getenv("UV_CPU_LEVEL_OVERRIDE"):
        try:
            current_level = int(override)
        except ValueError:
            raise ValueError(
                f"Invalid CPU level override (`UV_CPU_LEVEL_OVERRIDE`): {override}"
            )
    else:
        current_level = get_x86_64_level() or 1
    if current_level is None:
        return []

    supported_levels = [f"v{i}" for i in range(current_level, 0, -1)]
    return [VariantFeatureConfigType(name="x86_64_level", values=supported_levels)]


def get_x86_64_level() -> int | None:
    """Returns the highest supported x86_64 microarchitecture level (v1, v2, v3, or v4).

    TODO(konsti): Replace with non-slop"""
    if platform.machine() not in ("x86_64", "AMD64"):
        return None

    # Get CPU features from /proc/cpuinfo on Linux
    if sys.platform == "linux":
        with open("/proc/cpuinfo", "r") as f:
            cpuinfo = f.read()

        # Extract flags line
        flags_line = None
        for line in cpuinfo.splitlines():
            if line.startswith("flags"):
                flags_line = line
                break

        if not flags_line:
            return 1

        flags = set(flags_line.split()[2:])  # Skip "flags" and ":"

    else:
        # For other platforms, we can't easily detect features
        return None

    # Check for v4 features (AVX512)
    v4_features = {"avx512f", "avx512bw", "avx512cd", "avx512dq", "avx512vl"}
    if v4_features.issubset(flags):
        return 4

    # Check for v3 features (AVX2)
    v3_features = {"avx", "avx2", "bmi1", "bmi2", "fma", "movbe"}
    if v3_features.issubset(flags):
        return 3

    # Check for v2 features (SSE4.2)
    v2_features = {
        "cmpxchg16b",
        "lahf_lm",
        "popcnt",
        "sse3",
        "ssse3",
        "sse4_1",
        "sse4_2",
    }
    if v2_features.issubset(flags):
        return 2

    # Default to v1 (baseline x86_64)
    return 1


def main():
    print(get_supported_configs(None))


if __name__ == "__main__":
    main()
