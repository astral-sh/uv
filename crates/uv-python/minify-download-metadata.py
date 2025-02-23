# /// script
# requires-python = ">=3.12"
# ///
"""
Generate minified Python version download metadata json to embed in the binary.

Generates the `download-metadata-minified.json` file from the `download-metadata.json` file.

Usage:

    uv run -- crates/uv-python/minify-download-metadata.py
"""

import json
from pathlib import Path

CRATE_ROOT = Path(__file__).parent
VERSION_METADATA = CRATE_ROOT / "download-metadata.json"
TARGET = CRATE_ROOT / "src" / "download-metadata-minified.json"


def main() -> None:
    json_data = json.loads(Path(VERSION_METADATA).read_text())
    json_string = json.dumps(json_data, separators=(",", ":"))
    TARGET.write_text(json_string)


if __name__ == "__main__":
    main()
