# /// script
# requires-python = ">=3.11"
# dependencies = ["urllib3>=2"]
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Enforce the publication cooldown for registry versions in all tracked Cargo.lock files."""

import argparse
import json
import os
import re
import subprocess
import sys
import tomllib
from datetime import UTC, datetime, timedelta
from functools import cache
from pathlib import Path
from urllib.request import getproxies, proxy_bypass

import urllib3
from urllib3.util import Retry

POLICY = Path(".github/cargo-cooldowns.toml")
CRATES_IO = {
    "registry+https://github.com/rust-lang/crates.io-index",
    "registry+https://index.crates.io/",
    "sparse+https://index.crates.io/",
}


def load_policy() -> tuple[timedelta, dict[tuple[str, str | None], str]]:
    with POLICY.open("rb") as file:
        policy = tomllib.load(file)
    if policy.keys() != {"cooldown-days", "exclusions"} or not isinstance(
        policy["exclusions"], list
    ):
        raise ValueError(
            "The policy must contain cooldown-days and a list of exclusions"
        )
    days = policy["cooldown-days"]
    if type(days) is not int or days <= 0:
        raise ValueError("cooldown-days must be a positive integer")
    exclusions = {}
    for entry in policy["exclusions"]:
        if not isinstance(entry, dict) or entry.keys() not in (
            {"name", "reason"},
            {"name", "version", "reason"},
        ):
            raise ValueError(
                "Exclusions require a name, a reason, and optionally a version"
            )
        if any(
            not isinstance(value, str) or not value.strip() for value in entry.values()
        ):
            raise ValueError("Exclusion fields must be nonempty strings")
        exclusions[entry["name"], entry.get("version")] = entry["reason"]
    return timedelta(days=days), exclusions


def locked_packages() -> dict[tuple[str, str, str], set[Path]]:
    paths = subprocess.check_output(
        ["git", "ls-files", "-z", "--", ":(glob)**/Cargo.lock"]
    )
    packages: dict[tuple[str, str, str], set[Path]] = {}
    for raw in paths.split(b"\0"):
        if not raw:
            continue
        path = Path(os.fsdecode(raw))
        with path.open("rb") as file:
            for package in tomllib.load(file)["package"]:
                source = package.get("source", "")
                if source.startswith(("registry+", "sparse+")):
                    key = (package["name"], package["version"], source)
                    packages.setdefault(key, set()).add(path)
    return packages


@cache
def release_times(http: urllib3.PoolManager, name: str) -> dict[str, str]:
    if not re.fullmatch(r"[a-zA-Z0-9_-]+", name):
        raise ValueError(f"Invalid crates.io name: {name!r}")
    lower = name.lower()
    if len(lower) <= 2:
        prefix = str(len(lower))
    elif len(lower) == 3:
        prefix = f"3/{lower[0]}"
    else:
        prefix = f"{lower[:2]}/{lower[2:4]}"
    url = f"https://index.crates.io/{prefix}/{lower}"
    response = http.request(
        "GET",
        url,
        timeout=30,
        headers={
            "User-Agent": "uv-dependency-cooldowns (https://github.com/astral-sh/uv)",
            "Accept-Encoding": "gzip",
        },
        retries=Retry(
            total=2,
            backoff_factor=1,
            status_forcelist=(429, 500, 502, 503, 504),
            respect_retry_after_header=True,
        ),
    )
    if response.status != 200:
        raise ValueError(f"{url} returned HTTP {response.status}")
    entries = map(json.loads, response.data.splitlines())
    return {
        entry["vers"]: entry["pubtime"]
        for entry in entries
        if entry["name"] == name and "pubtime" in entry
    }


def check(cache_path: Path | None) -> int:
    cooldown, exclusions = load_policy()
    packages = locked_packages()
    cached: dict[str, str] = {}
    if cache_path is not None and cache_path.exists():
        try:
            data = json.loads(cache_path.read_bytes())
            if isinstance(data, dict):
                cached = data
        except (OSError, ValueError) as error:
            print(f"Cannot read cached publication times: {error}", file=sys.stderr)
    # Keep only timestamps for currently locked versions so the KV value stays small.
    timestamps: dict[str, str] = {}
    proxy = getproxies().get("https")
    http = (
        urllib3.ProxyManager(proxy)
        if proxy and not proxy_bypass("index.crates.io")
        else urllib3.PoolManager()
    )

    now = datetime.now(UTC)
    failed = False
    hits = 0
    for (name, version, source), paths in sorted(packages.items()):
        key = f"{name}@{version}"
        reason = exclusions.get((name, version)) or exclusions.get((name, None))
        if reason:
            print(f"Excluded {key!r}: {reason!r}")
            continue
        if source not in CRATES_IO:
            raise ValueError(f"Cannot verify {key}: unsupported registry {source!r}")
        timestamp = cached.get(key)
        if timestamp is None:
            timestamp = release_times(http, name).get(version)
            if timestamp is None:
                raise ValueError(
                    f"Cannot find the publication time for {key} in the crates.io index"
                )
        else:
            hits += 1
        timestamps[key] = timestamp
        published = datetime.fromisoformat(timestamp)
        eligible = published + cooldown
        if now < eligible:
            location = ", ".join(repr(str(path)) for path in sorted(paths))
            print(
                f"{key!r} in {location} was published at {published.isoformat()}; its {cooldown.days}-day cooldown ends at {eligible.isoformat()}.",
                file=sys.stderr,
            )
            failed = True

    if cache_path is not None:
        cache_path.write_text(json.dumps(timestamps, separators=(",", ":")))
    print(
        f"Checked {len(packages)} locked registry versions ({hits} cached publication times)."
    )
    return int(failed)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--cache", type=Path, help="Read and update a cache of publication times"
    )
    try:
        raise SystemExit(check(parser.parse_args().cache))
    except (
        OSError,
        ValueError,
        KeyError,
        TypeError,
        subprocess.CalledProcessError,
        urllib3.exceptions.HTTPError,
    ) as error:
        raise SystemExit(f"Cannot check Cargo dependency cooldowns: {error}") from error
