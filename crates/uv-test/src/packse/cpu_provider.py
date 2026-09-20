# From https://github.com/astral-sh/packse/pull/292.
import os
import sys
from dataclasses import dataclass


@dataclass
class VariantFeature:
    name: str
    values: list[str]
    multi_value: bool = False


namespace: str = "cpu"
is_build_plugin: bool = False


def get_all_configs() -> list[VariantFeature]:
    """Get all valid configs for the plugin"""
    raise NotImplementedError


def get_supported_configs() -> list[VariantFeature]:
    """Get supported configs for the current system"""
    level = os.getenv("PROVIDER_CPU_LEVEL")
    if not level:
        print("PROVIDER_CPU_LEVEL must be set", file=sys.stderr)
        sys.exit(1)
    level = int(level)
    return [VariantFeature("level", [f"v{i}" for i in range(level, 0, -1)])]
