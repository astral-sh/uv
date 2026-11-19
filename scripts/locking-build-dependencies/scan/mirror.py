"""Stage a pinned inventory of PyPI sdists; do not open archives on the host."""

import argparse
import csv
import hashlib
import json
import re
import threading
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
from pathlib import Path
from urllib.parse import quote

import requests
from requests.adapters import HTTPAdapter
from urllib3.util.retry import Retry

MAX_ARCHIVE = 4 * 1024 * 1024
LOCAL = threading.local()


def client() -> requests.Session:
    if not hasattr(LOCAL, "session"):
        LOCAL.session = requests.Session()
        LOCAL.session.mount(
            "https://",
            HTTPAdapter(
                max_retries=Retry(
                    total=2,
                    backoff_factor=1,
                    status_forcelist=(429, 500, 502, 503, 504),
                )
            ),
        )
    return LOCAL.session


def mirror(name, version, cutoff, files):
    row = {"name": name, "version": version}
    if not version:
        return row | {"status": "fetch-error", "detail": "No version in pinned input"}
    try:
        url = f"https://pypi.org/pypi/{quote(name, safe='')}/{quote(version, safe='')}/json"
        response = client().get(url, timeout=15)
        response.raise_for_status()
        candidates = [f for f in response.json()["urls"] if f["packagetype"] == "sdist"]
        eligible = [
            f
            for f in candidates
            if not f["yanked"]
            and datetime.fromisoformat(f["upload_time_iso_8601"]) < cutoff
        ]
        if not eligible:
            return row | {"status": "no-sdist" if not candidates else "excluded"}
        artifact = min(eligible, key=lambda f: (f["size"], f["filename"]))
        url = artifact["url"]
        digest = artifact["digests"]["sha256"]
        if not url.startswith(
            "https://files.pythonhosted.org/packages/"
        ) or not re.fullmatch("[0-9a-f]{64}", digest):
            raise ValueError("unexpected artifact location or digest")
        if artifact["size"] > MAX_ARCHIVE:
            return row | {"status": "size-limit"}
        path = files / digest
        if not path.exists():
            with client().get(
                url, timeout=25, stream=True, allow_redirects=False
            ) as archive:
                archive.raise_for_status()
                data = archive.raw.read(MAX_ARCHIVE + 1)
            if (
                hashlib.sha256(data).hexdigest() != digest
                or len(data) != artifact["size"]
            ):
                raise ValueError("sdist size or hash mismatch")
            path.write_bytes(data)
        return row | {"status": "staged", "sha256": digest}
    except (requests.RequestException, OSError, ValueError, KeyError) as exc:
        return row | {"status": "fetch-error", "detail": str(exc)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("versions", type=Path)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--exclude-newer", required=True, type=datetime.fromisoformat)
    parser.add_argument("--workers", default=12, type=int)
    args = parser.parse_args()
    cutoff = (
        args.exclude_newer.replace(tzinfo=timezone.utc)
        if args.exclude_newer.tzinfo is None
        else args.exclude_newer
    )
    files = args.directory / "files"
    files.mkdir(exist_ok=True, parents=True)
    with args.versions.open() as stream:
        packages = list(csv.DictReader(stream))
    with (
        ThreadPoolExecutor(args.workers) as pool,
        (args.directory / "manifest.jsonl").open("w", buffering=1) as output,
    ):

        def fetch(package):
            return mirror(
                package["package_name"], package["latest_version"], cutoff, files
            )

        for row in pool.map(fetch, packages):
            print(json.dumps(row), file=output)


if __name__ == "__main__":
    main()
