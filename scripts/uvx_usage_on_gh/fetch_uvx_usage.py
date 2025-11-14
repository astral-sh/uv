# /// script
# requires-python = ">=3.13"
# dependencies = [
# 	"httpx"
# ]
# ///

"""
Use the GitHub Code Search API to find instances of `uvx <package>` in:
- README files (*.md)
- Shell scripts (*.sh, *.bash, *.zsh)

Requirements:
    - A GitHub Personal Access Token (PAT) with `public_repo` scope
    - Set the GITHUB_TOKEN environment variable or pass --token

Usage:
    python scripts/uvx_usage_on_gh/fetch_uvx_usage.py --output crates/uv/src/commands/tool/top_packages.txt
"""

import argparse
import asyncio
import logging
import os
import re
import sys
import time
from collections import Counter
from pathlib import Path
from typing import Any, NamedTuple, Optional

import httpx

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(levelname)s - %(message)s",
)
logger = logging.getLogger(__name__)

# GitHub API configuration
GITHUB_API_BASE = "https://api.github.com"
CODE_SEARCH_ENDPOINT = f"{GITHUB_API_BASE}/search/code"

# Rate limiting configuration
RATE_LIMIT_DELAY = 6.1  # seconds between requests (slightly more than 60/10)

# GitHub Code Search API limits
GITHUB_CODE_SEARCH_MAX_RESULTS = 1000  # Hard limit: only first 1000 results accessible
GITHUB_CODE_SEARCH_MAX_PAGE = 10  # Page 10 = results 901-1000, page 11+ returns 422

# Retry configuration
MAX_RETRIES = 5
INITIAL_RETRY_DELAY = 10  # seconds
MAX_RETRY_DELAY = 300  # 5 minutes max delay

# PyPI check concurrency
PYPI_CONCURRENT_CHECKS = 20  # Number of concurrent PyPI checks

# PyPI API endpoint
PYPI_JSON_API_TEMPLATE = "https://pypi.org/pypi/{package}/json"


class RateLimitInfo(NamedTuple):
    remaining: int | None
    reset_time: int | None


class GitHubSearchResponse(NamedTuple):
    items: list[dict[str, Any]]
    total_count: int
    rate_limit: RateLimitInfo


# Regex patterns for extracting package names
PACKAGE_PATTERN_FROM = re.compile(
    r"\buvx\s+(?:--\w+(?:\s+\S+)?\s+)*--from\s+([a-z0-9](?:[a-z0-9._-]*[a-z0-9])?)(?:@\S+)?",
    re.IGNORECASE,
)
PACKAGE_PATTERN_NORMAL = re.compile(
    r"\buvx\s+(?:--\w+(?:\s+\S+)?\s+)+([a-z0-9](?:[a-z0-9._-]*[a-z0-9])?)(?:@\S+)?",
    re.IGNORECASE,
)
PACKAGE_PATTERN_SIMPLE = re.compile(
    r"\buvx\s+([a-z0-9](?:[a-z0-9._-]*[a-z0-9])?)(?:@\S+)?",
    re.IGNORECASE,
)
URL_PATTERN = re.compile(
    r"\buvx\s+(?:--\w+(?:\s+\S+)?\s+)*--from\s+(git\+[a-z]+://|git://|https?://)",
    re.IGNORECASE,
)


def extract_package_name(match_text: str) -> Optional[str]:
    """
    Extract package name from a match.

    Handles patterns like:
    - uvx ruff
    - uvx --from httpie http (extracts "httpie")
    - uvx --python 3.12 textual-demo
    - uvx black@latest
    - uvx pytest --version
    - uvx streamlit run streamlit_app/dashboard.py

    Skips patterns like:
    - uvx --from git+https://... (URLs are not package names)
    - uvx --from http://... (URLs are not package names)
    """
    # Skip URLs after --from
    if URL_PATTERN.search(match_text):
        return None

    # Try patterns in order: --from, flags, simple
    match = (
        PACKAGE_PATTERN_FROM.search(match_text)
        or PACKAGE_PATTERN_NORMAL.search(match_text)
        or PACKAGE_PATTERN_SIMPLE.search(match_text)
    )

    if not match:
        return None

    package = match.group(1).lower()

    # Remove version specifiers (e.g., @latest, @1.0.0)
    if "@" in package:
        package = package.split("@")[0]

    # Validation checks
    if package.startswith("--") or "/" in package or "\\" in package or len(package) < 2:
        return None

    return package


