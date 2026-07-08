#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = []
# ///

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import TypedDict


class Runner(TypedDict):
    label: str
    free: bool


class ConfigError(ValueError):
    pass


ROLE_PATTERN = re.compile(r"^[a-z0-9_]+(?:-[a-z0-9_]+)*$")


def load_config(path: Path) -> dict[str, list[Runner]]:
    try:
        with path.open(encoding="utf-8") as file:
            config = json.load(file)
    except OSError as error:
        raise ConfigError(f"Failed to read {path}: {error}") from error
    except json.JSONDecodeError as error:
        raise ConfigError(f"Failed to parse {path}: {error}") from error

    return validate_config(config)


def validate_config(config: object) -> dict[str, list[Runner]]:
    if not isinstance(config, dict) or not config:
        raise ConfigError("Runner configuration must be a non-empty object")

    validated: dict[str, list[Runner]] = {}
    output_names: set[str] = set()
    for role, candidates in config.items():
        if not isinstance(role, str) or not ROLE_PATTERN.fullmatch(role):
            raise ConfigError(f"Invalid runner role: {role!r}")
        if not isinstance(candidates, list) or not candidates:
            raise ConfigError(f"Runner role {role!r} must have at least one candidate")

        output_name = role.replace("-", "_")
        if output_name in output_names:
            raise ConfigError(f"Runner role {role!r} has a duplicate output name")
        output_names.add(output_name)

        validated_candidates: list[Runner] = []
        labels: set[str] = set()
        for index, candidate in enumerate(candidates):
            if not isinstance(candidate, dict):
                raise ConfigError(
                    f"Runner candidate {index} for {role!r} must be an object"
                )

            missing = {"label", "free"} - candidate.keys()
            unknown = candidate.keys() - {"label", "free"}
            if missing:
                raise ConfigError(
                    f"Runner candidate {index} for {role!r} is missing: {', '.join(sorted(missing))}"
                )
            if unknown:
                raise ConfigError(
                    f"Runner candidate {index} for {role!r} has unknown fields: "
                    f"{', '.join(sorted(unknown))}"
                )

            label = candidate["label"]
            free = candidate["free"]
            if (
                not isinstance(label, str)
                or not label
                or "\n" in label
                or "\r" in label
            ):
                raise ConfigError(
                    f"Runner candidate {index} for {role!r} has an invalid label"
                )
            if not isinstance(free, bool):
                raise ConfigError(
                    f"Runner candidate {index} for {role!r} has a non-boolean free field"
                )
            if label in labels:
                raise ConfigError(f"Runner role {role!r} repeats label {label!r}")
            labels.add(label)
            validated_candidates.append({"label": label, "free": free})

        validated[role] = validated_candidates

    return validated


def resolve_runners(
    config: dict[str, list[Runner]], *, free_only: bool
) -> dict[str, str]:
    resolved: dict[str, str] = {}
    for role, candidates in config.items():
        candidate = next(
            (
                candidate
                for candidate in candidates
                if not free_only or candidate["free"]
            ),
            None,
        )
        if candidate is not None:
            resolved[role] = candidate["label"]
    return resolved


def parse_bool(value: str) -> bool:
    if value.lower() == "true":
        return True
    if value.lower() == "false":
        return False
    raise argparse.ArgumentTypeError("expected true or false")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("config", type=Path)
    parser.add_argument(
        "--free-only", type=parse_bool, default=False, metavar="{true,false}"
    )
    arguments = parser.parse_args(argv)

    try:
        config = load_config(arguments.config)
    except ConfigError as error:
        parser.error(str(error))

    for role, label in resolve_runners(config, free_only=arguments.free_only).items():
        print(f"runner_{role.replace('-', '_')}={label}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
