#!/usr/bin/env python3.12
"""
Fetch Python version download metadata.

Generates the `download-metadata.json` file.

Usage:

    python fetch-download-metadata.py

Acknowledgements:

    Derived from https://github.com/mitsuhiko/rye/tree/f9822267a7f00332d15be8551f89a212e7bc9017
    Originally authored by Armin Ronacher under the MIT license
"""

import argparse
import hashlib
import json
import logging
import re
import urllib.error
import urllib.request
from itertools import chain
from pathlib import Path
from urllib.parse import unquote

SELF_DIR = Path(__file__).parent
RELEASE_URL = "https://api.github.com/repos/indygreg/python-build-standalone/releases"
HEADERS = {
    "X-GitHub-Api-Version": "2022-11-28",
}
VERSIONS_FILE = SELF_DIR / "download-metadata.json"
FLAVOR_PREFERENCES = [
    "shared-pgo",
    "shared-noopt",
    "shared-noopt",
    "static-noopt",
    "gnu-pgo+lto",
    "gnu-lto",
    "gnu-pgo",
    "pgo+lto",
    "lto",
    "pgo",
]
HIDDEN_FLAVORS = [
    "debug",
    "noopt",
    "install_only",
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
_suffix_re = re.compile(
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

# Normalized mappings to match the Rust types
ARCH_MAP = {
    "ppc64": "powerpc64",
    "ppc64le": "powerpc64le",
}


def parse_filename(filename):
    match = _filename_re.match(filename)
    if match is None:
        return
    version, triple = match.groups()
    if triple.endswith("-full"):
        triple = triple[:-5]
    match = _suffix_re.match(triple)
    if match is not None:
        triple, suffix = match.groups()
    else:
        suffix = None
    return (version, triple, suffix)


def normalize_triple(triple):
    if "-static" in triple:
        logging.debug("Skipping %r: static unsupported", triple)
        return
    triple = SPECIAL_TRIPLES.get(triple, triple)
    pieces = triple.split("-")
    try:
        arch = normalize_arch(pieces[0])
        operating_system = normalize_os(pieces[2])
        if pieces[2] == "linux":
            # On linux, the triple has four segments, the last one is the libc
            libc = pieces[3]
        else:
            libc = "none"
    except IndexError:
        logging.debug("Skipping %r: unknown triple", triple)
        return
    return "%s-%s-%s" % (arch, operating_system, libc)


def normalize_arch(arch):
    arch = ARCH_MAP.get(arch, arch)
    pieces = arch.split("_")
    # Strip `_vN` from `x86_64`
    return "_".join(pieces[:2])


def normalize_os(os):
    return os


def read_sha256(url):
    try:
        resp = urllib.request.urlopen(url + ".sha256")
    except urllib.error.HTTPError:
        return None
    assert resp.status == 200
    return resp.read().decode().strip()


def sha256(path):
    h = hashlib.sha256()

    with open(path, "rb") as file:
        while True:
            # Reading is buffered, so we can read smaller chunks.
            chunk = file.read(h.block_size)
            if not chunk:
                break
            h.update(chunk)

    return h.hexdigest()


def _sort_by_flavor_preference(info):
    _triple, flavor, _url = info
    try:
        pref = FLAVOR_PREFERENCES.index(flavor)
    except ValueError:
        pref = len(FLAVOR_PREFERENCES) + 1
    return pref


def _sort_by_interpreter_and_version(info):
    interpreter, version_tuple, _ = info
    return (interpreter, version_tuple)


def find():
    """
    Find available Python versions and write metadata to a file.
    """
    results = {}

    # Collect all available Python downloads
    for page in range(1, 100):
        logging.debug("Reading release page %s...", page)
        resp = urllib.request.urlopen("%s?page=%d" % (RELEASE_URL, page))
        rows = json.loads(resp.read())
        if not rows:
            break
        for row in rows:
            for asset in row["assets"]:
                url = asset["browser_download_url"]
                base_name = unquote(url.rsplit("/")[-1])
                if base_name.endswith(".sha256"):
                    continue
                info = parse_filename(base_name)
                if info is None:
                    continue
                py_ver, triple, flavor = info
                if "-static" in triple or (flavor and "noopt" in flavor):
                    continue
                triple = normalize_triple(triple)
                if triple is None:
                    continue
                results.setdefault(py_ver, []).append((triple, flavor, url))

    # Collapse CPython variants to a single URL flavor per triple
    cpython_results: dict[tuple[int, int, int], dict[tuple[str, str, str], str]] = {}
    for py_ver, choices in results.items():
        urls = {}
        for triple, flavor, url in sorted(choices, key=_sort_by_flavor_preference):
            triple = tuple(triple.split("-"))
            # Skip existing triples, preferring the first flavor
            if triple in urls:
                continue
            urls[triple] = url
        cpython_results[tuple(map(int, py_ver.split(".")))] = urls

    # Collect variants across interpreter kinds
    # TODO(zanieb): Note we only support CPython downloads at this time
    #               but this will include PyPy chain in the future.
    final_results = {}
    for interpreter, py_ver, choices in sorted(
        chain(
            (("cpython",) + x for x in cpython_results.items()),
        ),
        key=_sort_by_interpreter_and_version,
        # Reverse the ordering so newer versions are first
        reverse=True,
    ):
        # Sort by the remaining information for determinism
        # This groups download metadata in triple component order
        for (arch, operating_system, libc), url in sorted(choices.items()):
            key = "%s-%s.%s.%s-%s-%s-%s" % (
                interpreter,
                *py_ver,
                operating_system,
                arch,
                libc,
            )
            logging.info("Found %s", key)
            sha256 = read_sha256(url)

            final_results[key] = {
                "name": interpreter,
                "arch": arch,
                "os": operating_system,
                "libc": libc,
                "major": py_ver[0],
                "minor": py_ver[1],
                "patch": py_ver[2],
                "url": url,
                "sha256": sha256,
            }

    VERSIONS_FILE.parent.mkdir(parents=True, exist_ok=True)
    VERSIONS_FILE.write_text(json.dumps(final_results, indent=2))


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

    find()


if __name__ == "__main__":
    main()