def _calculate_retry_delay(
    status_code: int,
    retry_count: int,
    response_headers: httpx.Headers,
) -> int:
    """Calculate delay for retry based on status code and headers."""
    if status_code in (403, 429):
        # Try Retry-After header first
        retry_after = response_headers.get("Retry-After")
        if retry_after:
            try:
                return int(retry_after) + 2  # Add 2 second buffer
            except ValueError:
                pass

        # Fall back to X-RateLimit-Reset
        reset_time_str = response_headers.get("X-RateLimit-Reset")
        if reset_time_str:
            try:
                reset_time = int(reset_time_str)
                current_time = int(time.time())
                return max(reset_time - current_time + 2, 10)  # At least 10 seconds

            except ValueError:
                pass

    # Default: exponential backoff
    return min(INITIAL_RETRY_DELAY * (2**retry_count), MAX_RETRY_DELAY)


def search_github_code(
    query: str,
    token: str,
    page: int,
    per_page: int = 100,
    retry_count: int = 0,
) -> GitHubSearchResponse:
    headers = {
        "Accept": "application/vnd.github.text-match+json",
        "Authorization": f"Bearer {token}",
    }

    params = {
        "q": query,
        "page": page,
        "per_page": min(per_page, 100),
    }

    logger.info(f"Searching GitHub: {query} (page {page}, attempt {retry_count + 1})")

    try:
        response = httpx.get(
            CODE_SEARCH_ENDPOINT,
            headers=headers,
            params=params,
            timeout=30.0,
        )
        response.raise_for_status()

        # Extract rate limit info
        remaining_str = response.headers.get("X-RateLimit-Remaining")
        reset_time_str = response.headers.get("X-RateLimit-Reset")
        rate_limit = RateLimitInfo(
            remaining=int(remaining_str) if remaining_str else None,
            reset_time=int(reset_time_str) if reset_time_str else None,
        )

        logger.debug(
            f"Rate limit remaining: {rate_limit.remaining}, reset at: {rate_limit.reset_time}"
        )

        data = response.json()
        total_count = data.get("total_count", 0)
        logger.info(f"Count of total results: {total_count}")

        return GitHubSearchResponse(
            items=data.get("items", []),
            total_count=total_count,
            rate_limit=rate_limit,
        )

    except httpx.HTTPStatusError as e:
        status_code = e.response.status_code

        # 422 on page 11+ is likely the hard 1000 result limit
        if status_code == 422 and page > GITHUB_CODE_SEARCH_MAX_PAGE:
            logger.info(
                f"422 error on page {page} - likely hit GitHub's 1000 result limit. "
                f"Code Search API only returns first {GITHUB_CODE_SEARCH_MAX_RESULTS} results."
            )
            raise ValueError(
                f"Reached GitHub Code Search API limit (page {page} > {GITHUB_CODE_SEARCH_MAX_PAGE})"
            ) from e

        # Retryable errors
        if status_code in (403, 422, 429) and retry_count < MAX_RETRIES:
            delay = _calculate_retry_delay(status_code, retry_count, e.response.headers)

            if status_code == 403:
                logger.warning(
                    f"Rate limit exceeded (403). Retrying in {delay}s "
                    f"(attempt {retry_count + 1}/{MAX_RETRIES})"
                )
            elif status_code == 429:
                logger.warning(
                    f"Rate limit exceeded (429). Retrying in {delay}s "
                    f"(attempt {retry_count + 1}/{MAX_RETRIES})"
                )
            elif status_code == 422:
                logger.warning(
                    f"Validation error (422) - may be transient. Retrying in {delay}s "
                    f"(attempt {retry_count + 1}/{MAX_RETRIES})"
                )

            time.sleep(delay)
            return search_github_code(query, token, page, per_page, retry_count + 1)

        # Non-retryable or max retries reached
        if status_code == 403:
            logger.error(
                "Rate limit exceeded or authentication failed after retries. "
                "Check your token and wait before retrying."
            )
        elif status_code == 422:
            logger.error(f"Invalid query after retries: {query}")
        else:
            logger.error(f"HTTP error {status_code} after retries")

    except httpx.RequestError as e:
        # Network errors are retryable
        if retry_count < MAX_RETRIES:
            delay = min(INITIAL_RETRY_DELAY * (2**retry_count), MAX_RETRY_DELAY)
            logger.warning(
                f"Request failed: {e}. Retrying in {delay}s "
                f"(attempt {retry_count + 1}/{MAX_RETRIES})"
            )
            time.sleep(delay)
            return search_github_code(query, token, page, per_page, retry_count + 1)

        logger.error(f"Request failed after retries: {e}")
        raise


