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

    uv run -- crates/uv-python/fetch-download-metadata.py

Acknowledgements:

    Derived from https://github.com/mitsuhiko/rye/tree/f9822267a7f00332d15be8551f89a212e7bc9017
    Originally authored by Armin Ronacher under the MIT license
"""
# https://github.com/mitsuhiko/rye/raw/f9822267a7f00332d15be8551f89a212e7bc9017/LICENSE
#
# MIT License
#
# Copyright (c) 2023, Armin Ronacher
#
# Permission is hereby granted, free of charge, to any person obtaining a copy
# of this software and associated documentation files (the "Software"), to deal
# in the Software without restriction, including without limitation the rights
# to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
# copies of the Software, and to permit persons to whom the Software is
# furnished to do so, subject to the following conditions:
#
# The above copyright notice and this permission notice shall be included in all
# copies or substantial portions of the Software.
#
# THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
# IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
# FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
# AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
# LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
# OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
# SOFTWARE.

import abc
import argparse
import asyncio
import itertools
import json
import logging
import os
import re
from dataclasses import asdict, dataclass, field
from enum import StrEnum
from pathlib import Path
from typing import Generator, Iterable, NamedTuple, Self
from urllib.parse import unquote

import httpx

SELF_DIR = Path(__file__).parent
VERSIONS_FILE = SELF_DIR / "download-metadata.json"

# The date at which the default CPython musl builds became dynamically linked
# instead of statically.
CPYTHON_MUSL_STATIC_RELEASE_END = 20250311


def batched(iterable: Iterable, n: int) -> Generator[tuple, None, None]:
    """Batch data into tuples of length n. The last batch may be shorter."""
    # batched('ABCDEFG', 3) --> ABC DEF G
    if n < 1:
        raise ValueError("n must be at least one")
    it = iter(iterable)
    while batch := tuple(itertools.islice(it, n)):
        yield batch


@dataclass(frozen=True)
class Arch:
    # The architecture family, e.g. "x86_64", "aarch64".
    family: str
    # The architecture variant, e.g., "v2" in "x86_64_v2"
    variant: str | None = None

    def key(self) -> str:
        return str(self)

    def __str__(self) -> str:
        return (self.family + "_" + self.variant) if self.variant else self.family

    def __gt__(self, other) -> bool:
        return (self.family, self.variant or "") > (other.family, other.variant or "")

    def __lt__(self, other) -> bool:
        return (self.family, self.variant or "") < (other.family, other.variant or "")


type PlatformTripleKey = tuple[str, str, str]


class PlatformTriple(NamedTuple):
    # The operating system, e.g. "linux", "macos", "windows".
    platform: str
    # The architecture, e.g. "x86_64", "aarch64".
    arch: Arch
    # The libc implementation, e.g. "gnu", "musl", "none".
    libc: str

    def key(self) -> PlatformTripleKey:
        return (self.platform, self.arch.key(), self.libc)


class Version(NamedTuple):
    major: int
    minor: int
    patch: int
    prerelease: str = ""

    @classmethod
    def from_str(cls, version: str) -> Self:
        major, minor, patch = version.split(".", 3)
        prerelease = ""
        for prerelease_kind in ("a", "b", "rc"):
            parts = patch.split(prerelease_kind, 1)
            if len(parts) == 2:
                patch = parts[0]
                prerelease = prerelease_kind + parts[1]
                break

        return cls(int(major), int(minor), int(patch), prerelease)

    def __str__(self) -> str:
        return f"{self.major}.{self.minor}.{self.patch}{self.prerelease}"


class ImplementationName(StrEnum):
    CPYTHON = "cpython"
    PYPY = "pypy"


class Variant(StrEnum):
    FREETHREADED = "freethreaded"
    DEBUG = "debug"


@dataclass
class PythonDownload:
    release: int
    version: Version
    triple: PlatformTriple
    flavor: str
    implementation: ImplementationName
    filename: str
    url: str
    sha256: str | None = None
    build_options: list[str] = field(default_factory=list)
    variant: Variant | None = None

    def key(self) -> str:
        if self.variant:
            return f"{self.implementation}-{self.version}+{self.variant}-{self.triple.platform}-{self.triple.arch}-{self.triple.libc}"
        else:
            return f"{self.implementation}-{self.version}-{self.triple.platform}-{self.triple.arch}-{self.triple.libc}"


class Finder:
    implementation: ImplementationName

    @abc.abstractmethod
    async def find(self) -> list[PythonDownload]:
        raise NotImplementedError


class CPythonFinder(Finder):
    implementation = ImplementationName.CPYTHON

    RELEASE_URL = (
        "https://api.github.com/repos/astral-sh/python-build-standalone/releases"
    )

    FLAVOR_PREFERENCES = [
        "install_only_stripped",
        "install_only",
        "shared-pgo",
        "shared-noopt",
        "static-noopt",
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
            cpython-
            (?P<ver>\d+\.\d+\.\d+(?:(?:a|b|rc)\d+)?)(?:\+\d+)?\+
            (?P<date>\d+)-
            (?P<triple>[a-z\d_]+-[a-z\d]+(?>-[a-z\d]+)?-[a-z\d]+)-
            (?:(?P<build_options>.+)-)?
            (?P<flavor>[a-z_]+)?
            \.tar\.(?:gz|zst)
        $
        """
    )

    _legacy_filename_re = re.compile(
        r"""(?x)
        ^
            cpython-
            (?P<ver>\d+\.\d+\.\d+(?:(?:a|b|rc)\d+)?)(?:\+\d+)?-
            (?P<triple>[a-z\d_-]+)-
            (?P<build_options>(debug|pgo|noopt|lto|pgo\+lto))?-
            (?P<date>[a-zA-z\d]+)
            \.tar\.(?:gz|zst)
        $
        """
    )

    def __init__(self, client: httpx.AsyncClient):
        self.client = client

    async def find(self) -> list[PythonDownload]:
        downloads = await self._fetch_downloads()
        await self._fetch_checksums(downloads, n=20)
        return downloads

    async def _fetch_downloads(self, pages: int = 100) -> list[PythonDownload]:
        """Fetch all the indygreg downloads from the release API."""
        downloads_by_version: dict[Version, list[PythonDownload]] = {}

        # Collect all available Python downloads
        for page in range(1, pages + 1):
            logging.info("Fetching CPython release page %d", page)
            resp = await self.client.get(
                self.RELEASE_URL, params={"page": page, "per_page": 10}
            )
            resp.raise_for_status()
            rows = resp.json()
            if not rows:
                break
            for row in rows:
                # Sort the assets to ensure deterministic results
                row["assets"].sort(key=lambda asset: asset["browser_download_url"])
                for asset in row["assets"]:
                    url = asset["browser_download_url"]
                    download = self._parse_download_url(url)
                    if download is None:
                        continue
                    if (
                        download.release < CPYTHON_MUSL_STATIC_RELEASE_END
                        and download.triple.libc == "musl"
                    ):
                        continue
                    logging.debug("Found %s (%s)", download.key(), download.filename)
                    downloads_by_version.setdefault(download.version, []).append(
                        download
                    )

        # Collapse CPython variants to a single flavor per triple and variant
        downloads = []
        for version_downloads in downloads_by_version.values():
            selected: dict[
                tuple[PlatformTripleKey, Variant | None],
                tuple[PythonDownload, tuple[int, int]],
            ] = {}
            for download in version_downloads:
                priority = self._get_priority(download)
                existing = selected.get((download.triple.key(), download.variant))
                if existing:
                    existing_download, existing_priority = existing
                    # Skip if we have a flavor with higher priority already (indicated by a smaller value)
                    if priority >= existing_priority:
                        logging.debug(
                            "Skipping %s (%s): lower priority than %s (%s)",
                            download.key(),
                            download.flavor,
                            existing_download.key(),
                            existing_download.flavor,
                        )
                        continue
                selected[(download.triple.key(), download.variant)] = (
                    download,
                    priority,
                )

            # Drop the priorities
            downloads.extend([download for download, _ in selected.values()])

        return downloads

    async def _fetch_checksums(self, downloads: list[PythonDownload], n: int) -> None:
        """Fetch the checksums for the given downloads."""
        checksum_urls = set()
        for download in downloads:
            release_base_url = download.url.rsplit("/", maxsplit=1)[0]
            checksum_url = release_base_url + "/SHA256SUMS"
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
            logging.info(
                "Fetching CPython checksums: %d/%d", completed, len(checksum_urls)
            )
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
        # https://github.com/astral-sh/python-build-standalone/releases/download/20240107/cpython-3.12.1%2B20240107-aarch64-unknown-linux-gnu-lto-full.tar.zst
        if url.endswith(".sha256"):
            return None
        filename = unquote(url.rsplit("/", maxsplit=1)[-1])
        release = int(url.rsplit("/")[-2])

        match = self._filename_re.match(filename) or self._legacy_filename_re.match(
            filename
        )
        if match is None:
            logging.debug("Skipping %s: no regex match", filename)
            return None

        groups = match.groupdict()
        version = groups["ver"]
        triple = groups["triple"]
        build_options = groups.get("build_options")
        flavor = groups.get("flavor", "full")

        build_options = build_options.split("+") if build_options else []
        variant: Variant | None
        for variant in Variant:
            if variant in build_options:
                break
        else:
            variant = None

        version = Version.from_str(version)
        triple = self._normalize_triple(triple)
        if triple is None:
            # Skip is logged in `_normalize_triple`
            return None

        return PythonDownload(
            release=release,
            version=version,
            triple=triple,
            flavor=flavor,
            implementation=self.implementation,
            filename=filename,
            url=url,
            build_options=build_options,
            variant=variant,
        )

    def _normalize_triple(self, triple: str) -> PlatformTriple | None:
        if "-static" in triple:
            logging.debug("Skipping %r: static unsupported", triple)
            return None

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
            return None

        return PlatformTriple(operating_system, arch, libc)

    def _normalize_arch(self, arch: str) -> Arch:
        arch = self.ARCH_MAP.get(arch, arch)
        pieces = arch.split("_")
        family = "_".join(pieces[:2])
        variant = pieces[2] if len(pieces) > 2 else None
        return Arch(family, variant)

    def _normalize_os(self, os: str) -> str:
        return os

    def _get_priority(self, download: PythonDownload) -> tuple[int, int]:
        """
        Returns the priority of a download, a lower score is better.
        """
        flavor_priority = self._flavor_priority(download.flavor)
        build_option_priority = self._build_option_priority(download.build_options)
        return (flavor_priority, build_option_priority)

    def _flavor_priority(self, flavor: str) -> int:
        try:
            priority = self.FLAVOR_PREFERENCES.index(flavor)
        except ValueError:
            priority = len(self.FLAVOR_PREFERENCES) + 1
        return priority

    def _build_option_priority(self, build_options: list[str]) -> int:
        # Prefer optimized builds
        return -1 * sum(
            (
                "lto" in build_options,
                "pgo" in build_options,
                "static" not in build_options,
            )
        )


