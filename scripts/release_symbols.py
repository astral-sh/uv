"""Package and verify separate symbols for the executables in uv release wheels.

Requires Python 3.11 and Rust's llvm-tools-preview component. macOS also uses
Xcode's strip/dwarfdump and codesign. The executable bytes placed in the wheel
and standalone archive are identical before release signing.
"""

import argparse
import base64
import csv
import hashlib
import io
import json
import math
import os
import re
import shutil
import struct
import subprocess
import tarfile
import tempfile
import tomllib
import uuid
import zipfile
import zlib
from pathlib import Path


def run(*arguments: str | Path) -> str:
    return subprocess.check_output(list(map(str, arguments)), text=True)


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def llvm_tool(name: str) -> Path:
    host = next(
        line.removeprefix("host: ")
        for line in run("rustc", "-vV").splitlines()
        if line.startswith("host: ")
    )
    suffix = ".exe" if os.name == "nt" else ""
    tool = (
        Path(run("rustc", "--print", "sysroot").strip())
        / "lib/rustlib"
        / host
        / "bin"
        / (name + suffix)
    )
    if not tool.is_file():
        raise RuntimeError(f"Missing {tool}; install llvm-tools-preview")
    return tool


def system_for_target(target: str) -> str:
    if target.endswith("-windows-msvc"):
        return "windows"
    if target.endswith("-apple-darwin"):
        return "macos"
    if "-linux-" in target:
        return "linux"
    raise ValueError(f"Unsupported release target: {target}")


def executable_names(target: str) -> tuple[str, ...]:
    return (
        ("uv.exe", "uvx.exe", "uvw.exe")
        if system_for_target(target) == "windows"
        else ("uv", "uvx")
    )


def archive_name(target: str) -> str:
    extension = ".zip" if system_for_target(target) == "windows" else ".tar.gz"
    return f"uv-{target}-debug{extension}"


def pdb_identity(path: Path) -> dict:
    """Read the GUID/age from stream 1 of an MSF 7 PDB, without loading it all."""
    with path.open("rb") as stream:
        header = stream.read(56)
        if header[:32] != b"Microsoft C/C++ MSF 7.00\r\n\x1aDS\0\0\0":
            raise ValueError(f"Unsupported PDB format: {path}")
        block_size, _, block_count, directory_size, _, block_map = struct.unpack_from(
            "<6I", header, 32
        )
        if (
            block_size not in (512, 1024, 2048, 4096)
            or block_count * block_size > path.stat().st_size
        ):
            raise ValueError(f"Invalid PDB blocks: {path}")
        stream.seek(block_map * block_size)
        directory_blocks = struct.unpack(
            f"<{math.ceil(directory_size / block_size)}I",
            stream.read(4 * math.ceil(directory_size / block_size)),
        )
        directory = bytearray()
        for block in directory_blocks:
            stream.seek(block * block_size)
            directory.extend(stream.read(block_size))
        (count,) = struct.unpack_from("<I", directory)
        if count < 2:
            raise ValueError(f"PDB has no information stream: {path}")
        sizes = struct.unpack_from(f"<{count}I", directory, 4)
        first_blocks = 0 if sizes[0] == 0xFFFFFFFF else math.ceil(sizes[0] / block_size)
        (info_block,) = struct.unpack_from(
            "<I", directory, 4 + count * 4 + first_blocks * 4
        )
        if sizes[1] < 28 or info_block >= block_count:
            raise ValueError(f"Invalid PDB information stream: {path}")
        stream.seek(info_block * block_size)
        info = stream.read(28)
    return {
        "guid": str(uuid.UUID(bytes_le=info[12:28])).upper(),
        "age": struct.unpack_from("<I", info, 8)[0],
    }


def binary_identity(binary: Path, system: str) -> dict:
    if system == "windows":
        output = run(llvm_tool("llvm-readobj"), "--coff-debug-directory", binary)
        identifiers = re.findall(r"PDBGUID: \{([\w-]+)\}\s+PDBAge: (\d+)", output)
        if len(identifiers) != 1:
            raise ValueError(f"Expected one PDB identity: {binary}")
        guid, age = identifiers[0]
        return {"guid": guid.upper(), "age": int(age)}
    if system == "macos":
        output = run("xcrun", "dwarfdump", "--uuid", binary)
        identifiers = re.findall(r"UUID: ([\w-]+) \(([^)]+)\)", output)
        if not identifiers:
            raise ValueError(f"Missing Mach-O UUID: {binary}")
        return {
            "uuids": sorted(
                [guid.upper(), architecture] for guid, architecture in identifiers
            )
        }
    output = run(llvm_tool("llvm-readobj"), "--notes", binary)
    identifiers = re.findall(r"Build ID: ([a-fA-F0-9]+)", output)
    if len(identifiers) != 1:
        raise ValueError(f"Expected one ELF build ID: {binary}")
    return {"build_id": identifiers[0].lower()}


