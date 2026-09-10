# /// script
# requires-python = ">=3.10"
# dependencies = []
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Check the CodSpeed profile relay and mirror import contract."""

import base64
import copy
import hashlib
import importlib.util
import json
import tempfile
import threading
import unittest
from pathlib import Path
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "codspeed_profiles", Path(__file__).with_name("codspeed-profiles.py")
)
assert SPEC is not None and SPEC.loader is not None
profiles = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(profiles)

SHA = "a" * 40
SOURCE_RUN = "123"
PROFILE = b"a recorded profile archive"
ENVIRONMENT = {
    "GITHUB_REPOSITORY": "astral-sh/uv-security",
    "GITHUB_REF": "refs/heads/main",
    "GITHUB_EVENT_NAME": "push",
    "GITHUB_SHA": SHA,
    "GITHUB_RUN_ID": "456",
    "GITHUB_JOB": "benchmarks-import",
    "GITHUB_ACTOR_ID": "1234",
    "GITHUB_ACTOR": "example",
}


def metadata(mode="walltime"):
    return {
        "version": 10,
        "repositoryProvider": "GITHUB",
        "runEnvironment": "GITHUB_ACTIONS",
        "owner": "astral-sh",
        "repository": "uv",
        "ref": "refs/heads/main",
        "event": "push",
        "commitHash": SHA,
        "tokenless": False,
        "headRef": None,
        "baseRef": None,
        "sender": {"id": "1", "login": "source"},
        "repositoryRootPath": "/home/runner/work/uv/uv/",
        "runner": {
            "name": "codspeed-runner",
            "version": "5.0.2",
            "executor": profiles.EXECUTORS[mode],
            "os": "ubuntu",
            "osVersion": "22.04",
            "arch": "aarch64",
            "cpuBrand": "source CPU",
        },
        "runPart": {"runId": SOURCE_RUN, "runPartId": "source", "jobName": "source"},
        "ghData": {"runId": SOURCE_RUN, "job": "source"},
        "profileMd5": base64.b64encode(hashlib.md5(PROFILE).digest()).decode(),
        "profileEncoding": "gzip",
    }


class ProfilesTest(unittest.TestCase):
    def test_destination_preserves_measurements(self):
        source = metadata()
        original = copy.deepcopy(source)
        imported = profiles.destination_metadata(source, ENVIRONMENT)
        self.assertEqual(source, original)
        self.assertEqual(imported["runner"], source["runner"])
        self.assertEqual(imported["commitHash"], SHA)
        self.assertEqual(imported["repositoryRootPath"], source["repositoryRootPath"])
        self.assertEqual(imported["profileMd5"], source["profileMd5"])
        self.assertEqual(imported["repository"], "uv-security")
        self.assertEqual(imported["runPart"]["runId"], "456")
        self.assertEqual(imported["ghData"]["runId"], "456")

    def test_metadata_v11(self):
        source = metadata()
        source["version"] = 11
        del source["ghData"]
        imported = profiles.destination_metadata(source, ENVIRONMENT)
        self.assertNotIn("ghData", imported)
        self.assertEqual(imported["runner"], source["runner"])

    def test_rejects_other_destinations_and_prs(self):
        for key, value in (
            ("GITHUB_REPOSITORY", "example/uv"),
            ("GITHUB_REF", "refs/pull/1/merge"),
            ("GITHUB_EVENT_NAME", "pull_request"),
        ):
            with self.subTest(key=key), self.assertRaises(ValueError):
                profiles.destination_metadata(metadata(), ENVIRONMENT | {key: value})

    def test_source_identity(self):
        for key, value in (
            ("repository", "uv-dev"),
            ("commitHash", "b" * 40),
            ("event", "pull_request"),
            ("ref", "refs/pull/1/merge"),
            ("runPart", {"runId": "999"}),
            ("runner", {"executor": "valgrind"}),
        ):
            with self.subTest(key=key), self.assertRaises(ValueError):
                profiles.verify_source(
                    metadata() | {key: value}, SHA, SOURCE_RUN, "walltime"
                )

    def test_capture_relays_exact_archive_and_metadata(self):
        source = metadata()
        uploads = []

        def capture_upload(url, path, received_metadata):
            uploads.append((url, path.read_bytes(), copy.deepcopy(received_metadata)))

        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            with (
                profiles.CaptureServer(
                    directory, SHA, SOURCE_RUN, "walltime"
                ) as server,
                patch.object(
                    profiles,
                    "upload_request",
                    return_value={
                        "runId": "codspeed-run",
                        "uploadUrl": "https://storage.invalid/profile",
                    },
                ) as prepare,
                patch.object(profiles, "upload_profile", side_effect=capture_upload),
            ):
                thread = threading.Thread(target=server.serve_forever)
                thread.start()
                try:
                    response = json.loads(
                        profiles.request(
                            f"{server.endpoint}/upload",
                            data=json.dumps(source).encode(),
                            headers={
                                "Authorization": "not-persisted",
                                "Content-Type": "application/json",
                            },
                        )
                    )
                    profiles.request(response["uploadUrl"], data=PROFILE, method="PUT")
                    self.assertTrue(server.complete)
                    prepare.assert_called_once_with(source, "not-persisted")
                    self.assertEqual(
                        uploads, [("https://storage.invalid/profile", PROFILE, source)]
                    )
                    self.assertEqual((directory / "profile.tar").read_bytes(), PROFILE)
                    self.assertEqual(
                        json.loads((directory / "metadata.json").read_text()), source
                    )
                    self.assertNotIn(
                        "not-persisted", (directory / "metadata.json").read_text()
                    )
                finally:
                    server.shutdown()
                    thread.join()

    def test_checksum_mismatch(self):
        with tempfile.TemporaryDirectory() as temporary:
            profile = Path(temporary) / "profile.tar"
            profile.write_bytes(b"different profile")
            with self.assertRaises(ValueError):
                profiles.verify_profile(profile, metadata())

    def test_import_verifies_both_modes_before_upload(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for mode, name in profiles.ARTIFACTS.items():
                bundle = directory / name
                bundle.mkdir()
                (bundle / "metadata.json").write_text(json.dumps(metadata(mode)))
                (bundle / "profile.tar").write_bytes(PROFILE)
            with (
                patch.dict(profiles.os.environ, ENVIRONMENT),
                patch.object(
                    profiles, "oidc_token", return_value="destination-token"
                ) as token,
                patch.object(
                    profiles,
                    "upload_request",
                    return_value={
                        "runId": "new-run",
                        "uploadUrl": "https://storage.invalid/profile",
                    },
                ) as prepare,
                patch.object(profiles, "upload_profile") as upload,
            ):
                profiles.import_profiles(directory, SOURCE_RUN)
                self.assertEqual(token.call_count, 2)
                self.assertEqual(upload.call_count, 2)
                for call in prepare.call_args_list:
                    imported, credential = call.args
                    self.assertEqual(imported["runner"]["arch"], "aarch64")
                    self.assertEqual(imported["repository"], "uv-security")
                    self.assertEqual(credential, "destination-token")
                prepare.reset_mock()
                token.reset_mock()
                (
                    directory / profiles.ARTIFACTS["walltime"] / "profile.tar"
                ).write_bytes(b"bad")
                with self.assertRaises(ValueError):
                    profiles.import_profiles(directory, SOURCE_RUN)
                prepare.assert_not_called()
                token.assert_not_called()

    def test_find_requires_exact_public_main_and_successful_jobs(self):
        source = {
            "id": int(SOURCE_RUN),
            "head_sha": SHA,
            "head_branch": "main",
            "event": "push",
            "repository": {"full_name": "astral-sh/uv"},
            "head_repository": {"full_name": "astral-sh/uv"},
        }
        jobs = [
            {"name": name, "conclusion": "success"} for name in profiles.SOURCE_JOBS
        ]
        artifacts = [
            {"name": name, "expired": False} for name in profiles.ARTIFACTS.values()
        ]

        def response(path, key):
            if "/workflows/" in path:
                return [source]
            if "/jobs?" in path:
                return jobs
            return artifacts

        with patch.object(profiles, "github_items", side_effect=response):
            self.assertEqual(profiles.find_source_run(SHA, timeout=0), SOURCE_RUN)
            jobs[0]["conclusion"] = "failure"
            with self.assertRaises(RuntimeError):
                profiles.find_source_run(SHA, timeout=0)
            jobs[0]["conclusion"] = "success"
            source["head_branch"] = "feature"
            with self.assertRaises(RuntimeError):
                profiles.find_source_run(SHA, timeout=0)


if __name__ == "__main__":
    unittest.main()
