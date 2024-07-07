# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "httpx < 1",
# ]
# ///
"""
Fetch Python version download metadata.

Generates the `download-metadata.json` file.

Usage:

    uv run --isolated -- crates/uv-python/fetch-download-metadata.py

Acknowledgements:

    Derived from https://github.com/mitsuhiko/rye/tree/f9822267a7f00332d15be8551f89a212e7bc9017
    Originally authored by Armin Ronacher under the MIT license
"""

import abc
import argparse
import asyncio
import itertools
import json
import logging
import re
import os
import sys
from enum import StrEnum
from dataclasses import dataclass
from pathlib import Path
from typing import NamedTuple, Self
from urllib.parse import unquote

import httpx

SELF_DIR = Path(__file__).parent
VERSIONS_FILE = SELF_DIR / "download-metadata.json"


def batched(iterable, n):
    """Batch data into tuples of length n. The last batch may be shorter."""
    # batched('ABCDEFG', 3) --> ABC DEF G
    if n < 1:
        raise ValueError("n must be at least one")
    it = iter(iterable)
    while batch := tuple(itertools.islice(it, n)):
        yield batch


class PlatformTriple(NamedTuple):
    arch: str
    platform: str
    libc: str


class Version(NamedTuple):
    major: int
    minor: int
    patch: int

    @classmethod
    def from_str(cls, version: str) -> Self:
        major, minor, patch = version.split(".", 3)
        return cls(int(major), int(minor), int(patch))

    def __str__(self) -> str:
        return f"{self.major}.{self.minor}.{self.patch}"

    def __neg__(self) -> Self:
        return Version(-self.major, -self.minor, -self.patch)


class ImplementationName(StrEnum):
    CPYTHON = "cpython"
    PYPY = "pypy"


@dataclass
class PythonDownload:
    version: Version
    triple: PlatformTriple
    flavor: str | None
    implementation: ImplementationName
    filename: str
    url: str
    sha256: str | None = None

    def key(self) -> str:
        return f"{self.implementation}-{self.version}-{self.triple.platform}-{self.triple.arch}-{self.triple.libc}"


class Finder:
    implementation: ImplementationName

    @abc.abstractmethod
    async def find(self) -> list[PythonDownload]:
        raise NotImplementedError


class CPythonFinder(Finder):
    implementation = ImplementationName.CPYTHON

    RELEASE_URL = (
        "https://api.github.com/repos/indygreg/python-build-standalone/releases"
    )

    FLAVOR_PREFERENCES = [
        "install_only",
        "shared-pgo",
        "shared-noopt",
        "static-noopt",
        "pgo+lto",
        "pgo",
        "lto",
        "debug",
    ]
    HIDDEN_FLAVORS = [
        "noopt",
    ]
    SPECIAL_TRIPLES = {
        "macos": "x86_64-apple-darwin",
        "linux64": "x86_64-unknown-linux-gnu",
        "windows-amd64": "x86_64-pc-windows",
        "windows-x86": "i686-pc-windows",
        "windows-amd64-shared": "x86_64-pc-windows",
        "windows-x86-shared": "i686-pc-windows",
        "linux64-musl": "x86_64-unknown-linux-musl",
    }
    # Normalized mappings to match the Rust types
    ARCH_MAP = {
        "ppc64": "powerpc64",
        "ppc64le": "powerpc64le",
    }

    _filename_re = re.compile(
        r"""(?x)
        ^
            cpython-(?P<ver>\d+\.\d+\.\d+?)
            (?:\+\d+)?
            -(?P<triple>.*?)
            (?:-[\dT]+)?\.tar\.(?:gz|zst)
        $
    """
    )

    _flavor_re = re.compile(
        r"""(?x)^(.*?)-(%s)$"""
        % (
            "|".join(
                map(
                    re.escape,
                    sorted(FLAVOR_PREFERENCES + HIDDEN_FLAVORS, key=len, reverse=True),
                )
            )
        )
    )

    def __init__(self, client: httpx.AsyncClient):
        self.client = client

    async def find(self) -> list[PythonDownload]:
        downloads = await self._fetch_downloads()
        await self._fetch_checksums(downloads, n=20)
        return downloads

    async def _fetch_downloads(self, pages: int = 100) -> list[PythonDownload]:
        """Fetch all the indygreg downloads from the release API."""
        results: dict[Version, list[PythonDownload]] = {}

        # Collect all available Python downloads
        for page in range(1, pages):
            logging.info("Fetching indygreg release page %d", page)
            resp = await self.client.get(self.RELEASE_URL, params={"page": page})
            resp.raise_for_status()
            rows = resp.json()
            if not rows:
                break
            for row in rows:
                for asset in row["assets"]:
                    url = asset["browser_download_url"]
                    download = self._parse_download_url(url)
                    if download is None:
                        continue
                    results.setdefault(download.version, []).append(download)

        # Collapse CPython variants to a single URL flavor per triple
        downloads = []
        for choices in results.values():
            flavors = {}
            for choice in choices:
                priority = self._get_flavor_priority(choice.flavor)
                existing = flavors.get(choice.triple)
                if existing:
                    _, existing_priority = existing
                    # Skip if we have a flavor with higher priority already
                    if priority >= existing_priority:
                        continue
                flavors[choice.triple] = (choice, priority)

            # Drop the priorities
            downloads.extend([choice for choice, _ in flavors.values()])

        return downloads

    async def _fetch_checksums(self, downloads: list[PythonDownload], n: int) -> None:
        """Fetch the checksums for the given downloads."""
        checksum_urls = set()
        for download in downloads:
            release_url = download.url.rsplit("/", maxsplit=1)[0]
            checksum_url = release_url + "/SHA256SUMS"
            checksum_urls.add(checksum_url)

        async def fetch_checksums(url: str) -> httpx.Response | None:
            try:
                resp = await self.client.get(url)
                resp.raise_for_status()
            except httpx.HTTPStatusError as e:
                if e.response.status_code == 404:
                    return None
                raise
            return resp

        completed = 0
        tasks = []
        for batch in batched(checksum_urls, n):
            logging.info("Fetching checksums: %d/%d", completed, len(checksum_urls))
            async with asyncio.TaskGroup() as tg:
                for url in batch:
                    task = tg.create_task(fetch_checksums(url))
                    tasks.append(task)
            completed += n

        checksums = {}
        for task in tasks:
            resp = task.result()
            if resp is None:
                continue
            lines = resp.text.splitlines()
            for line in lines:
                checksum, filename = line.split(" ", maxsplit=1)
                filename = filename.strip()
                checksums[filename] = checksum

        for download in downloads:
            download.sha256 = checksums.get(download.filename)

    def _parse_download_url(self, url: str) -> PythonDownload | None:
        """Parse an indygreg download URL into a PythonDownload object."""
        # Ex)
        # https://github.com/indygreg/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-aarch64-unknown-linux-gnu-lto-full.tar.zst
        filename = unquote(url.rsplit("/", maxsplit=1)[-1])
        if filename.endswith(".sha256"):
            return

        match = self._filename_re.match(filename)
        if match is None:
            return

        version, triple = match.groups()
        if triple.endswith("-full"):
            triple = triple[:-5]

        match = self._flavor_re.match(triple)
        if match is not None:
            triple, flavor = match.groups()
        else:
            flavor = None
        if flavor in self.HIDDEN_FLAVORS:
            return

        version = Version.from_str(version)
        triple = self._normalize_triple(triple)
        if triple is None:
            return

        return PythonDownload(
            version=version,
            triple=triple,
            flavor=flavor,
            implementation=self.implementation,
            filename=filename,
            url=url,
        )

    def _normalize_triple(self, triple: str) -> PlatformTriple | None:
        if "-static" in triple:
            logging.debug("Skipping %r: static unsupported", triple)
            return

        triple = self.SPECIAL_TRIPLES.get(triple, triple)
        pieces = triple.split("-")
        try:
            arch = self._normalize_arch(pieces[0])
            operating_system = self._normalize_os(pieces[2])
            if pieces[2] == "linux":
                # On linux, the triple has four segments, the last one is the libc
                libc = pieces[3]
            else:
                libc = "none"
        except IndexError:
            logging.debug("Skipping %r: unknown triple", triple)
            return

        return PlatformTriple(arch, operating_system, libc)

    def _normalize_arch(self, arch: str) -> str:
        arch = self.ARCH_MAP.get(arch, arch)
        pieces = arch.split("_")
        # Strip `_vN` from `x86_64`
        return "_".join(pieces[:2])

    def _normalize_os(self, os: str) -> str:
        return os

    def _get_flavor_priority(self, flavor: str) -> int:
        """
        Returns the priority of a flavor. Lower is better."""
        try:
            pref = self.FLAVOR_PREFERENCES.index(flavor)
        except ValueError:
            pref = len(self.FLAVOR_PREFERENCES) + 1
        return pref


