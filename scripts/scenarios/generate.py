#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.12"
# dependencies = [
#     "chevron-blue>=0.2.1",
#     "packaging>=24.0",
# ]
# ///
"""
Generates and updates snapshot test cases from packse scenarios.

This script parses packse scenario TOML files directly (no packse dependency)
and renders Mustache templates into Rust test files.

Usage:

    Regenerate the scenario test files (with snapshot updates):

        $ uv run scripts/scenarios/generate.py

    Skip snapshot updates (just regenerate .rs files):

        $ uv run scripts/scenarios/generate.py --no-snapshot-update
"""

from __future__ import annotations

import argparse
import logging
import subprocess
import sys
import tomllib
from dataclasses import dataclass, field
from enum import StrEnum, auto
from pathlib import Path
from typing import Any

import chevron_blue
from packaging.requirements import Requirement

TOOL_ROOT = Path(__file__).parent
TEMPLATES = TOOL_ROOT / "templates"
PROJECT_ROOT = TOOL_ROOT.parent.parent
SCENARIOS_DIR = PROJECT_ROOT / "test" / "scenarios"
TESTS = PROJECT_ROOT / "crates" / "uv" / "tests" / "it"


class TemplateKind(StrEnum):
    install = auto()
    compile = auto()
    lock = auto()

    def template_file(self) -> Path:
        return TEMPLATES / f"{self.name}.mustache"

    def test_file(self) -> Path:
        match self.value:
            case TemplateKind.install:
                return TESTS / "pip_install_scenarios.rs"
            case TemplateKind.compile:
                return TESTS / "pip_compile_scenarios.rs"
            case TemplateKind.lock:
                return TESTS / "lock_scenarios.rs"
            case _:
                raise NotImplementedError()


@dataclass
class PackageMetadata:
    requires_python: str | None = ">=3.12"
    requires: list[str] = field(default_factory=list)
    extras: dict[str, list[str]] = field(default_factory=dict)
    sdist: bool = True
    wheel: bool = True
    yanked: bool = False
    wheel_tags: list[str] = field(default_factory=list)
    description: str = ""

    @classmethod
    def from_dict(cls, data: dict[str, Any] | None) -> PackageMetadata:
        values = data or {}
        return cls(
            requires_python=values.get("requires_python", ">=3.12"),
            requires=values.get("requires", []),
            extras=values.get("extras", {}),
            sdist=values.get("sdist", True),
            wheel=values.get("wheel", True),
            yanked=values.get("yanked", False),
            wheel_tags=values.get("wheel_tags", []),
            description=values.get("description", ""),
        )


@dataclass
class Package:
    versions: dict[str, PackageMetadata]

    @classmethod
    def from_dict(cls, data: dict[str, Any] | None) -> Package:
        values = data or {}
        versions = {
            version: PackageMetadata.from_dict(metadata)
            for version, metadata in values.get("versions", {}).items()
        }
        return cls(versions=versions)


@dataclass
class RootPackage:
    requires_python: str | None = ">=3.12"
    requires: list[str] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: dict[str, Any] | None) -> RootPackage:
        values = data or {}
        return cls(
            requires_python=values.get("requires_python", ">=3.12"),
            requires=values.get("requires", []),
        )


@dataclass
class Expected:
    satisfiable: bool
    packages: dict[str, str] = field(default_factory=dict)
    explanation: str | None = None

    @classmethod
    def from_dict(cls, data: dict[str, Any] | None) -> Expected:
        values = data or {}
        return cls(
            satisfiable=values.get("satisfiable", True),
            packages=values.get("packages", {}),
            explanation=values.get("explanation"),
        )


@dataclass
class Environment:
    python: str = "3.12"
    additional_python: list[str] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: dict[str, Any] | None) -> Environment:
        values = data or {}
        return cls(
            python=values.get("python", "3.12"),
            additional_python=values.get("additional_python", []),
        )


