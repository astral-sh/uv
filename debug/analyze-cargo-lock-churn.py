#!/usr/bin/env python3
"""One-off analysis of Cargo.lock dependency-selection churn in pull requests."""

# /// script
# requires-python = ">=3.12"
# ///

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tomllib
import urllib.request
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any, BinaryIO

ROOT = Path(__file__).parent.parent
CRATES_IO_INDEX = "https://index.crates.io"
PR_SUFFIX = re.compile(r" \(#(?P<number>\d+)\)$")
DEPENDENCY_REFERENCE = re.compile(
    r"^(?P<name>\S+)(?: (?P<version>\S+))?(?: \((?P<source>.+)\))?$"
)
COMPARATOR = re.compile(r"^(?P<operator>>=|<=|>|<|=|~|\^)?\s*(?P<version>[^\s]+)$")


@dataclass(frozen=True)
class Version:
    major: int
    minor: int
    patch: int
    prerelease: tuple[int | str, ...] = ()

    @classmethod
    def parse(cls, value: str) -> Version:
        value = value.split("+", 1)[0]
        release, separator, prerelease = value.partition("-")
        components = release.split(".")
        if len(components) != 3:
            raise ValueError(f"Expected a complete semantic version, got {value!r}")
        prerelease_components: list[int | str] = []
        if separator:
            for component in prerelease.split("."):
                prerelease_components.append(
                    int(component) if component.isdigit() else component
                )
        return cls(
            major=int(components[0]),
            minor=int(components[1]),
            patch=int(components[2]),
            prerelease=tuple(prerelease_components),
        )


@dataclass(frozen=True)
class Comparator:
    operator: str
    major: int
    minor: int | None
    patch: int | None
    prerelease: tuple[int | str, ...]


@dataclass(frozen=True)
class PackageId:
    name: str
    version: str
    source: str | None


@dataclass(frozen=True)
class Package:
    package_id: PackageId
    checksum: str | None
    dependencies: tuple[str, ...]


@dataclass(frozen=True)
class Lockfile:
    packages: dict[PackageId, Package]
    packages_by_name: dict[str, tuple[PackageId, ...]]


@dataclass(frozen=True)
class Toggle:
    depender: PackageId
    dependency_before: PackageId
    dependency_after: PackageId


@dataclass(frozen=True)
class VerifiedToggle:
    toggle: Toggle
    requirements: tuple[str, ...]


@dataclass(frozen=True)
class VersionChange:
    name: str
    before: tuple[str, ...]
    after: tuple[str, ...]


@dataclass(frozen=True)
class Commit:
    commit: str
    parent: str
    date: str
    subject: str
    pull_request: int


@dataclass(frozen=True)
class ChurnCase:
    commit: Commit
    version_changes: tuple[VersionChange, ...]
    toggles: tuple[VerifiedToggle, ...]


class GitObjectReader:
    def __init__(self, repository: Path) -> None:
        self.process = subprocess.Popen(
            ["git", "cat-file", "--batch"],
            cwd=repository,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
        )

    def __enter__(self) -> GitObjectReader:
        return self

    def __exit__(self, *_: object) -> None:
        if self.process.stdin is not None:
            self.process.stdin.close()
        self.process.wait()

    def read_blob(self, object_spec: str) -> bytes | None:
        stdin = require_stream(self.process.stdin)
        stdout = require_stream(self.process.stdout)
        stdin.write(f"{object_spec}\n".encode())
        stdin.flush()

        header = stdout.readline().decode().rstrip("\n")
        if header.endswith(" missing"):
            return None
        _, object_type, size_text = header.split()
        if object_type != "blob":
            raise RuntimeError(f"Expected a blob for {object_spec}, got {object_type}")
        content = stdout.read(int(size_text))
        if stdout.read(1) != b"\n":
            raise RuntimeError(f"Malformed git cat-file output for {object_spec}")
        return content


class CratesIoIndex:
    def __init__(self) -> None:
        self.records: dict[str, dict[str, dict[str, Any]]] = {}
        self.local_indexes = find_local_crates_io_indexes()

    def record(self, name: str, version: str) -> dict[str, Any] | None:
        normalized_name = name.lower()
        if normalized_name not in self.records:
            self.records[normalized_name] = self._load_records(normalized_name)
        return self.records[normalized_name].get(version)

    def _load_records(self, name: str) -> dict[str, dict[str, Any]]:
        relative_path = index_relative_path(name)
        for index in self.local_indexes:
            cache_path = index / ".cache" / relative_path
            if not cache_path.is_file():
                continue
            records = parse_index_cache(cache_path.read_bytes())
            if records:
                return records

        request = urllib.request.Request(
            f"{CRATES_IO_INDEX}/{relative_path.as_posix()}",
            headers={"User-Agent": "uv-cargo-lock-churn-analysis"},
        )
        with urllib.request.urlopen(request, timeout=30) as response:
            return parse_index_json_lines(response.read())