def sbom_digest(binary: Path, system: str) -> str:
    """Require cargo-auditable metadata to survive symbol extraction/stripping."""
    output = run(llvm_tool("llvm-readobj"), "--sections", binary)
    for section in output.split("Section {")[1:]:
        if not re.search(r"Name: \.dep-v0(?:\s|$)", section):
            continue
        offset_field, size_field = (
            ("PointerToRawData", "RawDataSize")
            if system == "windows"
            else ("Offset", "Size")
        )
        (offset,) = re.findall(rf"\b{offset_field}: (0x[0-9A-Fa-f]+|\d+)", section)
        (size,) = re.findall(rf"\b{size_field}: (0x[0-9A-Fa-f]+|\d+)", section)
        with binary.open("rb") as stream:
            stream.seek(int(offset, 0))
            data = stream.read(int(size, 0))
        json.loads(zlib.decompress(data))
        return hashlib.sha256(data).hexdigest()
    raise ValueError(f"Missing embedded SBOM: {binary}")


def separate(binary: Path, built: Path, symbols: Path, system: str) -> Path:
    if system == "linux":
        companion = symbols / (binary.name + ".debug")
        sections = run(llvm_tool("llvm-readobj"), "--sections", binary)
        if not re.search(r"Name: \.debug_info\b", sections):
            raise ValueError(f"Executable has no full DWARF: {binary}")
        run(llvm_tool("llvm-objcopy"), "--only-keep-debug", binary, companion)
        # GNU stripping retains cargo-auditable's non-allocated SBOM section.
        run(llvm_tool("llvm-strip"), "--strip-all-gnu", binary)
        run(llvm_tool("llvm-objcopy"), f"--add-gnu-debuglink={companion}", binary)
    elif system == "macos":
        companion = symbols / (binary.name + ".dSYM")
        shutil.copytree(built.with_suffix(".dSYM"), companion)
        run("strip", "-S", "-x", binary)
        # Stripping invalidates the linker signature. Ad-hoc signing permits
        # native smoke tests before the release's Developer ID signing job.
        run("codesign", "--force", "--sign", "-", binary)
        run("codesign", "--verify", "--strict", binary)
    else:
        companion = symbols / (binary.stem + ".pdb")
        shutil.copyfile(built.with_suffix(".pdb"), companion)
    expected = binary_identity(binary, system)
    actual = (
        pdb_identity(companion)
        if system == "windows"
        else binary_identity(companion, system)
    )
    if actual != expected:
        raise ValueError(f"Executable/companion identities differ: {binary}")
    return companion


def repack_wheel(
    source: Path, destination: Path, replacements: dict[str, Path]
) -> None:
    """Replace exact executable members, retaining metadata and rewriting RECORD."""
    with (
        zipfile.ZipFile(source) as original,
        zipfile.ZipFile(destination, "w") as wheel,
    ):
        entries = original.infolist()
        if len({entry.filename for entry in entries}) != len(entries):
            raise ValueError("Duplicate wheel members")
        records = [
            entry for entry in entries if entry.filename.endswith(".dist-info/RECORD")
        ]
        if len(records) != 1 or not replacements.keys() <= {
            entry.filename for entry in entries
        }:
            raise ValueError(
                "Wheel has missing executables or an invalid RECORD inventory"
            )
        record_rows = []
        for entry in entries:
            if entry == records[0]:
                continue
            if entry.filename.endswith(("/RECORD.jws", "/RECORD.p7s")):
                raise ValueError("Cannot rewrite a signed wheel")
            checksum = hashlib.sha256()
            size = 0
            with (
                (
                    replacements[entry.filename].open("rb")
                    if entry.filename in replacements
                    else original.open(entry)
                ) as source_stream,
                wheel.open(entry, "w", force_zip64=True) as destination_stream,
            ):
                while chunk := source_stream.read(1024 * 1024):
                    checksum.update(chunk)
                    size += len(chunk)
                    destination_stream.write(chunk)
            encoded = base64.urlsafe_b64encode(checksum.digest()).rstrip(b"=").decode()
            record_rows.append((entry.filename, f"sha256={encoded}", size))
        buffer = io.StringIO(newline="")
        writer = csv.writer(buffer, lineterminator="\n")
        writer.writerows(record_rows)
        writer.writerow((records[0].filename, "", ""))
        wheel.writestr(records[0], buffer.getvalue())


