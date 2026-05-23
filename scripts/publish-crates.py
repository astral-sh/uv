# Publish workspace crates to crates.io idempotently.
#
# `cargo publish --workspace` fails if any selected crate version already exists on crates.io. That
# makes release re-runs fail after a previous partial publish. This script queries crates.io first,
# excludes package versions that already exist, then delegates to `cargo publish --workspace` for the
# remaining packages so Cargo still handles package ordering.
#
# Usage:
#
#   CARGO_REGISTRY_TOKEN=<tok> python3 scripts/publish-crates.py --no-verify

from __future__ import annotations

import argparse
import dataclasses
import json
import pathlib
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

CRATES_IO_API = "https://crates.io/api/v1"
USER_AGENT = "uv-crates-io-publish (github.com/astral-sh/uv)"

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent

# Short delay between crates.io API requests to avoid burst rate limits.
CRATES_IO_REQUEST_DELAY_SECS = 0.2


@dataclasses.dataclass(frozen=True)
class Crate:
    name: str
    version: str

    def pretty(self) -> str:
        return f"{self.name}@{self.version}"


def get_publishable_crates(cargo: list[str]) -> list[Crate]:
    """Return publishable workspace crates."""
    result = subprocess.run(
        [*cargo, "metadata", "--format-version", "1", "--no-deps"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    metadata = json.loads(result.stdout)

    workspace_member_ids = set(metadata["workspace_members"])
    crates = []
    for package in metadata["packages"]:
        if package["id"] not in workspace_member_ids:
            continue
        # `publish = false` is represented as an empty list in cargo metadata.
        if package.get("publish") == []:
            continue
        crates.append(Crate(name=package["name"], version=package["version"]))

    return crates


def crate_version_url(crate: Crate, api_url: str) -> str:
    crate_name = urllib.parse.quote(crate.name, safe="")
    crate_version = urllib.parse.quote(crate.version, safe="")
    return f"{api_url}/crates/{crate_name}/{crate_version}"


def crate_version_exists(crate: Crate, api_url: str) -> bool:
    """Return True if the crate version already exists on crates.io."""
    request = urllib.request.Request(
        crate_version_url(crate, api_url),
        headers={"User-Agent": USER_AGENT},
    )
    try:
        with urllib.request.urlopen(request) as response:
            if response.status == 200:
                return True
            raise RuntimeError(
                f"unexpected status {response.status} for {crate.pretty()}"
            )
    except urllib.error.HTTPError as exc:
        if exc.code == 404:
            return False
        raise RuntimeError(
            f"failed to query {crate.pretty()} from crates.io: HTTP {exc.code}"
        ) from exc
    except urllib.error.URLError as exc:
        raise RuntimeError(
            f"failed to query {crate.pretty()} from crates.io: {exc}"
        ) from exc


def existing_crate_versions(crates: list[Crate], api_url: str) -> set[str]:
    """Return the names of workspace crates whose current version exists on crates.io."""
    existing = set()
    for index, crate in enumerate(crates):
        if index > 0:
            time.sleep(CRATES_IO_REQUEST_DELAY_SECS)
        if crate_version_exists(crate, api_url):
            print(f"Skipping {crate.pretty()}: already exists on crates.io")
            existing.add(crate.name)
    return existing


def build_cargo_publish_command(
    cargo: list[str], existing: set[str], cargo_publish_args: list[str]
) -> list[str]:
    """Build the `cargo publish` command for all not-yet-published workspace crates."""
    command = [*cargo, "publish", "--workspace"]
    for crate_name in sorted(existing):
        command.extend(["--exclude", crate_name])
    command.extend(cargo_publish_args)
    return command


def publish_workspace(
    cargo: list[str], crates: list[Crate], api_url: str, cargo_publish_args: list[str]
) -> int:
    existing = existing_crate_versions(crates, api_url)
    missing = [crate for crate in crates if crate.name not in existing]

    if not missing:
        print("All publishable workspace crate versions already exist on crates.io")
        return 0

    print("Publishing workspace crate versions that do not exist on crates.io:")
    for crate in missing:
        print(f"  {crate.pretty()}")

    command = build_cargo_publish_command(cargo, existing, cargo_publish_args)
    return subprocess.run(command, cwd=REPO_ROOT).returncode


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Publish workspace crates to crates.io idempotently."
    )
    parser.add_argument(
        "--api-url",
        default=CRATES_IO_API,
        help=argparse.SUPPRESS,
    )
    parser.add_argument(
        "--cargo",
        default=["cargo"],
        nargs="+",
        help=argparse.SUPPRESS,
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Pass --dry-run through to cargo publish.",
    )
    parser.add_argument(
        "--no-verify",
        action="store_true",
        help="Pass --no-verify through to cargo publish.",
    )
    parser.add_argument(
        "cargo_publish_args",
        nargs=argparse.REMAINDER,
        help=argparse.SUPPRESS,
    )
    args = parser.parse_args(argv)
    if args.cargo_publish_args[:1] == ["--"]:
        args.cargo_publish_args = args.cargo_publish_args[1:]
    return args


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)

    cargo_publish_args = []
    if args.dry_run:
        cargo_publish_args.append("--dry-run")
    if args.no_verify:
        cargo_publish_args.append("--no-verify")
    cargo_publish_args.extend(args.cargo_publish_args)

    crates = get_publishable_crates(args.cargo)
    return publish_workspace(args.cargo, crates, args.api_url, cargo_publish_args)


if __name__ == "__main__":
    sys.exit(main())
