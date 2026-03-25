# Ensure workspace crates are ready for trusted publishing on crates.io.
#
# This script performs four steps for each candidate crate:
#
#   1. Publish a placeholder crate if the crate doesn't yet exist on crates.io.
#   2. Remove trusted publisher configs that don't match our desired config.
#   3. Ensure exactly one desired trusted publishing config exists.
#   4. Enable `trustpub_only` ("Require trusted publishing for all new versions").
#
# It authenticates with `CARGO_REGISTRY_TOKEN`, which must have the `publish-new` and
# `trusted-publishing` scopes.
#
# Crates tracked in `.known-crates` are assumed to be configured and are skipped unless `--force` is
# used.
#
# Usage:
#
#   CARGO_REGISTRY_TOKEN=<tok> uv run scripts/setup-crates-io-publish.py [--dry-run] [--force] [--quiet]

# /// script
# requires-python = ">=3.13"
# dependencies = ["httpx"]
# ///

from __future__ import annotations

import argparse
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import time
import tomllib

import httpx

CRATES_IO_API = "https://crates.io/api/v1"
USER_AGENT = "uv-crates-io-publish-setup (github.com/astral-sh/uv)"

REPOSITORY_OWNER = "astral-sh"
REPOSITORY_NAME = "uv"
WORKFLOW_FILENAME = "release.yml"
ENVIRONMENT = "release"

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
WORKSPACE_MANIFEST_PATH = REPO_ROOT / "Cargo.toml"
KNOWN_CRATES_PATH = REPO_ROOT / ".known-crates"
KNOWN_CRATES_HEADER = "# GENERATED-BY scripts/setup-crates-io-publish.py\n"

PLACEHOLDER_VERSION = "0.0.0"

# Delay between `cargo publish` calls to respect crates.io rate limits.
PUBLISH_DELAY_SECS = 15


def get_publishable_crates() -> list[dict[str, str]]:
    """Return publishable workspace crates as ``[{"name": …, "version": …}]``."""
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
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
        # ``publish = false`` is represented as an empty list in cargo metadata.
        if package.get("publish") == []:
            continue
        crates.append({"name": package["name"], "version": package["version"]})

    return sorted(crates, key=lambda c: c["name"])


def load_known_crates() -> set[str]:
    """Load crate names from ``.known-crates``."""
    if not KNOWN_CRATES_PATH.exists():
        return set()

    known = set()
    for line in KNOWN_CRATES_PATH.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        known.add(line)
    return known


def save_known_crates(crates: set[str]) -> None:
    """Persist the sorted set of fully-configured crate names."""
    lines = [KNOWN_CRATES_HEADER]
    for name in sorted(crates):
        lines.append(f"{name}\n")
    KNOWN_CRATES_PATH.write_text("".join(lines))


def get_crate_metadata(
    client: httpx.Client, crate_name: str
) -> dict[str, object] | None:
    """Get crate metadata from the public crates.io API."""
    response = client.get(f"{CRATES_IO_API}/crates/{crate_name}")
    if response.status_code == 404:
        return None
    response.raise_for_status()
    return response.json()


def load_workspace_package_metadata() -> dict[str, object]:
    """Load shared package metadata from the workspace manifest."""
    manifest = tomllib.loads(WORKSPACE_MANIFEST_PATH.read_text())
    workspace_package = manifest.get("workspace", {}).get("package")
    if not isinstance(workspace_package, dict):
        raise RuntimeError("workspace.package is missing from Cargo.toml")
    return workspace_package