@dataclass
class ResolverOptions:
    python: str | None = None
    prereleases: bool = False
    no_build: list[str] = field(default_factory=list)
    no_binary: list[str] = field(default_factory=list)
    universal: bool = False
    python_platform: str | None = None
    required_environments: list[str] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: dict[str, Any] | None) -> ResolverOptions:
        values = data or {}
        return cls(
            python=values.get("python"),
            prereleases=values.get("prereleases", False),
            no_build=values.get("no_build", []),
            no_binary=values.get("no_binary", []),
            universal=values.get("universal", False),
            python_platform=values.get("python_platform"),
            required_environments=values.get("required_environments", []),
        )


@dataclass
class Scenario:
    name: str
    packages: dict[str, Package]
    root: RootPackage
    expected: Expected
    environment: Environment = field(default_factory=Environment)
    resolver_options: ResolverOptions = field(default_factory=ResolverOptions)
    description: str | None = None
    source_path: Path | None = None

    @classmethod
    def from_dict(
        cls,
        data: dict[str, Any],
        source_path: Path | None = None,
    ) -> Scenario:
        packages = {
            package_name: Package.from_dict(package_data)
            for package_name, package_data in data.get("packages", {}).items()
        }
        return cls(
            name=data["name"],
            packages=packages,
            root=RootPackage.from_dict(data.get("root")),
            expected=Expected.from_dict(data.get("expected")),
            environment=Environment.from_dict(data.get("environment")),
            resolver_options=ResolverOptions.from_dict(data.get("resolver_options")),
            description=data.get("description"),
            source_path=source_path,
        )

    @classmethod
    def from_path(
        cls,
        path: Path,
        *,
        scenarios_dir: Path = SCENARIOS_DIR,
    ) -> Scenario:
        with open(path, "rb") as file:
            data = tomllib.load(file)
        return cls.from_dict(data, source_path=path.relative_to(scenarios_dir))

    @classmethod
    def load_all(cls, scenarios_dir: Path = SCENARIOS_DIR) -> list[Scenario]:
        scenarios = []
        for path in sorted(scenarios_dir.rglob("*.toml")):
            try:
                scenarios.append(cls.from_path(path, scenarios_dir=scenarios_dir))
            except Exception as exc:
                logging.warning(f"Skipping {path}: {exc}")
        return scenarios

    def to_pretty_tree(self) -> str:
        space = "    "
        branch = "│   "
        tee = "├── "
        last = "└── "
        buffer = ""

        all_packages: dict[str, Package] = dict(self.packages)
        all_packages["root"] = Package(
            versions={
                "0.0.0": PackageMetadata(
                    requires=self.root.requires,
                    requires_python=self.root.requires_python,
                )
            }
        )

        def render_versions(
            package: str,
            prefix: str = "",
            for_requirement: Requirement | None = None,
        ):
            versions = all_packages[package].versions
            if for_requirement:
                spec = for_requirement.specifier
                versions = {
                    version: metadata
                    for version, metadata in versions.items()
                    if spec.contains(version) and not metadata.yanked
                }
                if not versions:
                    yield prefix + last + "unsatisfied: no matching version"
                    return

            show_versions: list[tuple[str, list[str], PackageMetadata]] = []
            for version, metadata in versions.items():
                dependencies = list(metadata.requires)
                if metadata.requires_python:
                    dependencies.append(f"python{metadata.requires_python}")
                show_versions.append((version, dependencies, metadata))

                for extra, extra_dependencies in metadata.extras.items():
                    show_versions.append(
                        (f"{version}[{extra}]", list(extra_dependencies), metadata)
                    )

            pointers = [tee] * (len(show_versions) - 1) + [last]
            for pointer, (version, requirements, metadata) in zip(
                pointers, show_versions, strict=True
            ):
                message = "satisfied by " if for_requirement and package else ""
                yanked = " (yanked)" if metadata.yanked else ""
                yield prefix + pointer + message + f"{package}-{version}" + yanked

                if not for_requirement:
                    extension = branch if pointer == tee else space
                    yield from render_requirements(
                        requirements,
                        prefix=prefix + extension,
                    )

        def render_requirements(requirements: list[str], prefix: str = ""):
            parsed_requirements = []
            for requirement in requirements:
                try:
                    parsed_requirements.append(Requirement(requirement))
                except Exception:
                    parsed_requirements.append(
                        Requirement("invalid ; python_version == '0'")
                    )

            filtered_requirements = []
            for requirement in parsed_requirements:
                if requirement.name == "python":
                    specifier = requirement.specifier
                    if (
                        specifier.contains(self.environment.python)
                        and len(list(specifier)) == 1
                    ):
                        spec = next(iter(specifier))
                        if spec.version == self.environment.python:
                            continue
                filtered_requirements.append(requirement)

            if not filtered_requirements:
                return

            pointers = [tee] * (len(filtered_requirements) - 1) + [last]
            for pointer, requirement in zip(
                pointers,
                sorted(filtered_requirements, key=lambda req: req.name),
                strict=True,
            ):
                if requirement.name == "python":
                    suffix = ""
                    if not requirement.specifier.contains(self.environment.python):
                        suffix = " (incompatible with environment)"
                    yield prefix + pointer + "requires " + str(requirement) + suffix
                    continue

                yield prefix + pointer + "requires " + str(requirement)

                if requirement.name in all_packages:
                    extension = branch if pointer == tee else space
                    yield from render_versions(
                        requirement.name,
                        prefix=prefix + extension,
                        for_requirement=requirement,
                    )
                else:
                    yield prefix + space + last + "unsatisfied: no versions for package"

        pointer = tee
        buffer += pointer + "environment\n"
        prefix = branch
        if self.environment.additional_python:
            python_versions = self.environment.additional_python + [
                self.environment.python
            ]
            pointers_list = [tee] * (len(python_versions) - 1) + [last]
            for tree_pointer, version in zip(
                pointers_list, sorted(python_versions), strict=True
            ):
                active = " (active)" if version == self.environment.python else ""
                buffer += prefix + tree_pointer + f"python{version}" + active + "\n"
        else:
            buffer += prefix + last + f"python{self.environment.python}\n"

        pointer = tee if self.packages else last
        buffer += pointer + "root\n"
        prefix = branch if pointer == tee else space
        for line in render_requirements(self.root.requires, prefix=prefix):
            buffer += line + "\n"

        package_names = sorted(self.packages.keys())
        pointers_list = [tee] * (len(package_names) - 1) + [last]
        for pointer, package_name in zip(pointers_list, package_names, strict=True):
            buffer += pointer + package_name + "\n"
            prefix = branch if pointer == tee else space
            for line in render_versions(package_name, prefix=prefix):
                buffer += line + "\n"

        return buffer

    def to_template_variables(self) -> dict[str, Any]:
        raw: dict[str, Any] = {
            "name": self.name,
            "module_name": self.name.replace("-", "_"),
            "scenario_path": self.source_path.as_posix() if self.source_path else "",
        }

        if self.description:
            raw["description"] = "\n/// ".join(self.description.splitlines())
        else:
            raw["description"] = self.name

        raw["tree"] = self.to_pretty_tree().splitlines()

        raw["environment"] = {
            "python": self.environment.python,
            "additional_python": self.environment.additional_python,
        }

        raw["python_patch"] = "patch" in self.name

        raw["root"] = {
            "requires": [
                {
                    "requirement": requirement,
                    "name": Requirement(requirement).name,
                    "module_name": Requirement(requirement).name.replace("-", "_"),
                }
                for requirement in self.root.requires
            ],
            "requires_python": self.root.requires_python,
        }

        resolver_options = self.resolver_options
        raw["resolver_options"] = {
            "python": resolver_options.python,
            "prereleases": resolver_options.prereleases,
            "no_build": resolver_options.no_build,
            "no_binary": resolver_options.no_binary,
            "universal": resolver_options.universal,
            "python_platform": resolver_options.python_platform,
            "required_environments": resolver_options.required_environments,
            "has_required_environments": bool(resolver_options.required_environments),
        }

        raw["expected"] = {
            "satisfiable": self.expected.satisfiable,
            "packages": [
                {
                    "name": name,
                    "version": version,
                    "module_name": name.replace("-", "_"),
                }
                for name, version in self.expected.packages.items()
            ],
            "explanation": self.expected.explanation,
        }
        if self.expected.explanation:
            raw["expected"]["explanation"] = "\n// ".join(
                self.expected.explanation.splitlines()
            )

        return raw