async def wait_for_rate_limit(rate_limit: RateLimitInfo) -> None:
    """
    Wait if we're approaching rate limit or need to wait until reset.

    Args:
        rate_limit: Rate limit information from previous request
    """
    if rate_limit.remaining is None or rate_limit.reset_time is None:
        await asyncio.sleep(RATE_LIMIT_DELAY)
        return

    # If running low on requests, wait until reset
    if rate_limit.remaining <= 2:
        wait_time = rate_limit.reset_time - int(time.time()) + 2  # Add 2 second buffer
        if wait_time > 0:
            logger.info(
                f"Rate limit low ({rate_limit.remaining} remaining). "
                f"Waiting {wait_time}s until reset at {rate_limit.reset_time}"
            )
            await asyncio.sleep(wait_time)
        else:
            await asyncio.sleep(RATE_LIMIT_DELAY)
    else:
        await asyncio.sleep(RATE_LIMIT_DELAY)


def build_size_query(base_query: str, start_bytes: int, end_bytes: Optional[int]) -> str:
    """Build a GitHub Code Search query with size filter."""
    if end_bytes is None:
        return f"{base_query} size:>={start_bytes}"
    return f"{base_query} size:{start_bytes}..{end_bytes}"


async def check_pypi_package_exists(
    package: str,
    cache: dict[str, bool],
    client: httpx.AsyncClient,
) -> tuple[str, bool]:
    """
    Check if a single package exists on PyPI.

    Args:
        package: Package name to check
        cache: Dictionary to cache results (modified in-place)
        client: httpx async client instance

    Returns:
        Tuple of (package_name, exists)
    """
    # Check cache first
    if package in cache:
        return (package, cache[package])

    url = PYPI_JSON_API_TEMPLATE.format(package=package)

    try:
        response = await client.get(url, timeout=10.0, follow_redirects=True)
        exists = response.status_code == 200
        cache[package] = exists

        if exists:
            logger.debug(f"✓ {package} exists on PyPI")
        else:
            logger.debug(f"✗ {package} not found on PyPI")

        return (package, exists)
    except httpx.RequestError as e:
        logger.debug(f"Error checking {package} on PyPI: {e}")
        cache[package] = False
        return (package, False)


async def check_packages_batch(
    packages: list[str],
    cache: dict[str, bool],
    semaphore: asyncio.Semaphore,
) -> dict[str, bool]:
    """
    Check a batch of packages against PyPI concurrently.

    Args:
        packages: List of package names to check
        cache: Dictionary to cache results (modified in-place)
        semaphore: Semaphore to limit concurrent requests

    Returns:
        Dictionary mapping package names to their existence status
    """
    async def check_one(package: str) -> tuple[str, bool]:
        async with semaphore:
            async with httpx.AsyncClient() as client:
                return await check_pypi_package_exists(package, cache, client)

    tasks = [check_one(pkg) for pkg in packages]
    results = await asyncio.gather(*tasks, return_exceptions=False)
    return dict(results)


def extract_packages_from_items(items: list[dict[str, Any]]) -> Counter:
    page_packages = Counter()

    for item in items:
        # Extract from text_matches (code snippets)
        text_matches = item.get("text_matches", [])
        for match in text_matches:
            fragment = match.get("fragment", "")
            package = extract_package_name(fragment)
            if package:
                page_packages[package] += 1
                logger.debug(f"Found package: {package}")

        # Also check file path/name
        path = item.get("path", "")
        if "uvx" in path.lower():
            package = extract_package_name(path)
            if package:
                page_packages[package] += 1

    return page_packages


