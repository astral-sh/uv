#!/usr/bin/env python3
#
# Download required Python versions and install to `bin`
# Uses prebuilt Python distributions from indygreg/python-build-standalone
#
# This script can be run without Python isntalled via `install.sh`
#
# Requirements
#
#   pip install zstandard==0.22.0
#
# Usage
#
#   python scripts/bootstrap/install.py
#
# The Python versions are installed from `.python_versions`.
# Python versions are linked in-order such that the _last_ defined version will be the default.
#
# Version metadata can be updated with `fetch-version-metadata.py`

import hashlib
import json
import platform
import shutil
import sys
import tarfile
import tempfile
import urllib.parse
import urllib.request
from pathlib import Path

try:
    import zstandard
except ImportError:
    print("ERROR: zstandard is required; install with `pip install zstandard==0.22.0`")
    sys.exit(1)

# Setup some file paths
THIS_DIR = Path(__file__).parent
ROOT_DIR = THIS_DIR.parent.parent
BIN_DIR = ROOT_DIR / "bin"
INSTALL_DIR = BIN_DIR / "versions"
VERSIONS_FILE = ROOT_DIR / ".python-versions"
VERSIONS_METADATA_FILE = THIS_DIR / "versions.json"

# Map system information to those in the versions metadata
ARCH_MAP = {"aarch64": "arm64", "amd64": "x86_64"}
PLATFORM_MAP = {"win32": "windows"}
PLATFORM = sys.platform
ARCH = platform.machine().lower()
INTERPRETER = "cpython"


def decompress_file(archive_path: Path, output_path: Path):
    if str(archive_path).endswith(".tar.zst"):
        dctx = zstandard.ZstdDecompressor()

        with tempfile.TemporaryFile(suffix=".tar") as ofh:
            with archive_path.open("rb") as ifh:
                dctx.copy_stream(ifh, ofh)
            ofh.seek(0)
            with tarfile.open(fileobj=ofh) as z:
                z.extractall(output_path)
    else:
        raise ValueError(f"Unknown archive type {archive_path.suffix}")


def sha256_file(path: Path):
    h = hashlib.sha256()

    with open(path, "rb") as file:
        while True:
            # Reading is buffered, so we can read smaller chunks.
            chunk = file.read(h.block_size)
            if not chunk:
                break
            h.update(chunk)

    return h.hexdigest()


versions_metadata = json.loads(VERSIONS_METADATA_FILE.read_text())
versions = VERSIONS_FILE.read_text().splitlines()


# Install each version
for version in versions:
    key = f"{INTERPRETER}-{version}-{PLATFORM_MAP.get(PLATFORM, PLATFORM)}-{ARCH_MAP.get(ARCH, ARCH)}"
    print(f"Installing {key}")

    url = versions_metadata[key]["url"]

    if not url:
        print(f"No matching download for {key}")
        sys.exit(1)

    filename = url.split("/")[-1]
    print(f"Downloading {urllib.parse.unquote(filename)}")
    download_path = THIS_DIR / filename
    with urllib.request.urlopen(url) as response:
        with download_path.open("wb") as download_file:
            shutil.copyfileobj(response, download_file)

    sha = versions_metadata[key]["sha256"]
    if not sha:
        print(f"WARNING: no checksum for {key}")
    else:
        print("Verifying checksum...", end="")
        if sha256_file(download_path) != sha:
            print(" FAILED!")
            sys.exit(1)
        print(" OK")

    install_dir = INSTALL_DIR / f"{INTERPRETER}@{version}"
    if install_dir.exists():
        shutil.rmtree(install_dir)
    print("Extracting to", install_dir)
    install_dir.parent.mkdir(parents=True, exist_ok=True)

    decompress_file(THIS_DIR / filename, install_dir.with_suffix(".tmp"))

    # Setup the installation
    (install_dir.with_suffix(".tmp") / "python").rename(install_dir)

    # Use relative paths for links so if the bin is moved they don't break
    executable = "." / install_dir.relative_to(BIN_DIR) / "install" / "bin" / "python3"
    if PLATFORM == "win32":
        executable = executable.with_suffix(".exe")

    major = versions_metadata[key]["major"]
    minor = versions_metadata[key]["minor"]

    # Link as all version tuples, later versions in the file will take precedence
    BIN_DIR.mkdir(parents=True, exist_ok=True)
    targets = (
        (BIN_DIR / f"python{version}"),
        (BIN_DIR / f"python{major}.{minor}"),
        (BIN_DIR / f"python{major}"),
        (BIN_DIR / "python"),
    )
    for target in targets:
        if PLATFORM == "win32":
            target = target.with_suffix(".exe")

        target.unlink(missing_ok=True)
        target.symlink_to(executable)

    print(f"Installed as python{version}")

    # Cleanup
    install_dir.with_suffix(".tmp").rmdir()
    (THIS_DIR / filename).unlink()

print("Done!")