def main(
    template_kinds: list[TemplateKind],
    snapshot_update: bool = True,
):
    debug = logging.getLogger().getEffectiveLevel() <= logging.DEBUG

    logging.info("Loading scenarios from %s", SCENARIOS_DIR)
    scenarios = Scenario.load_all(SCENARIOS_DIR)
    logging.info("Loaded %d scenarios", len(scenarios))

    install_scenarios = []
    compile_scenarios = []
    lock_scenarios = []

    for scenario in scenarios:
        if scenario.resolver_options.universal:
            lock_scenarios.append(scenario)
        elif scenario.resolver_options.python is not None:
            compile_scenarios.append(scenario)
        else:
            install_scenarios.append(scenario)

    template_kinds_and_scenarios: list[tuple[TemplateKind, list[Scenario]]] = [
        (TemplateKind.install, install_scenarios),
        (TemplateKind.compile, compile_scenarios),
        (TemplateKind.lock, lock_scenarios),
    ]

    for template_kind, kind_scenarios in template_kinds_and_scenarios:
        if template_kind not in template_kinds:
            continue

        data: dict[str, Any] = {
            "scenarios": [
                scenario.to_template_variables() for scenario in kind_scenarios
            ],
            "generated_from": str(SCENARIOS_DIR.relative_to(PROJECT_ROOT)),
            "generated_with": "uv run scripts/scenarios/generate.py",
        }

        logging.info(
            f"Rendering template {template_kind.name} ({len(kind_scenarios)} scenarios)"
        )
        output = chevron_blue.render(
            template=template_kind.template_file().read_text(),
            data=data,
            no_escape=True,
            warn=True,
        )

        logging.info(
            f"Updating test file at `{template_kind.test_file().relative_to(PROJECT_ROOT)}`..."
        )
        with open(template_kind.test_file(), "w") as test_file:
            test_file.write(output)

        logging.info("Formatting test file...")
        subprocess.check_call(
            ["rustfmt", template_kind.test_file()],
            stderr=subprocess.STDOUT,
            stdout=sys.stderr if debug else subprocess.DEVNULL,
        )

        if snapshot_update:
            logging.info("Updating snapshots...")
            command = [
                "cargo",
                "insta",
                "test",
                "--features",
                "test-python,test-python-patch",
                "--accept",
                "--test-runner",
                "nextest",
                "--test",
                "it",
                "--",
                template_kind.test_file().with_suffix("").name,
            ]
            logging.debug(f"Running {' '.join(command)}")
            exit_code = subprocess.call(
                command,
                cwd=PROJECT_ROOT,
                stderr=subprocess.STDOUT,
                stdout=sys.stderr if debug else subprocess.DEVNULL,
            )
            if exit_code != 0:
                logging.warning(
                    f"Snapshot update failed with exit code {exit_code} (use -v to show details)"
                )
        else:
            logging.info("Skipping snapshot update")

    logging.info("Done!")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Generates and updates snapshot test cases from packse scenarios.",
    )
    parser.add_argument(
        "--templates",
        type=TemplateKind,
        choices=list(TemplateKind),
        default=list(TemplateKind),
        nargs="*",
        help="The templates to render. By default, all templates are rendered",
    )
    parser.add_argument(
        "-v",
        "--verbose",
        action="store_true",
        help="Enable debug logging",
    )
    parser.add_argument(
        "-q",
        "--quiet",
        action="store_true",
        help="Disable logging",
    )
    parser.add_argument(
        "--no-snapshot-update",
        action="store_true",
        help="Disable automatic snapshot updates",
    )

    args = parser.parse_args()
    if args.quiet:
        log_level = logging.CRITICAL
    elif args.verbose:
        log_level = logging.DEBUG
    else:
        log_level = logging.INFO

    logging.basicConfig(level=log_level, format="%(message)s")

    main(args.templates, snapshot_update=not args.no_snapshot_update)