async def search_uvx_usage(
    token: str, max_pages: int = 10
) -> tuple[Counter[str], dict[str, bool]]:
    """
    Search for uvx usage across GitHub and extract package names.

    Processes packages incrementally and checks PyPI concurrently.

    Args:
        token: GitHub Personal Access Token
        max_pages: Maximum number of pages to fetch per query (default: 10)

    Returns:
        Tuple of (Counter of valid package names with counts, updated PyPI cache)
    """
    pypi_cache: dict[str, bool] = {}
    valid_package_counts: Counter[str] = Counter()
    all_package_counts: Counter[str] = Counter()
    unknown_packages_queue: list[str] = []

    semaphore = asyncio.Semaphore(PYPI_CONCURRENT_CHECKS)
    current_rate_limit = RateLimitInfo(None, None)

    # Size buckets to work around GitHub's 1000 result limit
    # It would be way smarter to do this dynamically (query a given size range and do a 
    # binary/proportional split on the number of results) but I already got this far
    # so I'm not going to change it for now.
    markdown_size_buckets = [
        (0, 1025),
        (1025, 1250),
        (1250, 1500),
        (1500, 1750),
        (1750, 2000),
        (2000, 2500),
        (2500, 3500),
        (3500, 4500),
        (4500, 5500),
        (5500, 6250),
        (6250, 7000),
        (7000, 7750),
        (7750, 8500),
        (8500, 9250),
        (9250, 10000),
        (10000, 10750),
        (10750, 11750),
        (11750, 13000),
        (13000, 14000),
        (14000, 15250),
        (15250, 16250),
        (16250, 17500),
        (17500, 18750),
        (18750, 20000),
        (20000, 22000),
        (22000, 24000),
        (24000, 26000),
        (26000, 28000),
        (28000, 30000),
        (30000, 33000),
        (33000, 36000),
        (36000, 39000),
        (39000, 42000),
        (42000, 45000),
        (45000, 50000),
        (50000, 60000),
        (60000, 70000),
        (70000, 80000),
        (80000, 100000),
        (100000, 120000),
        (120000, 140000),
        (140000, 160000),
        (160000, 180000),
        (180000, 200000),
        (200000, 250000),
        (250000, 300000),
        (300000, None),
    ]

    shell_size_buckets = [
        (0, 2800),
        (2800, 6000),
        (6000, 15000),
        (15000, 32000),
        (32000, None),
    ]

    queries = [
        build_size_query("uvx AND language:Markdown in:file", start, end)
        for start, end in markdown_size_buckets
    ]
    queries.extend(
        build_size_query("uvx AND language:Shell in:file", start, end)
        for start, end in shell_size_buckets
    )

    async def process_unknown_packages() -> None:
        """Process queued unknown packages against PyPI."""
        if not unknown_packages_queue:
            return

        packages_to_check = list(set(unknown_packages_queue))
        unknown_packages_queue.clear()

        logger.info(f"Checking {len(packages_to_check)} unknown packages against PyPI...")
        results = await check_packages_batch(packages_to_check, pypi_cache, semaphore)

        # Update valid package counts based on results
        for package, exists in results.items():
            if exists:
                count = all_package_counts.get(package, 0)
                if count > 0:
                    valid_package_counts[package] = count
                    logger.debug(f"Added {package} to valid packages ({count} occurrences)")
                else:
                    logger.warning(f"Package {package} validated but has no count")

    for query_idx, query in enumerate(queries):
        page = 1
        effective_max_pages = min(max_pages, GITHUB_CODE_SEARCH_MAX_PAGE)

        # Wait before starting a new query (except the first one)
        if query_idx > 0:
            logger.debug("Waiting before starting new query...")
            await wait_for_rate_limit(current_rate_limit)
            await process_unknown_packages()

        while page <= effective_max_pages:
            try:
                # Rate limiting: wait between page requests (except for the first page)
                if page > 1:
                    logger.debug("Waiting before next page...")
                    await wait_for_rate_limit(current_rate_limit)
                    await process_unknown_packages()

                response = search_github_code(query, token, page=page)

                # Update rate limit state from response
                current_rate_limit = response.rate_limit

                items = response.items
                if not items:
                    logger.info(f"No more results for query: {query}")
                    break

                logger.info(f"Found {len(items)} results on page {page}")

                # Extract package names from this page
                page_packages = extract_packages_from_items(items)

                # Process packages from this page
                for package, count in page_packages.items():
                    all_package_counts[package] += count

                    # Check cache first
                    if package in pypi_cache:
                        if pypi_cache[package]:
                            valid_package_counts[package] = all_package_counts[package]
                            logger.debug(
                                f"Known valid: {package} (total: {all_package_counts[package]})"
                            )
                    else:
                        unknown_packages_queue.append(package)

                # Process unknown packages while we have time before next GitHub request
                if unknown_packages_queue:
                    await process_unknown_packages()

                # Check if there are more pages
                effective_total = min(
                    response.total_count, GITHUB_CODE_SEARCH_MAX_RESULTS
                )

                if len(items) < 100 or page * 100 >= effective_total:
                    logger.info(
                        f"Reached end of results for query: {query} "
                        f"(page {page}, total: {response.total_count})"
                    )
                    break

                page += 1

            except ValueError as e:
                # This is raised when we hit the 1000 result limit
                logger.info(f"Hit GitHub Code Search API limit: {e}")
                break
            except Exception as e:
                logger.error(f"Error processing page {page} of query '{query}': {e}")
                break

        # Process any remaining unknown packages after each query
        await process_unknown_packages()

    # Final processing of any remaining unknown packages
    await process_unknown_packages()

    logger.info(
        f"Found {len(valid_package_counts)} valid PyPI packages "
        f"out of {len(all_package_counts)} total"
    )

    return valid_package_counts, pypi_cache


