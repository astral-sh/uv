"""Release signing fixtures; set UV_DEV_BIN to exercise the compiled wheel replacement helper."""

import base64
import contextlib
import csv
import hashlib
import io
import json
import os
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo

import release_signing as signing
from check_uv_wheel_contents import uv_build_expected, uv_expected


def wheel(path, distribution, *, signed=False, tamper=False):
    """Create a wheel with uv's platform layout and a complete RECORD."""
    names = uv_expected if distribution == "uv" else uv_build_expected
    if signing.TARGET == "x86_64-pc-windows-msvc":
        if distribution == "uv":
            names = names | {"uv-VERSION.data/scripts/uvw"}
        names = {name + ".exe" if ".data/scripts/" in name else name for name in names}
    contents = {}
    for template in sorted(names):
        name = template.replace("VERSION", "1.2.3")
        if name.endswith("/RECORD"):
            continue
        if ".data/scripts/" in name:
            contents[name] = (
                "signed " if signed else "unsigned "
            ).encode() + name.encode()
        elif name.endswith("/WHEEL"):
            contents[name] = f"Wheel-Version: 1.0\nTag: {signing.TAG}\n".encode()
        else:
            contents[name] = b"metadata"
    record = f"{distribution}-1.2.3.dist-info/RECORD"
    stream = io.StringIO()
    rows = csv.writer(stream)
    for name, data in contents.items():
        hashed = (
            base64.urlsafe_b64encode(hashlib.sha256(data).digest()).decode().rstrip("=")
        )
        rows.writerow([name, "sha256=" + hashed, len(data)])
    rows.writerow([record, "", ""])
    contents[record] = stream.getvalue().encode()
    if tamper:
        contents[f"{distribution}-1.2.3.dist-info/METADATA"] = b"tampered"
    with ZipFile(path, "w") as archive:
        for name, data in contents.items():
            info = ZipInfo(name, (2001, 2, 3, 4, 5, 6))
            if signed and name == record:
                info.date_time = (1980, 1, 1, 0, 0, 0)
            info.compress_type = ZIP_DEFLATED
            mode = 0o100755 if ".data/scripts/" in name else 0o100644
            info.external_attr = mode << 16
            archive.writestr(info, data)


