# /// script
# requires-python = ">=3.11"
# dependencies = []
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Check the publication age of registry versions introduced by Cargo lockfile changes."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
import tomllib
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.parse import quote
from urllib.request import Request, urlopen

CRATES_IO_SOURCES = {
    "registry+https://github.com/rust-lang/crates.io-index",
    "registry+https://index.crates.io/",
}


@dataclass(frozen=True, order=True)
class Package:
    name: str
    version: str
    source: str


@dataclass(frozen=True)
class Exclusion:
    name: str
    version: str | None
    reason: str

    def matches(self, package: Package) -> bool:
        return self.name == package.name and (
            self.version is None or self.version == package.version
        )


@dataclass(frozen=True)
class Policy:
    cooldown: timedelta
    exclusions: list[Exclusion]


def load_policy(path: Path) -> Policy:
    with path.open("rb") as file:
        policy = tomllib.load(file)
    if policy.keys() != {"cooldown-days", "exclusions"}:
        raise ValueError("Policy must contain only `cooldown-days` and `exclusions`")
    days = policy["cooldown-days"]
    if type(days) is not int or days <= 0:
        raise ValueError("`cooldown-days` must be a positive integer")
    if not isinstance(policy["exclusions"], list):
        raise TypeError("`exclusions` must be a list")
    exclusions = []
    for entry in policy["exclusions"]:
        if not isinstance(entry, dict) or not (
            {"name", "reason"} <= entry.keys() <= {"name", "version", "reason"}
        ):
            raise ValueError(
                "Exclusions require `name`, `reason`, and optional `version`"
            )
        if any(
            not isinstance(value, str) or not value.strip() for value in entry.values()
        ):
            raise ValueError("Exclusion fields must be nonempty strings")
        exclusions.append(
            Exclusion(entry["name"], entry.get("version"), entry["reason"])
        )
    return Policy(timedelta(days=days), exclusions)


def git(*arguments: str) -> bytes:
    return subprocess.check_output(["git", *arguments])


def packages(lockfile: bytes) -> set[Package]:
    return {
        Package(package["name"], package["version"], package["source"])
        for package in tomllib.loads(lockfile.decode()).get("package", [])
        if package.get("source", "").startswith("registry+")
    }


def added_packages(base: str, head: str) -> dict[Package, set[str]]:
    changes = git(
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--no-renames",
        "--name-status",
        "-z",
        base,
        head,
        "--",
        ":(glob)**/Cargo.lock",
    ).split(b"\0")[:-1]
    added: dict[Package, set[str]] = {}
    for index in range(0, len(changes), 2):
        status = changes[index].decode("ascii")
        lockfile = os.fsdecode(changes[index + 1])
        if status == "D":
            manifest = str(Path(lockfile).with_name("Cargo.toml"))
            if git("ls-tree", "--name-only", head, "--", manifest):
                raise ValueError(
                    f"Cargo workspace is missing its lockfile: {lockfile!r}"
                )
            continue
        before = set() if status == "A" else packages(git("show", f"{base}:{lockfile}"))
        after = packages(git("show", f"{head}:{lockfile}"))
        for package in after - before:
            added.setdefault(package, set()).add(lockfile)
    return added


class CratesIo:
    def __init__(self) -> None:
        self.last_request: float | None = None

    def published_at(self, package: Package) -> datetime:
        if package.source not in CRATES_IO_SOURCES:
            raise ValueError(f"Unsupported registry: {package.source!r}")
        request = Request(
            f"https://crates.io/api/v1/crates/{quote(package.name, safe='')}/"
            f"{quote(package.version, safe='')}",
            headers={
                "User-Agent": "uv-dependency-cooldowns (https://github.com/astral-sh/uv)",
                "Accept": "application/json",
            },
        )
        for attempt in range(3):
            # https://crates.io/data-access limits API clients to one request per second.
            if self.last_request is not None:
                time.sleep(max(0, 1 - (time.monotonic() - self.last_request)))
            self.last_request = time.monotonic()
            try:
                with urlopen(request, timeout=30) as response:
                    version = json.load(response)["version"]
                break
            except HTTPError as error:
                if attempt == 2 or error.code not in {429, 500, 502, 503, 504}:
                    raise
            except (URLError, TimeoutError):
                if attempt == 2:
                    raise
            time.sleep(2**attempt)
        else:
            raise RuntimeError("Failed to retrieve publication metadata")

        if version["crate"] != package.name or version["num"] != package.version:
            raise ValueError("Registry metadata does not match the locked package")
        published = datetime.fromisoformat(version["created_at"])
        if published.tzinfo is None:
            raise ValueError("Publication timestamp has no timezone")
        return published


def error(message: str) -> None:
    escaped = message.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
    print(f"::error::{escaped}")


def check_packages(
    added: dict[Package, set[str]], policy: Policy, now: datetime
) -> int:
    registry = CratesIo()
    failed = False
    for package, lockfiles in sorted(added.items()):
        location = ", ".join(repr(path) for path in sorted(lockfiles))
        exclusion = next(
            (entry for entry in policy.exclusions if entry.matches(package)), None
        )
        if exclusion is not None:
            print(f"Excluded {package.name} {package.version}: {exclusion.reason!r}")
            continue
        try:
            published = registry.published_at(package)
        except (OSError, ValueError, KeyError, TypeError) as exception:
            error(
                f"Cannot verify {package.name} {package.version} in {location}: {exception}"
            )
            failed = True
            continue
        eligible = published + policy.cooldown
        if now < eligible:
            error(
                f"{package.name} {package.version} in {location} was published at "
                f"{published.isoformat()}; its {policy.cooldown.days}-day cooldown ends "
                f"at {eligible.isoformat()}. Re-run CI after that time or add a reviewed "
                "exclusion to .github/cargo-cooldowns.toml."
            )
            failed = True
        else:
            print(f"Passed {package.name} {package.version} ({location})")
    if not added:
        print("No new registry package versions in Cargo lockfiles.")
    return int(failed)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("base", help="Base Git revision to compare against")
    parser.add_argument("head", nargs="?", default="HEAD")
    parser.add_argument(
        "--policy", type=Path, default=Path(".github/cargo-cooldowns.toml")
    )
    args = parser.parse_args()
    try:
        policy = load_policy(args.policy)
        added = added_packages(args.base, args.head)
        return check_packages(added, policy, datetime.now(UTC))
    except (
        OSError,
        ValueError,
        KeyError,
        TypeError,
        subprocess.CalledProcessError,
    ) as exception:
        error(str(exception))
        return 1


if __name__ == "__main__":
    sys.exit(main())