def require_stream(stream: BinaryIO | None) -> BinaryIO:
    if stream is None:
        raise RuntimeError("Subprocess stream was not configured")
    return stream


def parse_pr_commit(line: str) -> Commit | None:
    commit, parents, date, subject = line.split("\x1f", 3)
    match = PR_SUFFIX.search(subject)
    parent = parents.split(maxsplit=1)[0] if parents else ""
    if match is None or not parent:
        return None
    return Commit(
        commit=commit,
        parent=parent,
        date=date,
        subject=subject[: match.start()],
        pull_request=int(match.group("number")),
    )


def lockfile_commits(repository: Path) -> list[Commit]:
    result = subprocess.run(
        [
            "git",
            "log",
            "--first-parent",
            "--format=%H%x1f%P%x1f%cs%x1f%s",
            "--",
            "Cargo.lock",
        ],
        cwd=repository,
        check=True,
        capture_output=True,
        text=True,
    )
    commits: list[Commit] = []
    for line in result.stdout.splitlines():
        if commit := parse_pr_commit(line):
            commits.append(commit)
    return commits


def parse_lockfile(content: bytes) -> Lockfile:
    data = tomllib.loads(content.decode())
    packages: dict[PackageId, Package] = {}
    packages_by_name: dict[str, list[PackageId]] = defaultdict(list)
    for entry in data.get("package", []):
        package_id = PackageId(
            name=entry["name"],
            version=entry["version"],
            source=entry.get("source"),
        )
        packages[package_id] = Package(
            package_id=package_id,
            checksum=entry.get("checksum"),
            dependencies=tuple(entry.get("dependencies", ())),
        )
        packages_by_name[package_id.name].append(package_id)
    return Lockfile(
        packages=packages,
        packages_by_name={
            name: tuple(package_ids) for name, package_ids in packages_by_name.items()
        },
    )


def resolve_dependency(reference: str, lockfile: Lockfile) -> PackageId | None:
    match = DEPENDENCY_REFERENCE.match(reference)
    if match is None:
        return None
    candidates = lockfile.packages_by_name.get(match.group("name"), ())
    version = match.group("version")
    source = match.group("source")
    if version is not None:
        candidates = tuple(
            package_id for package_id in candidates if package_id.version == version
        )
    if source is not None:
        candidates = tuple(
            package_id for package_id in candidates if package_id.source == source
        )
    return candidates[0] if len(candidates) == 1 else None


def dependency_targets(
    package: Package, lockfile: Lockfile
) -> dict[str, frozenset[PackageId]]:
    targets: dict[str, set[PackageId]] = defaultdict(set)
    for reference in package.dependencies:
        if package_id := resolve_dependency(reference, lockfile):
            targets[package_id.name].add(package_id)
    return {name: frozenset(package_ids) for name, package_ids in targets.items()}


def find_toggles(before: Lockfile, after: Lockfile) -> tuple[Toggle, ...]:
    toggles: list[Toggle] = []
    for package_id in before.packages.keys() & after.packages.keys():
        if package_id.source is None:
            continue
        package_before = before.packages[package_id]
        package_after = after.packages[package_id]
        if package_before.checksum != package_after.checksum:
            continue
        dependencies_before = dependency_targets(package_before, before)
        dependencies_after = dependency_targets(package_after, after)
        for dependency_name in dependencies_before.keys() | dependencies_after.keys():
            removed = dependencies_before.get(dependency_name, frozenset()) - (
                dependencies_after.get(dependency_name, frozenset())
            )
            added = dependencies_after.get(dependency_name, frozenset()) - (
                dependencies_before.get(dependency_name, frozenset())
            )
            if len(removed) != 1 or len(added) != 1:
                continue
            dependency_before = next(iter(removed))
            dependency_after = next(iter(added))
            if dependency_before.version == dependency_after.version:
                continue
            if not all(
                dependency in lockfile.packages
                for dependency in (dependency_before, dependency_after)
                for lockfile in (before, after)
            ):
                continue
            toggles.append(
                Toggle(
                    depender=package_id,
                    dependency_before=dependency_before,
                    dependency_after=dependency_after,
                )
            )
    return tuple(sorted(toggles, key=toggle_sort_key))