class PyPyFinder(Finder):
    implementation = ImplementationName.PYPY

    RELEASE_URL = "https://raw.githubusercontent.com/pypy/pypy/main/pypy/tool/release/versions.json"
    CHECKSUM_URL = (
        "https://raw.githubusercontent.com/pypy/pypy.org/main/pages/checksums.rst"
    )

    _checksum_re = re.compile(
        r"^\s*(?P<checksum>\w{64})\s+(?P<filename>pypy.+)$", re.MULTILINE
    )

    ARCH_MAPPING = {
        "x64": "x86_64",
        "x86": "i686",
        "i686": "i686",
        "aarch64": "aarch64",
        "arm64": "aarch64",
        "s390x": "s390x",
    }

    PLATFORM_MAPPING = {
        "win32": "windows",
        "win64": "windows",
        "linux": "linux",
        "darwin": "darwin",
    }

    def __init__(self, client: httpx.AsyncClient):
        self.client = client

    async def find(self) -> list[PythonDownload]:
        downloads = await self._fetch_downloads()
        await self._fetch_checksums(downloads)
        return downloads

    async def _fetch_downloads(self) -> list[PythonDownload]:
        resp = await self.client.get(self.RELEASE_URL)
        resp.raise_for_status()
        versions = resp.json()

        results = {}
        for version in versions:
            if not version["stable"]:
                continue
            python_version = Version.from_str(version["python_version"])
            if python_version < (3, 7, 0):
                continue
            for file in version["files"]:
                arch = self._normalize_arch(file["arch"])
                platform = self._normalize_os(file["platform"])
                libc = "gnu" if platform == "linux" else "none"
                download = PythonDownload(
                    release=0,
                    version=python_version,
                    triple=PlatformTriple(
                        platform=platform,
                        arch=arch,
                        libc=libc,
                    ),
                    flavor="",
                    implementation=self.implementation,
                    filename=file["filename"],
                    url=file["download_url"],
                )
                # Only keep the latest pypy version of each arch/platform
                if (python_version, arch, platform) not in results:
                    results[(python_version, arch, platform)] = download

        return list(results.values())

    def _normalize_arch(self, arch: str) -> Arch:
        return Arch(self.ARCH_MAPPING.get(arch, arch), None)

    def _normalize_os(self, os: str) -> str:
        return self.PLATFORM_MAPPING.get(os, os)

    async def _fetch_checksums(self, downloads: list[PythonDownload]) -> None:
        logging.info("Fetching PyPy checksums")
        resp = await self.client.get(self.CHECKSUM_URL)
        resp.raise_for_status()
        text = resp.text

        checksums = {}
        for match in self._checksum_re.finditer(text):
            checksums[match.group("filename")] = match.group("checksum")

        for download in downloads:
            download.sha256 = checksums.get(download.filename)


