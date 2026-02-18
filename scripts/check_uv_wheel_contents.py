"""Check that uv and uv_build wheels contain exactly the expected files"""

import re
import sys
from argparse import ArgumentParser
from pathlib import Path
from zipfile import ZipFile

# Update these when changing the wheel contents
uv_expected = {
    "uv-VERSION.data/scripts/uv",
    "uv-VERSION.data/scripts/uvx",
    "uv-VERSION.dist-info/METADATA",
    "uv-VERSION.dist-info/RECORD",
    "uv-VERSION.dist-info/WHEEL",
    "uv-VERSION.dist-info/licenses/LICENSE-APACHE",
    "uv-VERSION.dist-info/licenses/LICENSE-MIT",
    "uv/__init__.py",
    "uv/__main__.py",
    "uv/_find_uv.py",
    "uv/py.typed",
}
uv_build_expected = {
    "uv_build-VERSION.data/scripts/uv-build",
    "uv_build-VERSION.dist-info/METADATA",
    "uv_build-VERSION.dist-info/RECORD",
    "uv_build-VERSION.dist-info/WHEEL",
    "uv_build-VERSION.dist-info/licenses/LICENSE-APACHE",
    "uv_build-VERSION.dist-info/licenses/LICENSE-MIT",
    "uv_build/__init__.py",
    "uv_build/__main__.py",
    "uv_build/py.typed",
}


def check_uv_wheel(uv_wheel: Path) -> None:
    if uv_wheel.name.startswith("uv-"):
        expected = uv_expected
        # Windows wheels contain uvw, the windowed launcher.
        if "-win" in uv_wheel.name:
            expected = expected | {"uv-VERSION.data/scripts/uvw"}
    elif uv_wheel.name.startswith("uv_build-"):
        expected = uv_build_expected
    else:
        raise RuntimeError(f"Unknown wheel filename: {uv_wheel.name}")

    with ZipFile(uv_wheel) as wheel:
        files = wheel.namelist()
    # Escape the version and remove the Windows exe extension.
    actual = {
        re.sub(r"^([a-z_0-9]*)-([0-9]+\.)*[0-9]+", r"\1-VERSION", file).replace(
            ".exe", ""
        )
        for file in files
    }
    if expected != actual:
        # Verbose log
        print(f"Expected: {sorted(expected)}", file=sys.stderr)
        print(f"Actual:   {sorted(actual)}", file=sys.stderr)
        print("", file=sys.stderr)
        # Concise error
        print("error: uv wheel has unexpected contents", file=sys.stderr)
        if expected - actual:
            print(f"  Missing wheel entries: {expected - actual}", file=sys.stderr)
        if actual - expected:
            print(f"  Unexpected wheel entries: {actual - expected}", file=sys.stderr)
        sys.exit(1)


def main():
    parser = ArgumentParser()
    parser.add_argument("wheels", type=Path, nargs="+")
    args = parser.parse_args()

    for uv_wheel in args.wheels:
        if uv_wheel.name.endswith(".tar.gz"):
            continue
        print(f"Checking {uv_wheel}")
        check_uv_wheel(uv_wheel)


if __name__ == "__main__":
    main()