def render(downloads: list[PythonDownload]) -> None:
    """Render `download-metadata.json`."""

    def sort_key(download: PythonDownload) -> tuple[int, Version, PlatformTriple]:
        # Sort by implementation, version (latest first), and then by triple.
        impl_order = [ImplementationName.CPYTHON, ImplementationName.PYPY]
        return (
            impl_order.index(download.implementation),
            -download.version,
            download.triple,
        )

    downloads.sort(key=sort_key)

    results = {}
    for download in downloads:
        key = download.key()
        logging.info("Found %s (%s)", key, download.flavor)
        results[key] = {
            "name": download.implementation,
            "arch": download.triple.arch,
            "os": download.triple.platform,
            "libc": download.triple.libc,
            "major": download.version.major,
            "minor": download.version.minor,
            "patch": download.version.patch,
            "url": download.url,
            "sha256": download.sha256,
        }

    VERSIONS_FILE.parent.mkdir(parents=True, exist_ok=True)
    # Make newlines consistent across platforms
    VERSIONS_FILE.write_text(json.dumps(results, indent=2), newline="\n")


async def find() -> None:
    token = os.environ.get("GH_TOKEN")
    if not token:
        logging.error("Please set `GH_TOKEN` environment variable.")
        sys.exit(1)

    headers = {
        "X-GitHub-Api-Version": "2022-11-28",
        "Authorization": "Bearer " + token,
    }
    client = httpx.AsyncClient(follow_redirects=True, headers=headers, timeout=15)

    # TODO: Add PyPyFinder
    finders = [
        CPythonFinder(client),
    ]
    downloads = []

    async with client:
        for finder in finders:
            logging.info("Finding %s downloads...", finder.implementation)
            downloads.extend(await finder.find())

    render(downloads)


def main():
    parser = argparse.ArgumentParser(description="Fetch Python version metadata.")
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
    args = parser.parse_args()

    if args.quiet:
        log_level = logging.CRITICAL
    elif args.verbose:
        log_level = logging.DEBUG
    else:
        log_level = logging.INFO

    logging.basicConfig(
        level=log_level,
        format="%(asctime)s %(levelname)s %(message)s",
        datefmt="%Y-%m-%d %H:%M:%S",
    )
    # Silence httpx logging
    logging.getLogger("httpx").setLevel(logging.WARNING)

    asyncio.run(find())


if __name__ == "__main__":
    main()
