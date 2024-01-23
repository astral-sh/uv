"""
Install Python versions required for Puffin development.

Requirements:

    Requires Python 3.12 to be installed already.

    # Install requirements
    python3.12 -m pip install -r scripts/bootstrap/requirements.txt

Usage:

    # Install the versions required for Puffin development
    cat .python-versions | xargs -L1 python3.12 scripts/bootstrap install
    
    # Pull available versions from GitHub
    python3.12 scripts/bootstrap find

    # List available versions for the current system
    python3.12 scripts/bootstrap list

    # Install a version
    python3.12 scripts/bootstrap install <version>

    # Install all available versions
    python3.12 scripts/bootstrap list | xargs -L1 -P3 python3.12 scripts/bootstrap install

    # Add the binaries to your path
    export PATH=$PWD/bootstrap/bin:$PATH

Acknowledgements:

    Derived from https://github.com/mitsuhiko/rye/tree/f9822267a7f00332d15be8551f89a212e7bc9017
    Originally authored by Armin Ronacher under the MIT license
"""
import argparse
import os
import json
import re
import hashlib
import requests
import platform
import sys
import logging
import shutil
import functools
import zstandard

from itertools import chain
from urllib.parse import unquote
import tempfile
import tarfile
from pathlib import Path

PROJECT_ROOT = Path(__file__).parents[2]
RELEASE_URL = "https://api.github.com/repos/indygreg/python-build-standalone/releases"
HEADERS = {
    "X-GitHub-Api-Version": "2022-11-28",
}
BOOTSTRAP_DIR = PROJECT_ROOT / "bootstrap"
SELF_DIR = BOOTSTRAP_DIR / "self"
VERSIONS_METADATA = SELF_DIR / "versions.json"
BIN_DIR = BOOTSTRAP_DIR / "bin"
VERSIONS_DIR = BOOTSTRAP_DIR / "versions"
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
    "linux64": "x86_64-unknown-linux",
    "windows-amd64": "x86_64-pc-windows",
    "windows-x86": "i686-pc-windows",
    "linux64-musl": "x86_64-unknown-linux",
}

# matches these: https://doc.rust-lang.org/std/env/consts/constant.ARCH.html
ARCH_MAPPING = {
    "x86_64": "x86_64",
    "x86": "x86",
    "i686": "x86",
    "aarch64": "aarch64",
    "arm64": "aarch64",
}

