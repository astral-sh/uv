"""Generate non-Apple Linux CPU profiles for aarch64-variant-provider 0.2.0.

The provider vendors archspec at ebd3eb502d9205f1033ffc028d661f2fbece7dfc, which pins
the archspec-json revision below.
"""

import hashlib
import json
from pathlib import Path
from urllib.request import urlopen

ARCHSPEC = "archspec/archspec-json/90b453f4053b6b139d1d2cb78c463b02e71ef363"
ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "crates/uv-distribution/src/variant/aarch64"


def fetch(path: str, sha256: str) -> str:
    with urlopen(f"https://raw.githubusercontent.com/{path}", timeout=30) as response:
        data = response.read()
    if hashlib.sha256(data).hexdigest() != sha256:
        raise ValueError(f"SHA-256 mismatch for {path}")
    return data.decode()


def main() -> None:
    data = fetch(
        f"{ARCHSPEC}/cpu/microarchitectures.json",
        "17d1d03070a23df60206b65151d104e09d1fe6e9af07f95e5f6c760d659e52ba",
    )
    (OUTPUT / "LICENSE-archspec").write_text(
        fetch(
            f"{ARCHSPEC}/LICENSE-MIT",
            "9001ad99a662075a0e5a6a9a539fc75040d8063c3f7251d6839ca40b5234054e",
        )
    )
    database = json.loads(data)
    vendors = {
        name: implementer
        for implementer, name in database["conversions"]["arm_vendors"].items()
    }
    versions = [
        "armv9.0a",
        *(f"armv8.{minor}a" for minor in range(6, 0, -1)),
        "aarch64",
    ]
    ancestors = {}
    linux = {}
    for name, cpu in database["microarchitectures"].items():
        lineage = {name}.union(*(ancestors[parent] for parent in cpu["from"]))
        ancestors[name] = lineage
        if "aarch64" not in lineage or cpu["vendor"] in {"generic", "Apple"}:
            continue

        # Armv9.0-A includes Armv8.5-A; Armv9.2-A also includes Armv8.6-A.
        supported = tuple(
            "8a" if version == "aarch64" else version.removeprefix("armv")
            for version in versions
            if version in lineage
        )
        vendor = vendors[cpu["vendor"]]
        if supported != ("8a",):
            linux.setdefault((vendor, supported), []).append(cpu["cpupart"])

    output = """\
//! Generated with `python3 scripts/generate-aarch64-variant-profiles.py`.
//! Architecture ancestry from archspec-json, licensed under the MIT license (LICENSE-archspec).

/// Architecture versions for a Linux CPU implementer and part, in preference order.
pub(super) fn linux_versions(implementer: &str, part: &str) -> Option<&'static [&'static str]> {
    Some(match (implementer, part) {
"""
    for (vendor, supported), parts in linux.items():
        pattern = " | ".join(json.dumps(part) for part in sorted(parts))
        output += f'        ("{vendor}", {pattern}) => &{json.dumps(supported)},\n'
    output += "        _ => return None,\n    })\n}\n"
    (OUTPUT / "profiles.rs").write_text(output)


if __name__ == "__main__":
    main()
