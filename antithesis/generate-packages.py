#!/usr/bin/env python3

import base64
import csv
import hashlib
import io
import sys
import zipfile
from pathlib import Path

PACKAGES = {
    "antithesis-left": {
        "1.0.0": [],
        "2.0.0": [],
    },
    "antithesis-right": {
        "1.0.0": ["antithesis-left==1.0.0"],
        "2.0.0": ["antithesis-left==1.0.0"],
    },
    "antithesis-root": {
        "1.0.0": ["antithesis-left==1.0.0", "antithesis-right==1.0.0"],
        "2.0.0": ["antithesis-left==2.0.0", "antithesis-right==2.0.0"],
    },
}
PAYLOAD_SIZE = 2 * 1024 * 1024
INSTALL_FILE_COUNT = 10_000


def record_hash(contents: bytes) -> str:
    digest = hashlib.sha256(contents).digest()
    encoded = base64.urlsafe_b64encode(digest).rstrip(b"=").decode()
    return f"sha256={encoded}"


def write_file(
    archive: zipfile.ZipFile,
    path: str,
    contents: bytes,
    compression: int = zipfile.ZIP_DEFLATED,
) -> None:
    info = zipfile.ZipInfo(path, date_time=(1980, 1, 1, 0, 0, 0))
    info.compress_type = compression
    info.external_attr = 0o644 << 16
    archive.writestr(info, contents)


def build_wheel(
    output_directory: Path,
    name: str,
    version: str,
    dependencies: list[str],
) -> Path:
    wheel_name = name.replace("-", "_")
    distribution = f"{wheel_name}-{version}"
    module = wheel_name
    filename = f"{distribution}-py3-none-any.whl"
    wheel_path = output_directory / filename

    metadata_lines = [
        "Metadata-Version: 2.1",
        f"Name: {name}",
        f"Version: {version}",
    ]
    metadata_lines.extend(f"Requires-Dist: {dependency}" for dependency in dependencies)

    files = {
        f"{module}/__init__.py": f'__version__ = "{version}"\n'.encode(),
        f"{module}/payload.bin": (
            hashlib.sha256(f"{name}=={version}".encode()).digest()
            * (PAYLOAD_SIZE // hashlib.sha256().digest_size)
        ),
        f"{distribution}.dist-info/METADATA": (
            "\n".join(metadata_lines) + "\n"
        ).encode(),
        f"{distribution}.dist-info/WHEEL": (
            "Wheel-Version: 1.0\n"
            "Generator: uv-antithesis\n"
            "Root-Is-Purelib: true\n"
            "Tag: py3-none-any\n"
        ).encode(),
    }
    if name == "antithesis-root" and version == "1.0.0":
        files.update(
            {
                f"{module}/generated/module_{index:05d}.py": (
                    f"VALUE = {index}\n".encode()
                )
                for index in range(INSTALL_FILE_COUNT)
            }
        )

    record_path = f"{distribution}.dist-info/RECORD"
    record_buffer = io.StringIO()
    writer = csv.writer(record_buffer, lineterminator="\n")
    for path, contents in files.items():
        writer.writerow((path, record_hash(contents), len(contents)))
    writer.writerow((record_path, "", ""))
    files[record_path] = record_buffer.getvalue().encode()

    with zipfile.ZipFile(wheel_path, "w") as archive:
        for path, contents in files.items():
            compression = (
                zipfile.ZIP_STORED
                if path.endswith("payload.bin")
                else zipfile.ZIP_DEFLATED
            )
            write_file(archive, path, contents, compression)

    return wheel_path


def generate_index(output_directory: Path) -> None:
    packages_directory = output_directory / "packages"
    simple_directory = output_directory / "simple"
    packages_directory.mkdir(parents=True)
    simple_directory.mkdir(parents=True)

    project_links = []
    for name, versions in PACKAGES.items():
        project_directory = simple_directory / name
        project_directory.mkdir()
        links = []
        for version, dependencies in versions.items():
            wheel_path = build_wheel(packages_directory, name, version, dependencies)
            digest = hashlib.sha256(wheel_path.read_bytes()).hexdigest()
            links.append(
                f'<a href="../../packages/{wheel_path.name}#sha256={digest}">'
                f"{wheel_path.name}</a>"
            )
        (project_directory / "index.html").write_text("\n".join(links) + "\n")
        project_links.append(f'<a href="{name}/">{name}</a>')

    (simple_directory / "index.html").write_text("\n".join(project_links) + "\n")


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: generate-packages.py OUTPUT_DIRECTORY")
    generate_index(Path(sys.argv[1]))


if __name__ == "__main__":
    main()