# matches these: https://doc.rust-lang.org/std/env/consts/constant.OS.html
PLATFORM_MAPPING = {
    "darwin": "macos",
    "windows": "windows",
    "linux": "linux",
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
    if "-musl" in triple or "-static" in triple:
        return
    triple = SPECIAL_TRIPLES.get(triple, triple)
    pieces = triple.split("-")
    try:
        arch = ARCH_MAPPING.get(pieces[0])
        if arch is None:
            return
        platform = PLATFORM_MAPPING.get(pieces[2])
        if platform is None:
            return
    except IndexError:
        return
    return "%s-%s" % (arch, platform)


def read_sha256(session, url):
    resp = session.get(url + ".sha256")
    if not resp.ok:
        return None
    return resp.text.strip()


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


def _sort_key(info):
    triple, flavor, url = info
    try:
        pref = FLAVOR_PREFERENCES.index(flavor)
    except ValueError:
        pref = len(FLAVOR_PREFERENCES) + 1
    return pref


def get_session() -> requests.Session:
    session = requests.Session()
    session.headers = HEADERS.copy()

    token = os.environ.get("GITHUB_TOKEN")
    if token:
        session.headers["Authorization"] = "Bearer " + token
    else:
        logging.warning(
            "An authentication token was not found at `GITHUB_TOKEN`, rate limits may be encountered.",
        )

    return session


def download_file(session, url, target_dir):
    local_path = Path(target_dir) / url.split("/")[-1]
    with session.get(url, stream=True) as response:
        response.raise_for_status()
        # Patch read to decode the content
        response.raw.read = functools.partial(response.raw.read, decode_content=True)
        with local_path.open("wb") as f:
            shutil.copyfileobj(response.raw, f)

    return local_path


def decompress_file(archive_path: Path, output_path: Path):
    if archive_path.suffix == ".zst":
        dctx = zstandard.ZstdDecompressor()

        with tempfile.TemporaryFile(suffix=".tar") as ofh:
            with archive_path.open("rb") as ifh:
                dctx.copy_stream(ifh, ofh)
            ofh.seek(0)
            with tarfile.open(fileobj=ofh) as z:
                z.extractall(output_path)
    else:
        raise ValueError(f"Unknown archive type {archive_path.suffix}")


def find(args):
    """
    Find available Python versions and write metadata to a file.
    """
    if VERSIONS_METADATA.exists() and args and not args.refresh:
        logging.info(
            "Version metadata already exist at %s (use --refresh to update)",
            VERSIONS_METADATA.relative_to(PROJECT_ROOT),
        )
        return

    results = {}
    session = get_session()

    for page in range(1, 100):
        logging.debug("Reading release page %s...", page)
        resp = session.get("%s?page=%d" % (RELEASE_URL, page))
        rows = resp.json()
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

    cpython_results = {}
    for py_ver, choices in results.items():
        choices.sort(key=_sort_key)
        urls = {}
        for triple, flavor, url in choices:
            triple = tuple(triple.split("-"))
            if triple in urls:
                continue
            urls[triple] = url
        cpython_results[tuple(map(int, py_ver.split(".")))] = urls

    final_results = []
    for interpreter, py_ver, choices in sorted(
        chain(
            (("cpython",) + x for x in cpython_results.items()),
        ),
        key=lambda x: x[:2],
        reverse=True,
    ):
        for (arch, py_os), url in sorted(choices.items()):
            logging.info("Found %s-%s.%s.%s-%s-%s", interpreter, *py_ver, arch, py_os)
            sha256 = read_sha256(session, url)

            final_results.append(
                {
                    "name": interpreter,
                    "arch": arch,
                    "os": py_os,
                    "major": py_ver[0],
                    "minor": py_ver[1],
                    "patch": py_ver[2],
                    "url": url,
                    "sha256": sha256,
                }
            )

    VERSIONS_METADATA.parent.mkdir(parents=True, exist_ok=True)
    VERSIONS_METADATA.write_text(json.dumps(final_results, indent=2))


def clean(_):
    """
    Remove any artifacts created by bootstrapping
    """
    if BIN_DIR.exists():
        logging.info(
            "Clearing binaries at %s",
            BIN_DIR.relative_to(PROJECT_ROOT),
        )
        shutil.rmtree(BIN_DIR)

    if VERSIONS_DIR.exists():
        logging.info(
            "Clearing installed versions at %s",
            VERSIONS_DIR.relative_to(PROJECT_ROOT),
        )
        shutil.rmtree(VERSIONS_DIR)

    if VERSIONS_METADATA.exists():
        logging.info(
            "Clearing version cache at %s",
            VERSIONS_METADATA.relative_to(PROJECT_ROOT),
        )
        VERSIONS_METADATA.unlink()

    logging.info("Done!")


def list(_):
    """
    List available versions
    """
    if not VERSIONS_METADATA.exists():
        logging.info("No version metadata found, fetching download links...")
        find(args=None)

    logging.info(
        "Using version metadata from %s", VERSIONS_METADATA.relative_to(PROJECT_ROOT)
    )
    version_metdata = json.loads(VERSIONS_METADATA.read_text())

    target_os = PLATFORM_MAPPING[sys.platform]
    target_arch = ARCH_MAPPING[platform.machine()]

    logging.info("Using system %s-%s", target_os, target_arch)
    for version in version_metdata:
        if version["os"] == target_os and version["arch"] == target_arch:
            print(
                "%s@%s.%s.%s"
                % (
                    version["name"],
                    version["major"],
                    version["minor"],
                    version["patch"],
                )
            )


def install(args):
    """
    Fetch and install the given Python version
    """
    if not VERSIONS_METADATA.exists():
        logging.info("No version metadata found, fetching download links...")
        find(args=None)

    logging.info(
        "Using version metadata from %s", VERSIONS_METADATA.relative_to(PROJECT_ROOT)
    )
    version_metdata = json.loads(VERSIONS_METADATA.read_text())

    target_os = PLATFORM_MAPPING[sys.platform]
    target_arch = ARCH_MAPPING[platform.machine()]

    logging.info("Using system %s-%s", target_os, target_arch)

    parsed_version = args.version.split("@")
    if len(parsed_version) == 2:
        python_name = parsed_version[0]
        python_version = parsed_version[1]
    elif len(parsed_version) == 1:
        python_name = "cpython"
        python_version = parsed_version[0]
    else:
        logging.critical(
            "Expected Python version formatted as 'name@major.minor.patch' but got: %r",
            args.version,
        )
        sys.exit(1)

    python_version = python_version.split(".")
    if not len(python_version) == 3:
        logging.critical(
            "Expected Python version formatted as 'major.minor.patch' but got: %r",
            args.version,
        )
        sys.exit(1)
    logging.info(
        "Searching for compatible Python version %s@%s",
        python_name,
        ".".join(python_version),
    )
    for version in version_metdata:
        if (
            version["name"] == python_name
            and version["os"] == target_os
            and version["arch"] == target_arch
            and str(version["major"]) == python_version[0]
            and str(version["minor"]) == python_version[1]
            and str(version["patch"]) == python_version[2]
        ):
            break
    else:
        logging.critical("No matching version found!")
        sys.exit(1)

    name = f"{version['name']}@{'.'.join(python_version)}"
    install_path = VERSIONS_DIR / name
    if install_path.exists():
        if not args.reinstall:
            logging.info("Python version %s already downloaded", name)
        else:
            shutil.rmtree(install_path)

    # Only download if it does not exist, but always create the links
    if not install_path.exists():
        session = get_session()
        VERSIONS_DIR.mkdir(parents=True, exist_ok=True)

        logging.info("Downloading %s", name)
        archive_file = download_file(session, version["url"], VERSIONS_DIR)

        if version["sha256"]:
            logging.info("Verifying hash...")
            if sha256(archive_file) != version["sha256"]:
                logging.critical("Hash verification failed!")
                sys.exit(1)
        else:
            logging.warning("Skipping hash verification: no hash for release")

        logging.debug("Decompressing %s", archive_file.name)
        tmp_dir = VERSIONS_DIR / f"{name}.tmp"
        if tmp_dir.exists():
            shutil.rmtree(tmp_dir)
        tmp_dir.mkdir()
        decompress_file(archive_file, tmp_dir)

        # Remove the downloaded archive
        archive_file.unlink()

        # Rename the extracted direcotry
        (tmp_dir / "python").rename(install_path)

        # Remove the temporary directory
        tmp_dir.rmdir()

    # Link binaries
    BIN_DIR.mkdir(exist_ok=True, parents=True)
    python_executable = install_path / "install" / "bin" / f"python{python_version[0]}"
    if not python_executable.exists():
        logging.critical("Python executable not found at %s", python_executable)
        sys.exit(1)

    full = BIN_DIR / f"python{'.'.join(python_version)}"
    minor = BIN_DIR / f"python{python_version[0]}.{python_version[1]}"
    major = BIN_DIR / f"python{python_version[0]}"
    default = BIN_DIR / "python"

    full.unlink(missing_ok=True)
    full.symlink_to(python_executable.relative_to(BIN_DIR, walk_up=True))
    logging.info("Installed to %s", full.relative_to(PROJECT_ROOT))

    if args.default_minor:
        minor.unlink(missing_ok=True)
        minor.symlink_to(python_executable.relative_to(BIN_DIR, walk_up=True))
        logging.info("Installed to %s", minor.relative_to(PROJECT_ROOT))

    if args.default:
        major.unlink(missing_ok=True)
        major.symlink_to(python_executable.relative_to(BIN_DIR, walk_up=True))

        default.unlink(missing_ok=True)
        default.symlink_to(python_executable.relative_to(BIN_DIR, walk_up=True))

        logging.info("Installed to %s", major.relative_to(PROJECT_ROOT))
        logging.info("Installed to %s", default.relative_to(PROJECT_ROOT))


def _add_find_parser(subparsers):
    parser = subparsers.add_parser(
        "find", help="Find available Python downloads and store metadata"
    )
    parser.set_defaults(call=find)

    parser.add_argument(
        "--refresh",
        action="store_true",
        help="Redownload versions if they already exist.",
    )
    _add_shared_arguments(parser)


def _add_clean_parser(subparsers):
    parser = subparsers.add_parser(
        "clean", help="Remove all artifacts from bootstrapping"
    )
    parser.set_defaults(call=clean)
    _add_shared_arguments(parser)


def _add_list_parser(subparsers):
    parser = subparsers.add_parser(
        "list", help="List all available versions for the current system"
    )
    parser.set_defaults(call=list)
    _add_shared_arguments(parser)


def _add_install_parser(subparsers):
    parser = subparsers.add_parser(
        "install", help="Fetch and install the given Python version"
    )
    parser.add_argument(
        "version",
        type=str,
        help="The Python version to install e.g. '3.11.4'",
    )
    parser.add_argument(
        "--default-minor",
        action="store_true",
        help="Use this patch Python version as the default when the minor version is requested.",
    )
    parser.add_argument(
        "--default",
        action="store_true",
        help="Use this Python version as the default.",
    )
    parser.add_argument(
        "--reinstall",
        action="store_true",
        help="Reinstall the version if it already exists.",
    )

    parser.set_defaults(call=install)
    _add_shared_arguments(parser)


def _add_shared_arguments(parser):
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


def get_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Bootstrap Puffin development.")
    _add_shared_arguments(parser)
    subparsers = parser.add_subparsers(title="commands")
    _add_find_parser(subparsers)
    _add_install_parser(subparsers)
    _add_list_parser(subparsers)
    _add_clean_parser(subparsers)

    return parser


def main():
    parser = get_parser()
    args = parser.parse_args()

    if not hasattr(args, "call"):
        parser.print_help()
        return None

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

    args.call(args)


if __name__ == "__main__":
    main()
