"""Run with python -m unittest discover -s scripts/tests.

UV_TEST_RELEASE_SYMBOLS=1 also builds the Rust+C wheel with cargo-auditable and
Maturin, then checks packaging, symbol identity, SBOM retention and installation.
"""

import base64
import csv
import hashlib
import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import release_symbols

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = Path(__file__).with_name("fixtures") / "release-symbols"


class ReleaseSymbols(unittest.TestCase):
    def test_repacked_wheel_retains_metadata_and_has_valid_records(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            original = directory / "original.whl"
            rewritten = directory / "rewritten.whl"
            replacement = directory / "uv"
            replacement.write_bytes(b"stripped executable")
            member = zipfile.ZipInfo("uv-0.0.0.data/scripts/uv")
            member.external_attr = 0o100755 << 16
            member.compress_type = zipfile.ZIP_DEFLATED
            with zipfile.ZipFile(original, "w") as wheel:
                wheel.writestr(member, b"unstripped executable")
                wheel.writestr(
                    "uv-0.0.0.dist-info/sboms/uv.cyclonedx.json",
                    b'{"bomFormat":"CycloneDX"}',
                )
                wheel.writestr("uv-0.0.0.dist-info/RECORD", b"old record")
            release_symbols.repack_wheel(
                original, rewritten, {member.filename: replacement}
            )
            with zipfile.ZipFile(rewritten) as wheel:
                self.assertEqual(wheel.read(member.filename), replacement.read_bytes())
                self.assertEqual(
                    wheel.getinfo(member.filename).external_attr, member.external_attr
                )
                self.assertEqual(
                    wheel.read("uv-0.0.0.dist-info/sboms/uv.cyclonedx.json"),
                    b'{"bomFormat":"CycloneDX"}',
                )
                self.assertEqual(wheel.testzip(), None)
                rows = list(
                    csv.reader(
                        io.StringIO(wheel.read("uv-0.0.0.dist-info/RECORD").decode())
                    )
                )
                for name, checksum, size in rows[:-1]:
                    data = wheel.read(name)
                    self.assertEqual(
                        checksum,
                        "sha256="
                        + base64.urlsafe_b64encode(hashlib.sha256(data).digest())
                        .rstrip(b"=")
                        .decode(),
                    )
                    self.assertEqual(int(size), len(data))
                self.assertEqual(rows[-1], ["uv-0.0.0.dist-info/RECORD", "", ""])

    def test_missing_wheel_executable_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with zipfile.ZipFile(root / "input.whl", "w") as wheel:
                wheel.writestr("uv-0.0.0.dist-info/RECORD", b"")
            with self.assertRaisesRegex(ValueError, "missing executables"):
                release_symbols.repack_wheel(
                    root / "input.whl",
                    root / "output.whl",
                    {"uv-0.0.0.data/scripts/uv": root / "uv"},
                )

    def test_manifest_links_symbols_without_changing_installer_artifacts(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            symbols = root / "symbols"
            symbols.mkdir()
            archive = symbols / "uv-x86_64-unknown-linux-gnu-debug.tar.gz"
            archive.write_bytes(b"symbol archive")
            (symbols / (archive.name + ".sha256")).write_text(
                f"{release_symbols.digest(archive)}  {archive.name}\n"
            )
            manifest = root / "dist-manifest.json"
            executable = {
                "name": "uv-x86_64-unknown-linux-gnu.tar.gz",
                "kind": "executable-zip",
                "target_triples": ["x86_64-unknown-linux-gnu"],
                "checksums": {"sha256": "original-digest"},
                "assets": [{"name": "uv", "kind": "executable"}],
            }
            manifest.write_text(
                json.dumps(
                    {
                        "releases": [
                            {
                                "app_name": "uv",
                                "artifacts": [executable["name"], "uv-installer.sh"],
                            }
                        ],
                        "artifacts": {
                            executable["name"]: executable,
                            "uv-installer.sh": {"kind": "installer"},
                        },
                    }
                )
            )
            release_symbols.patch_manifest(manifest, symbols)
            result = json.loads(manifest.read_text())
            self.assertEqual(
                result["artifacts"][executable["name"]]["checksums"],
                {"sha256": "original-digest"},
            )
            self.assertEqual(
                result["artifacts"]["uv-installer.sh"], {"kind": "installer"}
            )
            self.assertEqual(
                result["artifacts"][executable["name"]]["assets"][0][
                    "symbols_artifact"
                ],
                archive.name,
            )
            self.assertEqual(result["artifacts"][archive.name]["kind"], "symbols")
            self.assertEqual(
                result["releases"][0]["artifacts"],
                [
                    executable["name"],
                    "uv-installer.sh",
                    archive.name,
                    archive.name + ".sha256",
                ],
            )
            with self.assertRaisesRegex(ValueError, "Duplicate symbol artifact"):
                release_symbols.patch_manifest(manifest, symbols)

    def test_missing_target_and_corrupt_archive_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.assertRaisesRegex(ValueError, "Symbol inventory differs"):
                release_symbols.inventory(root, ["x86_64-pc-windows-msvc"])
            archive = root / "uv-x86_64-pc-windows-msvc-debug.zip"
            archive.write_bytes(b"data")
            archive.with_name(archive.name + ".sha256").write_text(
                "0" * 64 + "  " + archive.name + "\n"
            )
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                release_symbols.inventory(root, ["x86_64-pc-windows-msvc"])

    @unittest.skipUnless(
        os.environ.get("UV_TEST_RELEASE_SYMBOLS") == "1",
        "native release packaging test",
    )
    def test_native_full_debug_wheel(self):
        target = next(
            line.removeprefix("host: ")
            for line in release_symbols.run("rustc", "-vV").splitlines()
            if line.startswith("host: ")
        )
        system = release_symbols.system_for_target(target)
        with tempfile.TemporaryDirectory(prefix="uv-symbols-test-") as temporary:
            directory = Path(temporary)
            wheels = directory / "wheels"
            output = directory / "symbols"
            cargo_target = directory / "target"
            environment = os.environ | {
                "CARGO": str(
                    ROOT
                    / "scripts"
                    / ("cargo.cmd" if system == "windows" else "cargo.sh")
                ),
                "CARGO_TARGET_DIR": str(cargo_target),
                "CARGO_PROFILE_RELEASE_DEBUG": "full",
                "CARGO_PROFILE_RELEASE_STRIP": "none",
                "CARGO_PROFILE_RELEASE_SPLIT_DEBUGINFO": "off"
                if system == "linux"
                else "packed",
            }
            if system == "linux":
                environment["RUSTFLAGS"] = (
                    environment.get("RUSTFLAGS", "")
                    + " -C link-arg=-Wl,--build-id=sha1"
                ).strip()
            command = [
                "maturin",
                "build",
                "--manifest-path",
                str(FIXTURE / "Cargo.toml"),
                "--release",
                "--locked",
                "--strip=false",
                "--target",
                target,
                "--out",
                str(wheels),
            ]
            if system == "windows":
                command.extend(["--features", "windows-gui-bin"])
            subprocess.run(command, env=environment, check=True, cwd=FIXTURE)
            binaries = cargo_target / target / "release"
            release_symbols.package(target, wheels, binaries, output)
            archive = output / release_symbols.archive_name(target)
            release_symbols.verify(target, archive, binaries)
            release_symbols.inventory(output, [target])
            (wheel,) = wheels.glob("*.whl")
            with zipfile.ZipFile(wheel) as packaged:
                for name in release_symbols.executable_names(target):
                    (member,) = [
                        name_in_wheel
                        for name_in_wheel in packaged.namelist()
                        if name_in_wheel.endswith(f".data/scripts/{name}")
                    ]
                    self.assertEqual(
                        hashlib.sha256(packaged.read(member)).hexdigest(),
                        release_symbols.digest(binaries / name),
                    )
                    self.assertEqual(
                        subprocess.check_output([binaries / name], text=True).strip(),
                        "59",
                    )
            environment_path = directory / "venv"
            subprocess.run(
                [sys.executable, "-m", "venv", str(environment_path)], check=True
            )
            interpreter = environment_path / (
                "Scripts/python.exe" if system == "windows" else "bin/python"
            )
            subprocess.run(
                [str(interpreter), "-m", "pip", "install", "--no-index", str(wheel)],
                check=True,
            )
            executable_directory = interpreter.parent
            for name in release_symbols.executable_names(target):
                self.assertEqual(
                    release_symbols.digest(executable_directory / name),
                    release_symbols.digest(binaries / name),
                )
            # Signing changes bytes but must retain UUID/PDB identity; verification
            # deliberately compares these identities rather than pre-sign hashes.
            if system == "macos":
                subprocess.run(
                    ["codesign", "--force", "--sign", "-", str(binaries / "uv")],
                    check=True,
                )
                release_symbols.verify(target, archive, binaries)
            # Reject a companion manifest for a different target.
            wrong = (
                "x86_64-pc-windows-msvc"
                if system != "windows"
                else "aarch64-apple-darwin"
            )
            with self.assertRaises((ValueError, FileNotFoundError)):
                release_symbols.verify(wrong, archive, binaries)


if __name__ == "__main__":
    unittest.main()
