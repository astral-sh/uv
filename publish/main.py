"""Proxy a GitHub release to Cloudflare R2.

Example usage:
    uv run --env-file .env -- main.py https://github.com/astral-sh/uv-wheelnext/releases/tag/0.8.3
"""

# /// script
# requires-python = ">=3.13"
# dependencies = [
#     "requests",
# ]
# ///

import argparse
import logging
import os
import re
import subprocess
import sys
from pathlib import Path

import requests


def parse_release_url(url: str) -> tuple[str, str, str]:
    """Parse a GitHub release URL into components.

    For example, given https://github.com/astral-sh/uv/releases/tag/0.6.14,
    this function will return `("astral-sh", "uv", "0.6.14")`.
    """
    pattern = r"https://github\.com/([^/]+)/([^/]+)/releases/tag/([\w.-]+)"
    match = re.match(pattern, url)
    if not match:
        raise ValueError("Invalid GitHub release URL")
    owner, repo, version = match.groups()
    return owner, repo, version


def download_release_assets(url: str, output_dir: Path) -> None:
    """Download all assets from a GitHub release."""
    owner, repo, version = parse_release_url(url)

    # Create the output directory.
    output_dir.mkdir(parents=True, exist_ok=True)

    # Extract the release payload from the GitHub API.
    api_url = f"https://api.github.com/repos/{owner}/{repo}/releases/tags/{version}"
    headers = {}
    if "GITHUB_TOKEN" in os.environ:
        headers["Authorization"] = f"Bearer {os.environ['GITHUB_TOKEN']}"

    response = requests.get(api_url, headers=headers)
    response.raise_for_status()
    release = response.json()

    if not release["assets"]:
        print("No assets found")
        return

    # Download each asset to disk
    for asset in release["assets"]:
        asset_name = asset["name"]
        output_path = output_dir / asset_name

        # Skip if file already exists
        if output_path.exists():
            print(f"Skipping {asset_name} (already exists)")
            continue

        # Get download URL using the API URL.
        download_url = asset["url"]
        headers = {
            "Accept": "application/octet-stream",
        }
        if "GITHUB_TOKEN" in os.environ:
            headers["Authorization"] = f"Bearer {os.environ['GITHUB_TOKEN']}"
        response = requests.get(download_url, headers=headers, allow_redirects=True)
        response.raise_for_status()

        # Write the asset to disk.
        output_path.write_bytes(response.content)
        print(f"Saved to {output_path}")


def upload_to_r2(local_dir: Path, version: str) -> None:
    """Upload files to Cloudflare R2 using wrangler."""
    bucket = "uv-wheelnext-releases"

    # Upload each file
    for file_path in local_dir.iterdir():
        if file_path.is_file():
            print(f"Uploading {file_path.name} to R2...")
            r2_path = file_path.name

            try:
                result = subprocess.run(
                    [
                        "npx",
                        "wrangler",
                        "r2",
                        "object",
                        "put",
                        "--remote",
                        f"{bucket}/{r2_path}",
                        "--file",
                        str(file_path),
                        "--content-type",
                        "application/octet-stream",
                    ],
                    check=True,
                    capture_output=True,
                )
                print(f"Command output:\n{result.stdout.decode()}")
                if result.stderr:
                    print(f"Command stderr:\n{result.stderr.decode()}")
                print(f"Uploaded to {bucket}/{r2_path}")
            except subprocess.CalledProcessError as e:
                print(f"Error uploading {file_path.name}: {e.stderr.decode()}")
                sys.exit(1)


def main() -> None:
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s - %(levelname)s  - %(name)s - %(message)s",
    )

    parser = argparse.ArgumentParser(
        description="Download GitHub release assets and upload to Cloudflare R2"
    )
    parser.add_argument(
        "url",
        help="GitHub release URL (e.g., https://github.com/astral-sh/uv/releases/tag/0.6.14)",
    )
    args = parser.parse_args()

    # Extract the version from the URL.
    _, _, version = parse_release_url(args.url)

    # Create the downloads directory.
    downloads_dir = Path.cwd() / "downloads" / version
    downloads_dir.mkdir(parents=True, exist_ok=True)

    # Download release assets.
    print("Downloading release assets...")
    download_release_assets(args.url, downloads_dir)

    # Upload to R2.
    print("Uploading to Cloudflare R2...")
    upload_to_r2(downloads_dir, version)

    print("All done!")


if __name__ == "__main__":
    main()