def package(target: str, wheels: Path, binaries: Path, output: Path) -> None:
    system = system_for_target(target)
    matches = list(wheels.glob("uv-*.whl"))
    if len(matches) != 1:
        raise ValueError(f"Expected one uv wheel in {wheels}")
    (wheel,) = matches
    output.mkdir(parents=True, exist_ok=False)
    with tempfile.TemporaryDirectory(prefix="uv-release-symbols-") as temporary:
        temporary = Path(temporary).resolve()
        symbols = temporary / f"uv-{target}-debug"
        symbols.mkdir()
        replacements = {}
        manifest = {
            "schema_version": 1,
            "target": target,
            "source_commit": os.environ.get("GITHUB_SHA"),
            "debug_level": "full",
            "executables": {},
        }
        with zipfile.ZipFile(wheel) as archive:
            for name in executable_names(target):
                members = [
                    entry
                    for entry in archive.infolist()
                    if entry.filename.endswith(f".data/scripts/{name}")
                ]
                if len(members) != 1:
                    raise ValueError(f"Expected one wheel executable {name}")
                (member,) = members
                binary = temporary / name
                with archive.open(member) as source, binary.open("wb") as destination:
                    shutil.copyfileobj(source, destination)
                binary.chmod(0o755)
                before = sbom_digest(binary, system)
                companion = separate(binary, binaries / name, symbols, system)
                if sbom_digest(binary, system) != before:
                    raise ValueError(f"Stripping changed the embedded SBOM: {name}")
                files = (
                    sorted(companion.rglob("*")) if companion.is_dir() else [companion]
                )
                manifest["executables"][name] = {
                    "identity": binary_identity(binary, system),
                    "unsigned_executable_sha256": digest(binary),
                    "sbom_sha256": before,
                    "companion": companion.name,
                    "files": {
                        path.relative_to(symbols).as_posix(): {
                            "sha256": digest(path),
                            "bytes": path.stat().st_size,
                        }
                        for path in files
                        if path.is_file()
                    },
                }
                replacements[member.filename] = binary
        # The wheel and standalone archives must contain the same stripped bytes.
        rewritten = temporary / wheel.name
        repack_wheel(wheel, rewritten, replacements)
        for binary in replacements.values():
            shutil.copyfile(binary, binaries / binary.name)
        shutil.copyfile(rewritten, wheel)
        (symbols / "symbols.json").write_text(
            json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
        )
        destination = output / archive_name(target)
        if system == "windows":
            with zipfile.ZipFile(
                destination, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6
            ) as archive:
                for path in sorted(symbols.rglob("*")):
                    if path.is_file():
                        archive.write(path, path.relative_to(temporary).as_posix())
        else:
            with tarfile.open(destination, "w:gz", compresslevel=6) as archive:
                archive.add(symbols, arcname=symbols.name)
    destination.with_name(destination.name + ".sha256").write_text(
        f"{digest(destination)}  {destination.name}\n", encoding="utf-8"
    )
    print(f"Packaged matching full debug symbols: {destination}")


def inventory(directory: Path, targets: list[str]) -> list[Path]:
    expected = {
        name
        for target in targets
        for name in (archive_name(target), archive_name(target) + ".sha256")
    }
    actual = {path.name for path in directory.iterdir()}
    if actual != expected:
        raise ValueError(
            f"Symbol inventory differs: missing={sorted(expected - actual)}, extra={sorted(actual - expected)}"
        )
    archives = []
    for target in targets:
        archive = directory / archive_name(target)
        sidecar = archive.with_name(archive.name + ".sha256")
        if sidecar.read_text().split() != [digest(archive), archive.name]:
            raise ValueError(f"Symbol archive checksum mismatch: {archive}")
        archives.append(archive)
    return archives


