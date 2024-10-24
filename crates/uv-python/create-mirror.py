import argparse
import asyncio
import hashlib
import json
import logging
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple
from urllib.parse import unquote

import httpx
from git import GitCommandError, Repo  # GitPython
from tqdm import tqdm

# Constants
SELF_DIR = Path(__file__).parent
REPO_ROOT = SELF_DIR.parent.parent
VERSIONS_FILE = SELF_DIR / "download-metadata.json"
DST_DIR = SELF_DIR / "mirror"
PREFIXES = [
    "https://github.com/indygreg/python-build-standalone/releases/download/",
    "https://downloads.python.org/pypy/",
]

# Logger setup
logging.basicConfig(
    level=logging.INFO, format="%(asctime)s - %(levelname)s - %(message)s"
)
logger = logging.getLogger(__name__)

# Silence httpx logging
logging.getLogger("httpx").setLevel(logging.WARNING)
logging.getLogger("httpcore").setLevel(logging.WARNING)


def sanitize_url(url: str) -> Path:
    """Remove the prefix from the URL, decode it, and convert it to a relative path."""
    for prefix in PREFIXES:
        if url.startswith(prefix):
            return Path(unquote(url[len(prefix) :]))  # Decode the URL path
    return Path(unquote(url))  # Fallback to full decoded path if no prefix matched


def sha256_checksum(file_path: Path) -> str:
    """Calculate the SHA-256 checksum of a file."""
    hasher = hashlib.sha256()
    with open(file_path, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def collect_metadata_from_git_history() -> List[Dict]:
    """Collect all metadata entries from the history of the VERSIONS_FILE."""
    metadata = []
    try:
        repo = Repo(REPO_ROOT, search_parent_directories=True)

        for commit in repo.iter_commits(paths=VERSIONS_FILE):
            try:
                # Ensure the file exists in the commit tree
                blob = commit.tree / str(VERSIONS_FILE.relative_to(REPO_ROOT))
                content = blob.data_stream.read().decode()
                data = json.loads(content)
                metadata.extend(data.values())
            except KeyError:
                logger.warning(
                    f"File {VERSIONS_FILE} not found in commit {commit.hexsha}. Skipping."
                )
            except json.JSONDecodeError as e:
                logger.error(f"Error decoding JSON in commit {commit.hexsha}: {e}")

    except GitCommandError as e:
        logger.error(f"Git command error: {e}")
    except Exception as e:
        logger.exception(f"Unexpected error while collecting metadata: {e}")

    return metadata


def filter_metadata(
    metadata: List[Dict], name: Optional[str], arch: Optional[str], os: Optional[str]
) -> List[Dict]:
    """Filter the metadata based on name, architecture, and OS, ensuring unique URLs."""
    filtered = [
        entry
        for entry in metadata
        if (not name or entry["name"] == name)
        and (not arch or entry["arch"] == arch)
        and (not os or entry["os"] == os)
    ]
    # Use a set to ensure unique URLs
    unique_urls = set()
    unique_filtered = []
    for entry in filtered:
        if entry["url"] not in unique_urls:
            unique_urls.add(entry["url"])
            unique_filtered.append(entry)
    return unique_filtered


async def download_file(
    client: httpx.AsyncClient,
    url: str,
    dest: Path,
    expected_sha256: Optional[str],
    progress_bar,
    errors,
):
    """Download a file and verify its SHA-256 checksum if provided."""
    if dest.exists() and expected_sha256 and sha256_checksum(dest) == expected_sha256:
        logger.debug(
            f"File {dest} already exists and SHA-256 matches. Skipping download."
        )
        progress_bar.update(1)
        return True  # Success, even though skipped
    elif dest.exists() and expected_sha256 is None:
        logger.debug(
            f"File {dest} already exists no SHA-256 provided. Skipping download."
        )
        progress_bar.update(1)
        return True  # Success, even though skipped

    if not any(url.startswith(prefix) for prefix in PREFIXES):
        error_msg = f"No valid prefix found for {url}. Skipping."
        logger.warning(error_msg)
        errors.append((url, error_msg))
        progress_bar.update(1)
        return False

    dest.parent.mkdir(parents=True, exist_ok=True)
    logger.debug(f"Downloading {url} to {dest}")

    try:
        async with client.stream("GET", url) as response:
            response.raise_for_status()
            with open(dest, "wb") as f:
                async for chunk in response.aiter_bytes():
                    f.write(chunk)

        if expected_sha256 and sha256_checksum(dest) != expected_sha256:
            error_msg = f"SHA-256 mismatch for {dest}. Deleting corrupted file."
            logger.error(error_msg)
            dest.unlink()
            errors.append((url, "Checksum mismatch"))
            progress_bar.update(1)
            return False

    except Exception as e:
        error_msg = f"Failed to download {url}: {str(e)}"
        logger.error(error_msg)
        errors.append((url, str(e)))
        progress_bar.update(1)
        return False

    progress_bar.update(1)
    return True


async def download_files(urls: Set[Tuple[str, Optional[str]]], max_concurrent: int):
    """Download files with a limit on concurrent downloads using httpx."""
    async with httpx.AsyncClient(follow_redirects=True) as client:
        progress_bar = tqdm(total=len(urls), desc="Downloading", unit="file")
        sem = asyncio.Semaphore(max_concurrent)
        errors: List[Tuple[str, str]] = []  # To collect errors
        success_count = 0  # Track number of successful downloads

        async def sem_download(url, sha256):
            nonlocal success_count
            async with sem:
                success = await download_file(
                    client,
                    url,
                    DST_DIR / sanitize_url(url),
                    sha256,
                    progress_bar,
                    errors,
                )
                if success:
                    success_count += 1

        tasks = [sem_download(url, sha256) for url, sha256 in urls]
        await asyncio.gather(*tasks)
        progress_bar.close()

        return success_count, errors


def parse_arguments():
    """Parse command-line arguments using argparse."""
    parser = argparse.ArgumentParser(description="Download and mirror Python builds.")
    parser.add_argument("--name", help="Filter by name (e.g., 'cpython').")
    parser.add_argument("--arch", help="Filter by architecture (e.g., 'aarch64').")
    parser.add_argument("--os", help="Filter by operating system (e.g., 'darwin').")
    parser.add_argument(
        "--max-concurrent",
        type=int,
        default=20,
        help="Maximum number of simultaneous downloads.",
    )
    parser.add_argument(
        "--from-all-history",
        action="store_true",
        help="Collect URLs from the entire git history.",
    )
    return parser.parse_args()


def main():
    """Main function to run the CLI."""
    args = parse_arguments()

    if args.from_all_history:
        metadata = collect_metadata_from_git_history()
    else:
        with open(VERSIONS_FILE) as f:
            metadata = list(json.load(f).values())

    filtered_metadata = filter_metadata(metadata, args.name, args.arch, args.os)
    urls = {(entry["url"], entry["sha256"]) for entry in filtered_metadata}

    if not urls:
        logger.error("No URLs found.")
        return

    logger.info(f"Starting download of {len(urls)} files.")
    try:
        success_count, errors = asyncio.run(download_files(urls, args.max_concurrent))
        print(f"Successfully downloaded: {success_count}")
        if errors:
            print("Failed downloads:")
            for url, error in errors:
                print(f"- {url}: {error}")
        print(
            f"now you can run UV_PYTHON_INSTALL_MIRROR='file://{DST_DIR.absolute()}' uv python install"
        )
    except Exception as e:
        logger.error(f"Error during download: {e}")


if __name__ == "__main__":
    main()