def find_version_changes(
    before: Lockfile, after: Lockfile
) -> tuple[VersionChange, ...]:
    changes: list[VersionChange] = []
    names = before.packages_by_name.keys() | after.packages_by_name.keys()
    for name in names:
        versions_before = {
            package_id.version
            for package_id in before.packages_by_name.get(name, ())
            if is_external_source(package_id.source)
        }
        versions_after = {
            package_id.version
            for package_id in after.packages_by_name.get(name, ())
            if is_external_source(package_id.source)
        }
        removed = versions_before - versions_after
        added = versions_after - versions_before
        if removed and added:
            changes.append(
                VersionChange(
                    name=name,
                    before=tuple(sorted(removed)),
                    after=tuple(sorted(added)),
                )
            )
    return tuple(sorted(changes, key=lambda change: change.name))


def verify_toggle(toggle: Toggle, index: CratesIoIndex) -> VerifiedToggle | None:
    if not is_crates_io_source(toggle.depender.source):
        return None
    record = index.record(toggle.depender.name, toggle.depender.version)
    if record is None:
        return None
    requirements = matching_requirements(
        record,
        toggle.dependency_before.name,
        toggle.dependency_before.version,
        toggle.dependency_after.version,
    )
    if not requirements:
        return None
    return VerifiedToggle(toggle=toggle, requirements=requirements)


def matching_requirements(
    record: dict[str, Any], dependency_name: str, before: str, after: str
) -> tuple[str, ...]:
    requirements: set[str] = set()
    for dependency in record.get("deps", []):
        actual_name = dependency.get("package") or dependency["name"]
        if actual_name != dependency_name or dependency.get("kind") == "dev":
            continue
        requirement = dependency["req"]
        if requirement_matches(requirement, before) and requirement_matches(
            requirement, after
        ):
            requirements.add(requirement)
    return tuple(sorted(requirements))


def requirement_matches(requirement: str, version: str) -> bool:
    parsed_version = Version.parse(version)
    comparators = parse_requirement(requirement)
    if not all(
        comparator_matches(comparator, parsed_version) for comparator in comparators
    ):
        return False
    if not parsed_version.prerelease:
        return True
    return any(
        comparator.major == parsed_version.major
        and comparator.minor == parsed_version.minor
        and comparator.patch == parsed_version.patch
        and comparator.prerelease
        for comparator in comparators
    )


def parse_requirement(requirement: str) -> tuple[Comparator, ...]:
    if requirement.strip() in {"*", "x", "X"}:
        return ()
    return tuple(
        parse_comparator(component.strip()) for component in requirement.split(",")
    )


def parse_comparator(value: str) -> Comparator:
    match = COMPARATOR.match(value)
    if match is None:
        raise ValueError(f"Unsupported Cargo version comparator: {value!r}")
    operator = match.group("operator") or "^"
    version_text = match.group("version")
    release, separator, prerelease = version_text.partition("-")
    components = release.split(".")
    if len(components) > 3:
        raise ValueError(f"Unsupported Cargo version comparator: {value!r}")

    parsed_components: list[int | None] = []
    wildcard = False
    for component in components:
        if component in {"*", "x", "X"}:
            wildcard = True
            parsed_components.append(None)
        else:
            parsed_components.append(int(component))
    while len(parsed_components) < 3:
        parsed_components.append(None)
    if parsed_components[0] is None:
        raise ValueError(f"Wildcard major must be the whole requirement: {value!r}")
    if wildcard:
        operator = "*"

    prerelease_components: list[int | str] = []
    if separator:
        for component in prerelease.split("."):
            prerelease_components.append(
                int(component) if component.isdigit() else component
            )
    return Comparator(
        operator=operator,
        major=parsed_components[0],
        minor=parsed_components[1],
        patch=parsed_components[2],
        prerelease=tuple(prerelease_components),
    )


def comparator_matches(comparator: Comparator, version: Version) -> bool:
    if comparator.operator in {"=", "*"}:
        return matches_exact(comparator, version)
    if comparator.operator == ">":
        return matches_greater(comparator, version)
    if comparator.operator == ">=":
        return matches_exact(comparator, version) or matches_greater(
            comparator, version
        )
    if comparator.operator == "<":
        return matches_less(comparator, version)
    if comparator.operator == "<=":
        return matches_exact(comparator, version) or matches_less(comparator, version)
    if comparator.operator == "~":
        return matches_tilde(comparator, version)
    if comparator.operator == "^":
        return matches_caret(comparator, version)
    raise ValueError(f"Unsupported Cargo version operator: {comparator.operator}")