def patch_manifest(path: Path, directory: Path) -> None:
    manifest = json.loads(path.read_text())
    releases = [
        release for release in manifest["releases"] if release["app_name"] == "uv"
    ]
    if len(releases) != 1:
        raise ValueError("Expected one uv release")
    (release,) = releases
    executable_artifacts = [
        manifest["artifacts"][name]
        for name in release["artifacts"]
        if manifest["artifacts"][name]["kind"] == "executable-zip"
    ]
    targets = [artifact["target_triples"][0] for artifact in executable_artifacts]
    if not targets or len(targets) != len(set(targets)):
        raise ValueError("Missing or duplicate release targets")
    for artifact, archive in zip(
        executable_artifacts, inventory(directory, targets), strict=True
    ):
        checksum = archive.name + ".sha256"
        if archive.name in manifest["artifacts"] or checksum in manifest["artifacts"]:
            raise ValueError(f"Duplicate symbol artifact: {archive.name}")
        manifest["artifacts"][archive.name] = {
            "name": archive.name,
            "kind": "symbols",
            "target_triples": artifact["target_triples"],
            "checksum": checksum,
            "checksums": {"sha256": digest(archive)},
        }
        manifest["artifacts"][checksum] = {"name": checksum, "kind": "checksum"}
        release["artifacts"].extend([archive.name, checksum])
        for asset in artifact.get("assets", []):
            if asset.get("kind") == "executable":
                asset["symbols_artifact"] = archive.name
    path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


def verify(target: str, archive: Path, binaries: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="uv-verify-symbols-") as temporary:
        temporary = Path(temporary).resolve()
        if archive.suffix == ".zip":
            with zipfile.ZipFile(archive) as bundle:
                if any(
                    not (temporary / name).resolve().is_relative_to(temporary)
                    for name in bundle.namelist()
                ):
                    raise ValueError("Unsafe symbol archive paths")
                bundle.extractall(temporary)
        else:
            with tarfile.open(archive) as bundle:
                bundle.extractall(temporary, filter="data")
        root = temporary / f"uv-{target}-debug"
        manifest = json.loads((root / "symbols.json").read_text())
        if manifest["target"] != target or set(manifest["executables"]) != set(
            executable_names(target)
        ):
            raise ValueError("Symbol manifest target/executables differ")
        system = system_for_target(target)
        for name, record in manifest["executables"].items():
            for relative, expected in record["files"].items():
                path = (root / relative).resolve()
                if (
                    not path.is_relative_to(root)
                    or digest(path) != expected["sha256"]
                    or path.stat().st_size != expected["bytes"]
                ):
                    raise ValueError(f"Symbol file hash/size mismatch: {relative}")
            companion = root / record["companion"]
            identity = (
                pdb_identity(companion)
                if system == "windows"
                else binary_identity(companion, system)
            )
            if (
                identity != record["identity"]
                or binary_identity(binaries / name, system) != identity
            ):
                raise ValueError(
                    f"Release executable no longer matches symbols: {name}"
                )
            if sbom_digest(binaries / name, system) != record["sbom_sha256"]:
                raise ValueError(f"Release executable SBOM changed: {name}")
    print(f"Verified release symbols: {target}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    packaging = commands.add_parser("package")
    packaging.add_argument("--target", required=True)
    packaging.add_argument("--wheels", type=Path, default=Path("dist"))
    packaging.add_argument("--binaries", type=Path, required=True)
    packaging.add_argument("--output", type=Path, default=Path("debug-symbols"))
    collection = commands.add_parser("collect")
    collection.add_argument("directory", type=Path)
    collection.add_argument("--config", type=Path, default=Path("dist-workspace.toml"))
    patching = commands.add_parser("manifest")
    patching.add_argument("manifest", type=Path)
    patching.add_argument("directory", type=Path)
    verification = commands.add_parser("verify")
    verification.add_argument("--target", required=True)
    verification.add_argument("--archive", type=Path, required=True)
    verification.add_argument("--binaries", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "package":
        package(args.target, args.wheels, args.binaries, args.output)
    elif args.command == "collect":
        inventory(
            args.directory, tomllib.loads(args.config.read_text())["dist"]["targets"]
        )
    elif args.command == "manifest":
        patch_manifest(args.manifest, args.directory)
    elif args.command == "verify":
        verify(args.target, args.archive, args.binaries)


if __name__ == "__main__":
    main()