def render(downloads: list[PythonDownload]) -> None:
    """Render `download-metadata.json`."""

    def prerelease_sort_key(prerelease: str) -> tuple[int, int]:
        if prerelease.startswith("a"):
            return 0, int(prerelease[1:])
        if prerelease.startswith("b"):
            return 1, int(prerelease[1:])
        if prerelease.startswith("rc"):
            return 2, int(prerelease[2:])
        return 3, 0

    def variant_sort_key(variant: Variant | None) -> int:
        if variant is None:
            return 0
        match variant:
            case Variant.FREETHREADED:
                return 1
            case Variant.DEBUG:
                return 2
        raise ValueError(f"Missing sort key implementation for variant: {variant}")

    def sort_key(download: PythonDownload) -> tuple:
        # Sort by implementation, version (latest first), and then by triple.
        impl_order = [ImplementationName.CPYTHON, ImplementationName.PYPY]
        prerelease = prerelease_sort_key(download.version.prerelease)
        return (
            impl_order.index(download.implementation),
            -download.version.major,
            -download.version.minor,
            -download.version.patch,
            -prerelease[0],
            -prerelease[1],
            variant_sort_key(download.variant),
            download.triple,
        )

    downloads.sort(key=sort_key)

    results = {}
    for download in downloads:
        key = download.key()
        logging.info(
            "Selected %s%s", key, (" (%s)" % download.flavor) if download.flavor else ""
        )
        results[key] = {
            "name": download.implementation,
            "arch": asdict(download.triple.arch),
            "os": download.triple.platform,
            "libc": download.triple.libc,
            "major": download.version.major,
            "minor": download.version.minor,
            "patch": download.version.patch,
            "prerelease": download.version.prerelease,
            "url": download.url,
            "sha256": download.sha256,
            "variant": download.variant if download.variant else None,
        }

    VERSIONS_FILE.parent.mkdir(parents=True, exist_ok=True)
    # Make newlines consistent across platforms
    VERSIONS_FILE.write_text(json.dumps(results, indent=2), newline="\n")


async def find() -> None:
    token = os.environ.get("GITHUB_TOKEN")
    if not token:
        logging.warning(
            "`GITHUB_TOKEN` env var not found, you may hit rate limits for GitHub API requests."
        )

    headers = {
        "X-GitHub-Api-Version": "2022-11-28",
        "Accept-Encoding": "gzip, deflate",
    }
    if token:
        headers["Authorization"] = "Bearer " + token
    client = httpx.AsyncClient(follow_redirects=True, headers=headers, timeout=15)

    finders = [
        CPythonFinder(client),
        PyPyFinder(client),
    ]
    downloads = []

    async with client:
        for finder in finders:
            logging.info("Finding %s downloads...", finder.implementation)
            downloads.extend(await finder.find())

    render(downloads)


def main() -> None:
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
    logging.getLogger("httpcore").setLevel(logging.WARNING)

    asyncio.run(find())


if __name__ == "__main__":
    main()