def matches_exact(comparator: Comparator, version: Version) -> bool:
    return (
        version.major == comparator.major
        and (comparator.minor is None or version.minor == comparator.minor)
        and (comparator.patch is None or version.patch == comparator.patch)
        and version.prerelease == comparator.prerelease
    )


def matches_greater(comparator: Comparator, version: Version) -> bool:
    if version.major != comparator.major:
        return version.major > comparator.major
    if comparator.minor is None:
        return False
    if version.minor != comparator.minor:
        return version.minor > comparator.minor
    if comparator.patch is None:
        return False
    if version.patch != comparator.patch:
        return version.patch > comparator.patch
    return compare_prerelease(version.prerelease, comparator.prerelease) > 0


def matches_less(comparator: Comparator, version: Version) -> bool:
    if version.major != comparator.major:
        return version.major < comparator.major
    if comparator.minor is None:
        return False
    if version.minor != comparator.minor:
        return version.minor < comparator.minor
    if comparator.patch is None:
        return False
    if version.patch != comparator.patch:
        return version.patch < comparator.patch
    return compare_prerelease(version.prerelease, comparator.prerelease) < 0


def matches_tilde(comparator: Comparator, version: Version) -> bool:
    if version.major != comparator.major:
        return False
    if comparator.minor is not None and version.minor != comparator.minor:
        return False
    if comparator.patch is not None and version.patch != comparator.patch:
        return version.patch > comparator.patch
    return compare_prerelease(version.prerelease, comparator.prerelease) >= 0


def matches_caret(comparator: Comparator, version: Version) -> bool:
    if version.major != comparator.major:
        return False
    if comparator.minor is None:
        return True
    if comparator.patch is None:
        return (
            version.minor >= comparator.minor
            if comparator.major > 0
            else version.minor == comparator.minor
        )
    if comparator.major > 0:
        if version.minor != comparator.minor:
            return version.minor > comparator.minor
        if version.patch != comparator.patch:
            return version.patch > comparator.patch
    elif comparator.minor > 0:
        if version.minor != comparator.minor:
            return False
        if version.patch != comparator.patch:
            return version.patch > comparator.patch
    elif version.minor != comparator.minor or version.patch != comparator.patch:
        return False
    return compare_prerelease(version.prerelease, comparator.prerelease) >= 0


def compare_prerelease(
    left: tuple[int | str, ...], right: tuple[int | str, ...]
) -> int:
    if not left or not right:
        return (not left) - (not right)
    for left_component, right_component in zip(left, right, strict=False):
        if left_component == right_component:
            continue
        if isinstance(left_component, int) and isinstance(right_component, str):
            return -1
        if isinstance(left_component, str) and isinstance(right_component, int):
            return 1
        return (left_component > right_component) - (left_component < right_component)
    return (len(left) > len(right)) - (len(left) < len(right))


def analyze_history(repository: Path, limit: int) -> tuple[list[ChurnCase], int]:
    cases: list[ChurnCase] = []
    scanned = 0
    index = CratesIoIndex()
    with GitObjectReader(repository) as objects:
        for commit in lockfile_commits(repository):
            scanned += 1
            before_content = objects.read_blob(f"{commit.parent}:Cargo.lock")
            after_content = objects.read_blob(f"{commit.commit}:Cargo.lock")
            if before_content is None or after_content is None:
                continue
            before = parse_lockfile(before_content)
            after = parse_lockfile(after_content)
            version_changes = find_version_changes(before, after)
            if not version_changes:
                continue
            verified_toggles = tuple(
                verified
                for toggle in find_toggles(before, after)
                if (verified := verify_toggle(toggle, index)) is not None
            )
            if not verified_toggles:
                continue
            cases.append(
                ChurnCase(
                    commit=commit,
                    version_changes=version_changes,
                    toggles=verified_toggles,
                )
            )
            print(
                f"[{len(cases)}/{limit}] #{commit.pull_request} "
                f"({len(verified_toggles)} toggles)",
                file=sys.stderr,
            )
            if len(cases) == limit:
                break
    return cases, scanned