class SigningBindings(unittest.TestCase):
    target = "aarch64-apple-darwin"

    def setUp(self):
        """Prepare isolated workflow inputs for this platform."""
        tag, binaries = signing.PLATFORMS[self.target]
        self.enterContext(
            patch.multiple(signing, TARGET=self.target, TAG=tag, BINARIES=binaries)
        )
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)
        self.enterContext(contextlib.chdir(self.directory))
        self.enterContext(
            patch.dict(
                os.environ,
                {
                    "GITHUB_REPOSITORY": "astral-sh/uv",
                    "GITHUB_WORKFLOW_REF": "astral-sh/uv/.github/workflows/ci.yml@refs/pull/1/merge",
                    "GITHUB_WORKFLOW_SHA": "a" * 40,
                    "GITHUB_RUN_ID": "1",
                    "GITHUB_RUN_ATTEMPT": "1",
                    "GITHUB_SHA": "b" * 40,
                    "GITHUB_OUTPUT": str(self.directory / "job-output"),
                    "RUNNER_TEMP": str(self.directory),
                },
            )
        )
        Path("wheels").mkdir()
        Path("tools").mkdir()
        for name in signing.TOOLS:
            (Path("tools") / name).write_bytes(b"tool fixture")
        for distribution in ("uv", "uv_build"):
            wheel(
                Path("wheels") / f"{distribution}-1.2.3-{signing.TAG}.whl", distribution
            )

    def prepared(self):
        """Prepare the fixture wheels and bind the resulting input manifest."""
        signing.prepare()
        os.environ["INPUT_MANIFEST_SHA256"] = signing.file_digest(Path("manifest.json"))
        return json.loads(Path("manifest.json").read_text())

    def signed(self):
        """Emulate signing by replacing executable bytes and binding their digests."""
        manifest = self.prepared()
        manifest["signed"] = {}
        manifest["certificates"] = {}
        Path("signed").mkdir()
        for item in manifest["wheels"]:
            for member, binary in item["replacements"].items():
                data = b"signed " + member.encode()
                (Path("signed") / binary).write_bytes(data)
                manifest["certificates"][binary] = hashlib.sha256(
                    binary.encode()
                ).hexdigest()
                manifest["signed"][binary] = {
                    "sha256": hashlib.sha256(data).hexdigest(),
                    "size": len(data),
                }
        signing.save(Path("signed/manifest.json"), manifest)
        os.environ["SIGNED_MANIFEST_SHA256"] = signing.file_digest(
            Path("signed/manifest.json")
        )
        return manifest

    def test_prepare_binds_source_members_and_tools(self):
        manifest = self.prepared()
        self.assertEqual(manifest["context"], signing.context())
        self.assertEqual(set(manifest["tools"]), set(signing.TOOLS))
        for item in manifest["wheels"]:
            self.assertEqual(
                item["input_sha256"],
                signing.file_digest(Path("wheels") / item["filename"]),
            )
            for member, binary in item["replacements"].items():
                self.assertEqual(
                    item["members"][member]["sha256"],
                    signing.file_digest(Path("unsigned") / binary),
                )

    def test_context_and_manifest_substitution_are_rejected(self):
        self.prepared()
        digest = os.environ["INPUT_MANIFEST_SHA256"]
        with self.assertRaisesRegex(ValueError, "digest mismatch"):
            signing.load(Path("manifest.json"), "0" * 64)
        for key in (
            "GITHUB_RUN_ATTEMPT",
            "GITHUB_SHA",
            "GITHUB_REPOSITORY",
            "GITHUB_WORKFLOW_SHA",
        ):
            with (
                patch.dict(os.environ, {key: "other"}),
                self.assertRaisesRegex(ValueError, "workflow context"),
            ):
                signing.load(Path("manifest.json"), digest)

    def test_signed_manifest_cannot_change_original_binding(self):
        manifest = self.signed()
        manifest["wheels"][0]["input_sha256"] = "0" * 64
        Path("signed/manifest.json").write_text(json.dumps(manifest))
        os.environ["SIGNED_MANIFEST_SHA256"] = signing.file_digest(
            Path("signed/manifest.json")
        )
        with self.assertRaisesRegex(ValueError, "changed the input manifest"):
            signing.signed_manifest()

    def test_signed_certificates_cover_each_binary(self):
        """Accept rotated certificates, but require a fingerprint for every binary."""
        manifest = self.signed()
        self.assertEqual(signing.signed_manifest(), manifest)
        manifest["certificates"].pop(next(iter(manifest["certificates"])))
        Path("signed/manifest.json").write_text(json.dumps(manifest))
        os.environ["SIGNED_MANIFEST_SHA256"] = signing.file_digest(
            Path("signed/manifest.json")
        )
        with self.assertRaisesRegex(ValueError, "Invalid signing certificate map"):
            signing.signed_manifest()

    def test_final_wheels_are_checked_against_signed_bytes(self):
        manifest = self.signed()
        Path("output").mkdir()
        for distribution in ("uv", "uv_build"):
            wheel(
                Path("output") / f"{distribution}-1.2.3-{signing.TAG}.whl",
                distribution,
                signed=True,
            )
        signing.verify_wheels(manifest, Path("output"))
        wheel(Path("output") / f"uv-1.2.3-{signing.TAG}.whl", "uv")
        with self.assertRaisesRegex(ValueError, "Unexpected output member"):
            signing.verify_wheels(manifest, Path("output"))

    def test_bad_untouched_record_is_rejected(self):
        path = Path("wheels") / f"uv-1.2.3-{signing.TAG}.whl"
        wheel(path, "uv", tamper=True)
        with self.assertRaisesRegex(ValueError, "Invalid RECORD row"):
            signing.prepare()

    def test_assembly_failure_does_not_emit_final_manifest(self):
        self.signed()
        with (
            patch.object(
                signing.subprocess,
                "run",
                side_effect=RuntimeError("injected assembly failure"),
            ),
            self.assertRaisesRegex(RuntimeError, "injected"),
        ):
            signing.assemble()
        self.assertFalse(Path("dist/manifest.json").exists())

    def test_archive_contains_signed_uv_binaries(self):
        """Keep the standalone archive consistent with the signed wheel members."""
        manifest = self.signed()
        Path("dist").mkdir()
        manifest["archive"] = signing.write_archive()
        signing.verify_archive(manifest)

    def test_archive_rejects_unsigned_binary(self):
        """Reject an archive accidentally built from unsigned executables."""
        manifest = self.signed()
        (Path("signed") / signing.BINARIES["uv"][0]).write_bytes(b"unsigned")
        Path("dist").mkdir()
        manifest["archive"] = signing.write_archive()
        with self.assertRaisesRegex(ValueError, "Archive binary mismatch"):
            signing.verify_archive(manifest)

    @unittest.skipUnless(
        os.environ.get("UV_DEV_BIN"), "Set UV_DEV_BIN to test wheel replacement"
    )
    def test_wheel_replacement_and_final_verification(self):
        """Exercise the actual Rust helper through preparation, assembly, and verification."""
        shutil.copy(os.environ["UV_DEV_BIN"], "tools/uv-dev")
        manifest = self.signed()
        signing.assemble()
        os.environ["FINAL_MANIFEST_SHA256"] = signing.file_digest(
            Path("dist/manifest.json")
        )
        signing.verify()
        for binary in manifest["signed"]:
            self.assertEqual(
                (Path("verified-binaries") / binary).read_bytes(),
                (Path("signed") / binary).read_bytes(),
            )


class WindowsSigningBindings(SigningBindings):
    target = "x86_64-pc-windows-msvc"

    def test_prepare_includes_windowed_launcher_and_build_backend(self):
        """Sign all four Windows wheel executables, including uvw and uv-build."""
        manifest = self.prepared()
        self.assertEqual(
            {
                binary
                for item in manifest["wheels"]
                for binary in item["replacements"].values()
            },
            {"uv.exe", "uvx.exe", "uvw.exe", "uv-build.exe"},
        )

    def test_zip_has_flat_release_layout(self):
        """Keep the Windows release ZIP flat and exclude the build backend."""
        self.signed()
        Path("dist").mkdir()
        archive = signing.write_archive()
        with ZipFile(Path("dist") / archive["filename"]) as source:
            self.assertEqual(
                sorted(source.namelist()), ["uv.exe", "uvw.exe", "uvx.exe"]
            )


if __name__ == "__main__":
    unittest.main()