def write_top_packages(
    package_counts: Counter[str],
    output_path: Path,
    debug_output_path: Path,
    min_count: int = 2,
) -> None:
    """
    Write top packages to files, sorted by frequency.

    Packages are written in buckets by threshold (100+, 25+, 10+, 5+, min_count+).

    Args:
        package_counts: Counter of package names and counts
        output_path: Path to output file (main packages list)
        debug_output_path: Path to debug output file (with counts)
        min_count: Minimum occurrence count to include (default: 2)
    """
    thresholds = [min_count, 5, 10, 25, 100]

    # Filter packages into buckets by threshold
    buckets = []
    for i, threshold in enumerate(thresholds):
        next_threshold = thresholds[i + 1] if i + 1 < len(thresholds) else float("inf")
        bucket_packages = {
            pkg: count
            for pkg, count in package_counts.items()
            if threshold <= count < next_threshold
        }
        buckets.append({"threshold": threshold, "packages": bucket_packages})

    with open(output_path, "w") as f, open(debug_output_path, "w") as f_debug:
        for bucket in reversed(buckets):
            threshold = bucket["threshold"]
            packages = bucket["packages"]
            logger.info(
                f"Greater than or equal to {threshold} mentions: {len(packages)} packages"
            )

            # Sort by count descending, then alphabetically
            sorted_packages = sorted(
                packages.items(), key=lambda x: (-x[1], x[0])
            )

            for package, count in sorted_packages:
                f.write(f"{package}\n")
                f_debug.write(f"{package}: {count}\n")

    logger.info(f"Successfully wrote top packages to {output_path}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Fetch popular packages from GitHub by searching for uvx usage"
    )
    parser.add_argument(
        "--token",
        type=str,
        help="GitHub Personal Access Token (or set GITHUB_TOKEN env var)",
        default=os.getenv("GITHUB_TOKEN"),
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="Output file path (default: top_packages.txt)",
        default=None,
    )
    parser.add_argument(
        "--debug-output",
        type=Path,
        help="Debug output file path (default: top_packages_debug.txt)",
        default=None,
    )
    parser.add_argument(
        "--max-pages",
        type=int,
        default=10,
        help="Maximum pages to fetch per query (default: 10)",
    )
    parser.add_argument(
        "--min-count",
        type=int,
        default=2,
        help="Minimum occurrence count to include (default: 2)",
    )
    parser.add_argument(
        "--verbose",
        "-v",
        action="store_true",
        help="Enable verbose logging",
    )

    args = parser.parse_args()

    if args.verbose:
        logging.getLogger().setLevel(logging.DEBUG)

    if not args.token:
        logger.error(
            "GitHub token is required. Set GITHUB_TOKEN environment variable "
            "or pass --token. Create a token at: https://github.com/settings/tokens"
        )
        sys.exit(1)

    # Set default output paths
    if args.output is None or args.debug_output is None:
        script_dir = Path(__file__).parent
        project_root = script_dir.parent.parent
        if args.output is None:
            args.output = (
                project_root
                / "crates"
                / "uv"
                / "src"
                / "commands"
                / "tool"
                / "top_packages.txt"
            )
        if args.debug_output is None:
            args.debug_output = (
                project_root
                / "crates"
                / "uv"
                / "src"
                / "commands"
                / "tool"
                / "top_packages_debug.txt"
            )

    logger.info("Starting GitHub search for uvx usage...")
    logger.info(f"Output will be written to: {args.output}")
    logger.info(f"Debug output will be written to: {args.debug_output}")

    valid_packages, pypi_cache = asyncio.run(
        search_uvx_usage(args.token, max_pages=args.max_pages)
    )

    if not valid_packages:
        logger.warning("No valid PyPI packages found.")
        sys.exit(1)

    logger.info(f"Found {len(valid_packages)} valid PyPI packages")
    logger.info(f"Top 10 valid packages: {valid_packages.most_common(10)}")
    logger.info(f"PyPI cache contains {len(pypi_cache)} entries")

    write_top_packages(
        valid_packages, args.output, args.debug_output, min_count=args.min_count
    )


if __name__ == "__main__":
    main()