def render_report(cases: list[ChurnCase], scanned: int, head: str) -> str:
    toggle_count = sum(len(case.toggles) for case in cases)
    lines = [
        "# Cargo.lock dependency-selection churn",
        "",
        "This report was generated by the one-off `debug/analyze-cargo-lock-churn.py`. It lists "
        f"the newest {len(cases)} pull requests in the local first-parent history "
        "where an ordinary locked package update also changed which already-locked "
        "version an unrelated, unchanged crate selected.",
        "",
        "## Detection criteria",
        "",
        "A row is included only when all of the following are true:",
        "",
        "1. The pull request changes at least one external package version in `Cargo.lock`.",
        "2. The exact depender package identity (name, version, source, and checksum when present) is unchanged.",
        "3. Its dependency edge switches between two versions of the same package.",
        "4. Both dependency versions exist in both the parent and resulting lockfiles, so the switch is selection churn rather than an add/remove.",
        "5. The exact crates.io index record for the depender contains a non-development requirement that matches both selected versions under Cargo semver rules.",
        "",
        f"The scan examined {scanned} lockfile-changing PR commits from `{head[:12]}` "
        f"and stopped after {len(cases)} matching PRs. Those PRs contain "
        f"{toggle_count} verified dependency-edge toggles.",
        "",
    ]
    for position, case in enumerate(cases, start=1):
        commit = case.commit
        lines.extend(
            [
                f"## {position}. [#{commit.pull_request}](https://github.com/astral-sh/uv/pull/{commit.pull_request}) — {commit.subject}",
                "",
                f"- Date: {commit.date}",
                f"- Commit: [`{commit.commit[:12]}`](https://github.com/astral-sh/uv/commit/{commit.commit})",
                f"- Locked version changes: {render_version_changes(case.version_changes)}",
                "",
                "| Unchanged depender | Declared requirement | Dependency | Before | After |",
                "| --- | --- | --- | --- | --- |",
            ]
        )
        for verified in case.toggles:
            toggle = verified.toggle
            lines.append(
                "| "
                f"`{toggle.depender.name} {toggle.depender.version}` | "
                f"{', '.join(f'`{requirement}`' for requirement in verified.requirements)} | "
                f"`{toggle.dependency_before.name}` | "
                f"`{toggle.dependency_before.version}` | "
                f"`{toggle.dependency_after.version}` |"
            )
        lines.append("")
    return "\n".join(lines)


def render_version_changes(changes: tuple[VersionChange, ...]) -> str:
    rendered = []
    for change in changes:
        before = ", ".join(change.before)
        after = ", ".join(change.after)
        rendered.append(f"`{change.name} {before} -> {after}`")
    return "; ".join(rendered)


def is_external_source(source: str | None) -> bool:
    return source is not None


def is_crates_io_source(source: str | None) -> bool:
    return source is not None and (
        "github.com/rust-lang/crates.io-index" in source or "index.crates.io" in source
    )


def index_relative_path(name: str) -> Path:
    name = name.lower()
    if len(name) == 1:
        return Path("1") / name
    if len(name) == 2:
        return Path("2") / name
    if len(name) == 3:
        return Path("3") / name[0] / name
    return Path(name[:2]) / name[2:4] / name


def find_local_crates_io_indexes() -> tuple[Path, ...]:
    indexes = []
    registry_indexes = Path.home() / ".cargo" / "registry" / "index"
    if not registry_indexes.is_dir():
        return ()
    for index in registry_indexes.iterdir():
        config_path = index / "config.json"
        if not config_path.is_file():
            continue
        try:
            config = json.loads(config_path.read_text())
        except (json.JSONDecodeError, OSError):
            continue
        if "crates.io" in config.get("dl", ""):
            indexes.append(index)
    return tuple(indexes)


def parse_index_cache(content: bytes) -> dict[str, dict[str, Any]]:
    records = {}
    for component in content.split(b"\0"):
        if not component.startswith(b"{"):
            continue
        record = json.loads(component)
        records[record["vers"]] = record
    return records


def parse_index_json_lines(content: bytes) -> dict[str, dict[str, Any]]:
    return {
        record["vers"]: record
        for line in content.splitlines()
        if line
        for record in (json.loads(line),)
    }


def toggle_sort_key(toggle: Toggle) -> tuple[str, str, str, str]:
    return (
        toggle.dependency_before.name,
        toggle.dependency_before.version,
        toggle.dependency_after.version,
        f"{toggle.depender.name} {toggle.depender.version}",
    )


def git_head(repository: Path) -> str:
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=repository,
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout.strip()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--limit", type=int, default=50)
    parser.add_argument(
        "--output", type=Path, default=ROOT / "debug" / "cargo-lock-churn.md"
    )
    parser.add_argument("--repository", type=Path, default=ROOT)
    args = parser.parse_args()
    if args.limit < 1:
        parser.error("--limit must be at least 1")

    repository = args.repository.resolve()
    cases, scanned = analyze_history(repository, args.limit)
    report = render_report(cases, scanned, git_head(repository))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(report)
    print(f"Wrote {args.output} with {len(cases)} PR cases", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
