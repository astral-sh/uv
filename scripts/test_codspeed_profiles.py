"""Exercise profile capture, which the benchmark workflow skips on pull requests."""

import importlib.util
import json
import os
import tempfile
import threading
import unittest
import urllib.error
import urllib.request
import xml.etree.ElementTree as ET
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "profiles", Path(__file__).with_name("codspeed-profiles.py")
)
assert spec and spec.loader
profiles = importlib.util.module_from_spec(spec)
spec.loader.exec_module(profiles)

SHA = "a" * 40
PAYLOAD = b"123456789"
METADATA = {
    "repositoryProvider": "GITHUB",
    "runEnvironment": "GITHUB_ACTIONS",
    "owner": "astral-sh",
    "repository": "uv",
    "commitHash": SHA,
    "runPart": {"runId": "123"},
    "ref": "refs/heads/main",
    "event": "push",
    "runner": {"executor": "valgrind"},
}
LEGACY = {**METADATA, "version": 11, "profileMd5": "JfnnlDI7RTiF9RgfG2JNCw=="}
MULTIPART: dict = {
    **METADATA,
    "version": 12,
    "profileArchiveMetadata": {
        "encoding": None,
        "size": len(PAYLOAD),
        "partSize": 5,
        "partCrc64nvmes": ["Fmrit/9iyvg=", "4NY2j0LQBzc="],
        # Standard CRC-64/NVME check value for "123456789" (ae8b14860a799888).
        "crc64nvme": "rosUhgp5mIg=",
    },
}


def upload_request(metadata, token):
    prefix = f"https://example.invalid/{metadata['repository']}"
    if metadata["version"] < 12:
        return {"uploadUrl": prefix}
    return {
        "runId": "codspeed-run",
        "status": "success",
        "multipartUpload": {
            "parts": [
                {"url": f"{prefix}/{index}", "headers": {"X-Signed": str(index)}}
                for index in range(2)
            ],
            "complete": {"url": prefix, "headers": {"X-Signed": "complete"}},
        },
    }


class ProfileTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.directory = Path(temp.name)
        self.uploads = {}
        self.error = False
        for name, handler in [
            ("upload_request", upload_request),
            ("storage_request", self.storage),
        ]:
            mock = patch.object(profiles, name, side_effect=handler)
            mock.start()
            self.addCleanup(mock.stop)

    def storage(self, method, url, headers, body):
        if method == "PUT":
            self.uploads[url] = b"".join(body)
            if "Content-MD5" in headers:
                self.assertEqual(headers["Content-MD5"], LEGACY["profileMd5"])
                return None, b""
            index = url.rsplit("/", 1)[1]
            self.assertEqual(headers["x-signed"], index)
            self.assertEqual(int(headers["content-length"]), len(self.uploads[url]))
            return f'"etag-{index}"', b""
        self.assertEqual(headers["x-signed"], "complete")
        parts = ET.fromstring(body)
        self.assertEqual(
            [(part.findtext("PartNumber"), part.findtext("ETag")) for part in parts],
            [("1", '"etag-0"'), ("2", '"etag-1"')],
        )
        return None, b"<Error/>" if self.error else b"<CompleteMultipartUploadResult/>"

    def capture(self, metadata):
        server = profiles.CaptureServer(
            self.directory, SHA, "123", "simulation", "refs/heads/main", "push"
        )
        self.addCleanup(server.server_close)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.shutdown)
        response = profiles.request(
            server.endpoint + "/upload", data=json.dumps(metadata).encode()
        )
        self.assertNotIn(b"example.invalid", response)
        return json.loads(response)

    def put(self, url, payload):
        request = urllib.request.Request(url, data=payload, method="PUT")
        with urllib.request.urlopen(request, timeout=5) as response:
            return response.headers.get("ETag")

    def test_single_upload(self):
        self.put(self.capture(LEGACY)["uploadUrl"], PAYLOAD)
        self.assertEqual(self.uploads, {"https://example.invalid/uv": PAYLOAD})
        self.assertEqual((self.directory / "profile.tar").read_bytes(), PAYLOAD)

    def test_multipart_capture(self):
        response = self.capture(MULTIPART)
        self.assertEqual(response["runId"], "codspeed-run")
        upload = response["multipartUpload"]
        first, second = [part["url"] for part in upload["parts"]]
        complete = upload["complete"]["url"]
        with self.assertRaises(urllib.error.HTTPError):
            self.put(first, b"wrong")
        with self.assertRaises(RuntimeError):
            profiles.request(complete, data=b"")
        with ThreadPoolExecutor(2) as pool:
            last = pool.submit(self.put, second, b"6789")
            self.assertEqual(self.put(first, b"12345"), '"etag-0"')
            self.assertEqual(last.result(), '"etag-1"')
        self.assertEqual(self.put(second, b"6789"), '"etag-1"')
        with self.assertRaises(urllib.error.HTTPError):
            self.put(second, b"6780")
        self.error = True
        with self.assertRaises(RuntimeError):
            profiles.request(complete, data=b"")
        self.assertFalse((self.directory / "profile.tar").exists())
        self.assertFalse((self.directory / "metadata.json").exists())
        self.error = False
        profiles.request(complete, data=b"")
        profiles.request(
            complete, data=b""
        )  # A lost completion response is safe to retry.
        self.assertEqual((self.directory / "profile.tar").read_bytes(), PAYLOAD)
        self.assertEqual(
            json.loads((self.directory / "metadata.json").read_bytes()), MULTIPART
        )

    def test_valid_parts_must_also_match_archive_checksum(self):
        profile = self.directory / "profile.tar"
        profile.write_bytes(PAYLOAD)
        metadata = {
            **MULTIPART,
            "profileArchiveMetadata": {
                **MULTIPART["profileArchiveMetadata"],
                "crc64nvme": "AAAAAAAAAAA=",
            },
        }
        with self.assertRaisesRegex(ValueError, "archive checksum"):
            profiles.verify_profile(profile, metadata)

    def test_import_validates_both_profiles_before_upload(self):
        def artifact(mode, metadata):
            folder = self.directory / profiles.ARTIFACTS[mode]
            folder.mkdir()
            (folder / "metadata.json").write_text(json.dumps(metadata))
            return folder / "profile.tar"

        artifact("simulation", LEGACY).write_bytes(PAYLOAD)
        walltime = artifact(
            "walltime", {**MULTIPART, "runner": {"executor": "walltime"}}
        )
        env = {
            "GITHUB_REPOSITORY": "astral-sh/uv-dev",
            "GITHUB_REF": "refs/heads/main",
            "GITHUB_EVENT_NAME": "workflow_dispatch",
            "GITHUB_JOB": "import",
            "GITHUB_RUN_ID": "456",
            "GITHUB_ACTOR": "importer",
            "GITHUB_ACTOR_ID": "789",
        }
        with patch.dict(os.environ, env), patch.object(profiles, "oidc_token") as token:
            walltime.write_bytes(b"123456780")
            with self.assertRaisesRegex(ValueError, "checksum"):
                profiles.import_profiles(self.directory, "123", SHA)
            token.assert_not_called()
            walltime.write_bytes(PAYLOAD)
            profiles.import_profiles(self.directory, "123", SHA)
        self.assertEqual(
            self.uploads,
            {
                "https://example.invalid/uv-dev": PAYLOAD,
                "https://example.invalid/uv-dev/0": b"12345",
                "https://example.invalid/uv-dev/1": b"6789",
            },
        )


if __name__ == "__main__":
    unittest.main()