def publish_placeholder_crate(
    crate_name: str,
    workspace_package: dict[str, object],
) -> bool:
    """Publish a generated placeholder crate to reserve a new crates.io name."""
    with tempfile.TemporaryDirectory(prefix=f"{crate_name}-placeholder-") as temp_dir:
        temp_path = pathlib.Path(temp_dir)
        src_dir = temp_path / "src"
        src_dir.mkdir(parents=True)

        authors = workspace_package.get("authors", [])
        if not isinstance(authors, list):
            raise RuntimeError("workspace.package.authors must be a list")

        edition = workspace_package.get("edition")
        rust_version = workspace_package.get("rust-version")
        homepage = workspace_package.get("homepage")
        repository = workspace_package.get("repository")
        license_expression = workspace_package.get("license")

        description = (
            "This is a placeholder release for an internal component crate of uv"
        )

        manifest = (
            "[package]\n"
            f"name = {json.dumps(crate_name)}\n"
            f"version = {json.dumps(PLACEHOLDER_VERSION)}\n"
            f"edition = {json.dumps(edition)}\n"
            f"rust-version = {json.dumps(rust_version)}\n"
            f"authors = {json.dumps(authors)}\n"
            f"license = {json.dumps(license_expression)}\n"
            f"homepage = {json.dumps(homepage)}\n"
            f"repository = {json.dumps(repository)}\n"
            f"description = {json.dumps(description)}\n"
            'readme = "README.md"\n'
        )
        (temp_path / "Cargo.toml").write_text(manifest)
        (temp_path / "README.md").write_text(
            "<!-- This file is generated. DO NOT EDIT -->\n\n"
            f"# {crate_name}\n\n"
            f"This crate is an internal component of [uv](https://crates.io/crates/uv). "
            f"This placeholder version ({PLACEHOLDER_VERSION}) only exists to reserve the "
            "crate name and enable trusted publishing for future releases.\n"
        )
        (src_dir / "lib.rs").write_text(
            "//! Placeholder crate published to reserve the name on crates.io.\n\n"
            "/// Marker type for the placeholder release.\n"
            "pub struct Placeholder;\n"
        )

        result = subprocess.run(
            [
                "cargo",
                "publish",
                "--manifest-path",
                str(temp_path / "Cargo.toml"),
                "--no-verify",
            ],
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            print(result.stderr, file=sys.stderr, end="")
            return False
        return True


def create_trusted_publisher(client: httpx.Client, crate_name: str) -> None:
    """Create a Trusted Publishing GitHub config for *crate_name*."""
    response = client.post(
        f"{CRATES_IO_API}/trusted_publishing/github_configs",
        json={
            "github_config": {
                "crate": crate_name,
                "repository_owner": REPOSITORY_OWNER,
                "repository_name": REPOSITORY_NAME,
                "workflow_filename": WORKFLOW_FILENAME,
                "environment": ENVIRONMENT,
            }
        },
    )
    response.raise_for_status()


def list_trusted_publishers(
    client: httpx.Client, crate_name: str
) -> list[dict[str, object]]:
    """List Trusted Publishing GitHub configs for *crate_name*."""
    response = client.get(
        f"{CRATES_IO_API}/trusted_publishing/github_configs",
        params={"crate": crate_name},
    )
    response.raise_for_status()
    payload = response.json()
    return payload.get("github_configs", [])


def delete_trusted_publisher(client: httpx.Client, config_id: int) -> None:
    """Delete a Trusted Publishing GitHub config by ID."""
    response = client.delete(
        f"{CRATES_IO_API}/trusted_publishing/github_configs/{config_id}"
    )
    response.raise_for_status()


def set_trustpub_only(client: httpx.Client, crate_name: str, enabled: bool) -> None:
    """Enable or disable `trustpub_only` for a crate."""
    response = client.patch(
        f"{CRATES_IO_API}/crates/{crate_name}",
        json={"crate": {"trustpub_only": enabled}},
    )
    response.raise_for_status()


def is_desired_config(config: dict[str, object]) -> bool:
    """Return True if *config* matches the workflow this repo uses."""
    return (
        config.get("repository_owner") == REPOSITORY_OWNER
        and config.get("repository_name") == REPOSITORY_NAME
        and config.get("workflow_filename") == WORKFLOW_FILENAME
        and config.get("environment") == ENVIRONMENT
    )


def handle_trusted_publisher_error(exc: httpx.HTTPStatusError) -> None:
    """Print a helpful hint for common trusted-publishing API errors, then exit."""
    print(
        f"error {exc.response.status_code}: {exc.response.text}",
        file=sys.stderr,
    )
    if exc.response.status_code == 401:
        if "GitHub session has expired" in exc.response.text:
            print(
                "\nhint: your crates.io account's linked GitHub OAuth"
                " token is stale.\nLog out and back in at"
                " https://crates.io to refresh it, then re-run.",
                file=sys.stderr,
            )
        else:
            print(
                "\nhint: your CARGO_REGISTRY_TOKEN may be invalid or revoked.",
                file=sys.stderr,
            )
    elif exc.response.status_code == 403:
        print(
            "\nhint: your token may lack the trusted-publishing scope,"
            " or you are not an owner of this crate.",
            file=sys.stderr,
        )
    sys.exit(1)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Ensure workspace crates are ready for trusted publishing on crates.io."
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Show what would be done without making changes",
    )
    parser.add_argument(
        "--force", action="store_true", help="Re-check crates already in .known-crates"
    )
    parser.add_argument(
        "--quiet", "-q", action="store_true", help="Suppress informational output"
    )
    args = parser.parse_args()

    dry_run = args.dry_run
    force = args.force
    quiet = args.quiet

    workspace_package = load_workspace_package_metadata()

    crates = get_publishable_crates()
    known = set() if force else load_known_crates()
    candidates = [c for c in crates if c["name"] not in known]

    if not candidates:
        if not quiet:
            print(
                f"All {len(crates)} publishable crates are in .known-crates — nothing to do."
            )
        return

    token = os.environ.get("CARGO_REGISTRY_TOKEN", "")
    if not token:
        print(
            "error: CARGO_REGISTRY_TOKEN is required (with `publish-new`"
            " and `trusted-publishing` scopes)",
            file=sys.stderr,
        )
        sys.exit(1)

    client = httpx.Client(headers={"User-Agent": USER_AGENT}, timeout=30)
    auth_client = httpx.Client(
        headers={"Authorization": token, "User-Agent": USER_AGENT},
        timeout=30,
    )

    s = "" if len(candidates) == 1 else "s"
    if not quiet:
        print(f"Checking {len(candidates)} crate{s}")

    published_any = False
    for crate in candidates:
        name = crate["name"]
        version = crate["version"]
        metadata = get_crate_metadata(client, name)
        exists = metadata is not None

        trustpub_only_enabled = False
        if exists:
            crate_payload = metadata.get("crate")
            if isinstance(crate_payload, dict):
                trustpub_only_enabled = bool(crate_payload.get("trustpub_only", False))

        configs: list[dict[str, object]] = []
        if exists:
            try:
                configs = list_trusted_publishers(auth_client, name)
            except httpx.HTTPStatusError as exc:
                print(f"{name}: failed to list trusted publishers", file=sys.stderr)
                handle_trusted_publisher_error(exc)

        desired_configs = [config for config in configs if is_desired_config(config)]
        extra_desired_configs = desired_configs[1:]
        non_matching_configs = [
            config for config in configs if not is_desired_config(config)
        ]
        configs_to_delete = non_matching_configs + extra_desired_configs

        needs_initial_publish = not exists
        needs_add_publisher = not desired_configs
        needs_trustpub_only = not trustpub_only_enabled

        if dry_run:
            actions: list[str] = []
            if needs_initial_publish:
                actions.append("publish placeholder")
            if configs_to_delete:
                count = len(configs_to_delete)
                noun = "publisher" if count == 1 else "publishers"
                actions.append(f"remove {count} {noun}")
            if needs_add_publisher:
                actions.append("add trusted publisher")
            if needs_trustpub_only:
                actions.append("enable trustpub_only")

            if actions:
                print(f"{name}: would {' and '.join(actions)}")
            elif not quiet:
                print(f"{name}: already configured")
            continue

        if needs_initial_publish:
            if version == PLACEHOLDER_VERSION:
                print(
                    f"{name}: workspace version matches placeholder version {PLACEHOLDER_VERSION}; "
                    "bump the real crate version before running this script",
                    file=sys.stderr,
                )
                sys.exit(1)

            if published_any:
                print(f"waiting {PUBLISH_DELAY_SECS}s for rate limit")
                time.sleep(PUBLISH_DELAY_SECS)

            print(f"{name}: publishing placeholder")
            if not publish_placeholder_crate(name, workspace_package):
                print(f"{name}: placeholder publish failed", file=sys.stderr)
                sys.exit(1)
            published_any = True

        if configs_to_delete:
            deleted = 0
            for config in configs_to_delete:
                config_id = config.get("id")
                if not isinstance(config_id, int):
                    print(
                        f"{name}: unexpected trusted publisher payload: missing `id`",
                        file=sys.stderr,
                    )
                    sys.exit(1)
                try:
                    delete_trusted_publisher(auth_client, config_id)
                except httpx.HTTPStatusError as exc:
                    print(
                        f"{name}: failed to delete trusted publisher {config_id}",
                        file=sys.stderr,
                    )
                    handle_trusted_publisher_error(exc)
                deleted += 1
            noun = "publisher" if deleted == 1 else "publishers"
            print(f"{name}: removed {deleted} {noun}")

        if needs_add_publisher:
            print(f"{name}: registering trusted publisher")
            try:
                create_trusted_publisher(auth_client, name)
            except httpx.HTTPStatusError as exc:
                handle_trusted_publisher_error(exc)

        if needs_trustpub_only:
            print(f"{name}: enabling trustpub_only")
            try:
                set_trustpub_only(auth_client, name, enabled=True)
            except httpx.HTTPStatusError as exc:
                handle_trusted_publisher_error(exc)

        if (
            not configs_to_delete
            and not needs_add_publisher
            and not needs_trustpub_only
            and not quiet
        ):
            print(f"{name}: already configured")

        known.add(name)

    if not dry_run:
        save_known_crates(known)


if __name__ == "__main__":
    main()
