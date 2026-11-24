# /// script
# requires-python = ">=3.10"
# dependencies = []
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Retain CodSpeed uploads and import public-main measurements into uv-dev.

CodSpeed uploads an archive in one request (metadata v10 and v11) or as an
S3 multipart upload (v12). Retain the archive with its original measurement
metadata so uv-dev can replay either protocol under its own identity.
"""

import argparse
import base64
import copy
import hashlib
import http.client
import json
import os
import re
import secrets
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET
from collections.abc import Iterable, Iterator, Mapping
from functools import cache
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from io import BufferedIOBase
from pathlib import Path

UPLOAD_URL = "https://api.codspeed.io/upload"
SOURCE_REPOSITORY = "astral-sh/uv"
DESTINATION_REPOSITORY = "astral-sh/uv-dev"
EXECUTORS = {"simulation": "valgrind", "walltime": "walltime"}
ARTIFACTS = {mode: f"codspeed-profiles-{mode}" for mode in EXECUTORS}
SOURCE_JOBS = {"bench / simulated", "bench / walltime on x86_64 linux"}


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise urllib.error.HTTPError(req.full_url, code, msg, headers, fp)


def request(url: str, *, data=None, headers=None, method=None) -> bytes:
    """Do not include signed URLs, credentials, or response bodies in errors."""
    try:
        with urllib.request.build_opener(NoRedirect).open(
            urllib.request.Request(
                url, data=data, headers=headers or {}, method=method
            ),
            timeout=600,
        ) as response:
            return response.read()
    except urllib.error.HTTPError as error:
        raise RuntimeError(f"HTTP request failed with status {error.code}") from None
    except urllib.error.URLError:
        raise RuntimeError("HTTP request failed") from None


def upload_request(metadata: dict, token: str | None) -> dict:
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = token
    return json.loads(
        request(
            UPLOAD_URL,
            data=json.dumps(metadata).encode(),
            headers=headers,
        )
    )


class TransientUploadError(RuntimeError):
    """Storage returned a failure that can be retried."""


def storage_request(
    method: str, url: str, headers: dict, body: Iterable[bytes] | bytes
) -> tuple[str | None, bytes]:
    """Use only the storage request supplied by CodSpeed. Never follow redirects."""
    try:
        parsed = urllib.parse.urlsplit(url)
        if (
            parsed.scheme != "https"
            or not parsed.hostname
            or parsed.username is not None
        ):
            raise ValueError("Invalid storage endpoint")
        connection = http.client.HTTPSConnection(
            parsed.hostname, parsed.port, timeout=600
        )
        try:
            connection.request(
                method,
                urllib.parse.urlunsplit(("", "", parsed.path, parsed.query, "")),
                body=body,
                headers=headers,
            )
            response = connection.getresponse()
            if response.status in {408, 429, 500, 502, 503, 504}:
                raise TransientUploadError(
                    f"Profile upload failed with status {response.status}"
                )
            if not 200 <= response.status < 300:
                raise RuntimeError(
                    f"Profile upload failed with status {response.status}"
                )
            return response.getheader("ETag"), response.read()
        finally:
            connection.close()
    except (ConnectionError, TimeoutError, http.client.IncompleteRead):
        raise TransientUploadError("Profile upload connection failed") from None
    except (OSError, ValueError, http.client.HTTPException):
        raise RuntimeError("Profile upload failed") from None


def chunks(stream: BufferedIOBase, length: int) -> Iterator[bytes]:
    while length:
        chunk = stream.read(min(length, 1024 * 1024))
        if not chunk:
            raise ValueError("Incomplete profile upload")
        length -= len(chunk)
        yield chunk


def upload_profile(url: str, profile: Path, metadata: dict) -> None:
    size = profile.stat().st_size
    headers = {
        "Content-Type": "application/x-tar",
        "Content-Length": str(size),
        "Content-MD5": metadata["profileMd5"],
    }
    if encoding := metadata.get("profileEncoding"):
        headers["Content-Encoding"] = encoding
    with profile.open("rb") as stream:
        storage_request("PUT", url, headers, chunks(stream, size))


@cache
def crc64_table() -> list[int]:
    """Use the reflected CRC-64/NVME polynomial specified by S3."""
    table = []
    for byte in range(256):
        crc = byte
        for _ in range(8):
            crc = (crc >> 1) ^ (0x9A6C9329AC4BC9B5 if crc & 1 else 0)
        table.append(crc)
    return table


def crc64nvme(data: Iterable[bytes]) -> str:
    table = crc64_table()
    mask = 0xFFFFFFFFFFFFFFFF
    crc = mask
    for chunk in data:
        for byte in chunk:
            crc = table[(crc ^ byte) & 0xFF] ^ (crc >> 8)
    return base64.b64encode((crc ^ mask).to_bytes(8, "big")).decode()


def part_lengths(archive: dict) -> list[int]:
    size = archive["size"]
    part_size = archive["partSize"]
    checksums = archive["partCrc64nvmes"]
    if (
        not isinstance(size, int)
        or not isinstance(part_size, int)
        or size <= 0
        or not 0 < part_size <= 5 * 1024**3
        or not isinstance(checksums, list)
        or not 1 <= len(checksums) <= 10_000
        or len(checksums) != (size + part_size - 1) // part_size
    ):
        raise ValueError("Invalid CodSpeed profile parts")
    return [min(part_size, size - offset) for offset in range(0, size, part_size)]


def verify_profile(profile: Path, metadata: dict) -> None:
    if metadata["version"] == 12:
        archive = metadata["profileArchiveMetadata"]
        lengths = part_lengths(archive)
        if profile.stat().st_size != archive["size"]:
            raise ValueError("Unexpected profile archive size")
        with profile.open("rb") as stream:
            for length, expected in zip(
                lengths, archive["partCrc64nvmes"], strict=True
            ):
                actual = crc64nvme(chunks(stream, length))
                if actual != expected:
                    raise ValueError("Profile part checksum does not match")
            if len(lengths) > 1:
                stream.seek(0)
                actual = crc64nvme(chunks(stream, archive["size"]))
            if actual != archive["crc64nvme"]:
                raise ValueError("Profile archive checksum does not match")
        return
    digest = hashlib.md5(usedforsecurity=False)
    with profile.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
    if base64.b64encode(digest.digest()).decode() != metadata["profileMd5"]:
        raise ValueError("Profile archive does not match its upload metadata")


def verify_recording(
    metadata: dict, sha: str, run_id: str, mode: str, ref: str, event: str
) -> None:
    expected = {
        "repositoryProvider": "GITHUB",
        "runEnvironment": "GITHUB_ACTIONS",
        "owner": "astral-sh",
        "repository": "uv",
        "ref": ref,
        "event": event,
        "commitHash": sha,
    }
    if any(metadata.get(key) != value for key, value in expected.items()):
        raise ValueError("Expected measurements from the requested public uv run")
    if metadata["runPart"]["runId"] != run_id:
        raise ValueError("Measurements belong to a different GitHub Actions run")
    if metadata["runner"]["executor"] != EXECUTORS[mode]:
        raise ValueError("Unexpected CodSpeed executor")
    if metadata["version"] not in {10, 11, 12}:
        raise ValueError("Unsupported CodSpeed upload metadata version")
    if metadata["version"] == 12:
        archive = metadata["profileArchiveMetadata"]
        part_lengths(archive)
        encoding = archive.get("encoding")
    else:
        encoding = metadata.get("profileEncoding")
    if encoding not in (None, "gzip"):
        raise ValueError("Unsupported profile encoding")


def verify_source(metadata: dict, sha: str, run_id: str, mode: str) -> None:
    verify_recording(metadata, sha, run_id, mode, "refs/heads/main", "push")


class MultipartUpload:
    """One CodSpeed upload. Keep presigned requests in memory on this runner."""

    def __init__(self, metadata: dict, upload: dict):
        self.archive = metadata["profileArchiveMetadata"]
        self.lengths = part_lengths(self.archive)
        self.parts = upload["parts"]
        self.completion = upload["complete"]
        if len(self.parts) != len(self.lengths):
            raise ValueError("Unexpected number of CodSpeed upload requests")
        self.etags: dict[int, str] = {}
        self.locks = [threading.Lock() for _ in self.parts]

    def capture(
        self, index: int, source: BufferedIOBase, length: int, partial: Path
    ) -> str:
        if length != self.lengths[index]:
            raise ValueError("Unexpected profile part size")
        expected = self.archive["partCrc64nvmes"][index]
        # Parts can arrive out of order. A retry must not overwrite bytes
        # already acknowledged by storage, even if completion has since run.
        with self.locks[index]:
            if index in self.etags:
                if crc64nvme(chunks(source, length)) != expected:
                    raise ValueError("Invalid profile part retry")
                return self.etags[index]
            offset = index * self.archive["partSize"]
            with partial.open("r+b") as stream:
                stream.seek(offset)
                stream.writelines(chunks(source, length))
                stream.seek(offset)
                if crc64nvme(chunks(stream, length)) != expected:
                    raise ValueError("Profile part checksum does not match")
                stream.seek(offset)
                return self.put(index, stream)

    def put(self, index: int, stream: BufferedIOBase) -> str:
        part = self.parts[index]
        headers = {key.lower(): value for key, value in part["headers"].items()}
        length = str(self.lengths[index])
        if headers.setdefault("content-length", length) != length:
            raise ValueError("Unexpected signed profile part size")
        etag, _ = storage_request(
            "PUT", part["url"], headers, chunks(stream, self.lengths[index])
        )
        if not etag or not re.fullmatch(r"[ -~]+", etag):
            raise ValueError("Invalid ETag in profile part response")
        self.etags[index] = etag
        return etag

    def finish(self, *, retries: int = 0) -> None:
        if len(self.etags) != len(self.parts):
            raise ValueError("Missing profile parts")
        root = ET.Element("CompleteMultipartUpload")
        for index in range(len(self.parts)):
            part = ET.SubElement(root, "Part")
            ET.SubElement(part, "PartNumber").text = str(index + 1)
            ET.SubElement(part, "ETag").text = self.etags[index]
        headers = {
            key.lower(): value for key, value in self.completion["headers"].items()
        }
        headers.setdefault("content-type", "application/xml")
        payload = ET.tostring(root)
        for attempt in range(retries + 1):
            try:
                self.send_completion(headers, payload)
                return
            except TransientUploadError:
                if attempt == retries:
                    raise
                time.sleep(2**attempt)

    def send_completion(self, headers: dict, payload: bytes) -> None:
        _, body = storage_request("POST", self.completion["url"], headers, payload)
        # S3 can send an error in the body of an HTTP 200 response.
        try:
            result = ET.fromstring(body.strip())
        except ET.ParseError:
            raise ValueError("Invalid profile upload completion response") from None
        tag = result.tag.rsplit("}", 1)[-1]
        if tag == "CompleteMultipartUploadResult":
            return
        if tag == "Error" and result.findtext("{*}Code") in {
            "InternalError",
            "ServiceUnavailable",
            "SlowDown",
            "RequestTimeout",
        }:
            raise TransientUploadError("Profile upload did not complete")
        raise ValueError("Profile upload did not complete")


class CaptureServer(ThreadingHTTPServer):
    """A write-through relay: retain exactly what the runner uploads to CodSpeed."""

    def __init__(
        self, directory: Path, sha: str, run_id: str, mode: str, ref: str, event: str
    ):
        super().__init__(("127.0.0.1", 0), CaptureHandler)
        self.directory = directory
        self.sha = sha
        self.run_id = run_id
        self.mode = mode
        self.ref = ref
        self.event = event
        self.nonce = secrets.token_urlsafe()
        self.pending: tuple[dict, str | MultipartUpload, dict] | None = None
        self.lock = threading.Lock()
        self.partial = directory / "profile.partial"
        self.complete = False

    @property
    def endpoint(self) -> str:
        return f"http://127.0.0.1:{self.server_port}/{self.nonce}"

    def publish(self, metadata: dict) -> None:
        self.partial.replace(self.directory / "profile.tar")
        (self.directory / "metadata.json").write_text(
            json.dumps(metadata) + "\n", encoding="utf-8"
        )
        self.complete = True


class CaptureHandler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        # HTTP paths contain the relay nonce; never log requests or headers.
        pass

    def respond(self, status: int, value: dict, *, etag: str | None = None) -> None:
        body = json.dumps(value).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        if etag is not None:
            self.send_header("ETag", etag)
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        assert isinstance(self.server, CaptureServer)
        try:
            if self.path == f"/{self.server.nonce}/finish":
                self.respond(200, {"complete": self.server.complete})
                threading.Thread(target=self.server.shutdown, daemon=True).start()
                return
            if self.path == f"/{self.server.nonce}/complete" and self.server.pending:
                metadata, upload, _ = self.server.pending
                if not isinstance(upload, MultipartUpload):
                    raise ValueError("Unexpected multipart completion")
                # Build the completion from the ETags storage returned. Neither
                # the client nor a retained artifact can choose an upload URL.
                with self.server.lock:
                    if not self.server.complete:
                        if len(upload.etags) != len(upload.parts):
                            raise ValueError("Missing profile parts")
                        verify_profile(self.server.partial, metadata)
                        upload.finish()
                        self.server.publish(metadata)
                self.respond(200, {})
                return
            if self.path != f"/{self.server.nonce}/upload":
                self.respond(404, {})
                return
            length = int(self.headers.get("Content-Length", "0"))
            if not 0 < length <= 1024 * 1024:
                raise ValueError("Unexpected metadata upload")
            metadata = json.loads(self.rfile.read(length))
            verify_recording(
                metadata,
                self.server.sha,
                self.server.run_id,
                self.server.mode,
                self.server.ref,
                self.server.event,
            )
            token = self.headers.get("Authorization")
            with self.server.lock:
                if self.server.pending:
                    original, _, response = self.server.pending
                    if metadata != original:
                        raise ValueError("Profile upload already started")
                else:
                    response = upload_request(metadata, token)
                    upload: str | MultipartUpload
                    if metadata["version"] == 12:
                        upload = MultipartUpload(metadata, response["multipartUpload"])
                        self.server.partial.touch(exist_ok=False)
                        response["multipartUpload"] = {
                            "parts": [
                                {
                                    "url": f"{self.server.endpoint}/part/{index}",
                                    "headers": {},
                                }
                                for index in range(len(upload.parts))
                            ],
                            "complete": {
                                "url": f"{self.server.endpoint}/complete",
                                "headers": {},
                            },
                        }
                    else:
                        upload = response["uploadUrl"]
                        response["uploadUrl"] = f"{self.server.endpoint}/profile"
                    self.server.pending = metadata, upload, response
            self.respond(200, response)
        except (KeyError, OSError, RuntimeError, ValueError) as error:
            print(f"CodSpeed capture failed: {type(error).__name__}", flush=True)
            self.respond(502, {"error": "Could not prepare CodSpeed profile capture"})

    def do_PUT(self):
        assert isinstance(self.server, CaptureServer)
        try:
            if not self.server.pending:
                self.respond(404, {})
                return
            metadata, upload, _ = self.server.pending
            length = int(self.headers.get("Content-Length", "0"))
            if length <= 0:
                raise ValueError("Empty profile upload")
            if isinstance(upload, MultipartUpload):
                match = re.fullmatch(rf"/{self.server.nonce}/part/([0-9]+)", self.path)
                if not match or int(match[1]) >= len(upload.parts):
                    self.respond(404, {})
                    return
                etag = upload.capture(
                    int(match[1]), self.rfile, length, self.server.partial
                )
                self.respond(200, {}, etag=etag)
                return
            if self.path != f"/{self.server.nonce}/profile":
                self.respond(404, {})
                return
            with self.server.lock:
                if self.server.complete:
                    with (self.server.directory / "profile.tar").open("rb") as profile:
                        if length != os.fstat(profile.fileno()).st_size or any(
                            chunk != profile.read(len(chunk))
                            for chunk in chunks(self.rfile, length)
                        ):
                            raise ValueError("Invalid profile retry")
                else:
                    with self.server.partial.open("wb") as output:
                        output.writelines(chunks(self.rfile, length))
                    verify_profile(self.server.partial, metadata)
                    upload_profile(upload, self.server.partial, metadata)
                    self.server.publish(metadata)
            self.respond(200, {})
        except (KeyError, OSError, RuntimeError, ValueError) as error:
            print(f"CodSpeed capture failed: {type(error).__name__}", flush=True)
            self.respond(502, {"error": "Could not retain CodSpeed profile"})


def start_capture(directory: Path, mode: str) -> None:
    if os.environ["GITHUB_REPOSITORY"] != SOURCE_REPOSITORY:
        raise ValueError("Only public uv runs may publish shared profiles")
    directory.mkdir(parents=True, exist_ok=False)
    with (directory / "capture.log").open("wb") as log:
        process = subprocess.Popen(
            [sys.executable, __file__, "serve", str(directory), mode],
            stdout=log,
            stderr=log,
            start_new_session=True,
        )
    ready = directory / "endpoint"
    for _ in range(100):
        if ready.exists():
            print(f"upload-url={ready.read_text().strip()}/upload")
            return
        if process.poll() is not None:
            raise RuntimeError("CodSpeed capture exited before becoming ready")
        time.sleep(0.05)
    process.terminate()
    raise RuntimeError("CodSpeed capture did not become ready")


def serve_capture(directory: Path, mode: str) -> None:
    with CaptureServer(
        directory,
        os.environ["GITHUB_SHA"],
        os.environ["GITHUB_RUN_ID"],
        mode,
        os.environ["GITHUB_REF"],
        os.environ["GITHUB_EVENT_NAME"],
    ) as server:
        ready = directory / "endpoint.partial"
        ready.write_text(server.endpoint, encoding="utf-8")
        ready.replace(directory / "endpoint")
        server.serve_forever()


def finish_capture(directory: Path) -> None:
    endpoint = (directory / "endpoint").read_text().strip()
    response = json.loads(request(f"{endpoint}/finish", data=b""))
    if not response["complete"]:
        raise RuntimeError("CodSpeed did not finish uploading its profile")
    metadata = json.loads((directory / "metadata.json").read_text())
    verify_profile(directory / "profile.tar", metadata)


def github_items(path: str, key: str) -> list[dict]:
    pages = json.loads(
        subprocess.check_output(["gh", "api", "--paginate", "--slurp", path], text=True)
    )
    return [item for page in pages for item in page[key]]


def github_object(path: str, *, missing_ok: bool = False) -> dict | None:
    result = subprocess.run(
        ["gh", "api", path], capture_output=True, text=True, check=False
    )
    if result.returncode:
        if missing_ok and "(HTTP 404)" in result.stderr:
            return None
        raise RuntimeError("Could not read GitHub workflow information")
    return json.loads(result.stdout)


def find_source_run(sha: str, run_id: str | None = None) -> str | None:
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        raise ValueError("Expected a full commit SHA")
    if run_id:
        if not run_id.isdecimal():
            raise ValueError("Expected a GitHub Actions run ID")
        runs = [github_object(f"repos/{SOURCE_REPOSITORY}/actions/runs/{run_id}")]
    else:
        runs = github_items(
            f"repos/{SOURCE_REPOSITORY}/actions/workflows/ci.yml/runs"
            f"?event=push&head_sha={sha}&per_page=100",
            "workflow_runs",
        )
    for run in runs:
        if not run or (
            run["head_sha"] != sha
            or run["head_branch"] != "main"
            or run["event"] != "push"
            or run["path"] != ".github/workflows/ci.yml"
            or run["repository"]["full_name"] != SOURCE_REPOSITORY
            or run["head_repository"]["full_name"] != SOURCE_REPOSITORY
        ):
            continue
        source_run = str(run["id"])
        jobs = github_items(
            f"repos/{SOURCE_REPOSITORY}/actions/runs/{source_run}/jobs?per_page=100",
            "jobs",
        )
        successful = {job["name"] for job in jobs if job["conclusion"] == "success"}
        if not SOURCE_JOBS <= successful:
            continue
        artifacts = github_items(
            f"repos/{SOURCE_REPOSITORY}/actions/runs/{source_run}/artifacts?per_page=100",
            "artifacts",
        )
        available = {item["name"] for item in artifacts if not item["expired"]}
        if set(ARTIFACTS.values()) <= available:
            return source_run
    return None


def commit_is_mirrored(sha: str) -> bool:
    repository = os.environ["GITHUB_REPOSITORY"]
    if (
        repository != DESTINATION_REPOSITORY
        or os.environ["GITHUB_REF"] != "refs/heads/main"
    ):
        raise ValueError("Expected a uv-dev main-branch workflow")
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        raise ValueError("Expected a full commit SHA")
    comparison = github_object(
        f"repos/{repository}/compare/{sha}...{os.environ['GITHUB_SHA']}",
        missing_ok=True,
    )
    return comparison is not None and comparison["status"] in {"ahead", "identical"}


def destination_metadata(source: dict, environment: Mapping[str, str]) -> dict:
    repository = environment["GITHUB_REPOSITORY"]
    if repository != DESTINATION_REPOSITORY:
        raise ValueError("Only uv-dev may import benchmark profiles")
    if environment["GITHUB_REF"] != "refs/heads/main" or environment[
        "GITHUB_EVENT_NAME"
    ] not in {"push", "workflow_dispatch"}:
        raise ValueError("Benchmark profiles must be imported on uv-dev main")
    if source["version"] not in {10, 11, 12}:
        raise ValueError("Unsupported CodSpeed upload metadata version")
    metadata = copy.deepcopy(source)
    owner, name = repository.split("/")
    job = environment["GITHUB_JOB"]
    run_id = environment["GITHUB_RUN_ID"]
    executor = metadata["runner"]["executor"]
    metadata.update(
        tokenless=False,
        owner=owner,
        repository=name,
        event=environment["GITHUB_EVENT_NAME"],
        sender={
            "id": environment["GITHUB_ACTOR_ID"],
            "login": environment["GITHUB_ACTOR"],
        },
        runPart={
            "runId": run_id,
            "runPartId": f"{job}-{executor}",
            "jobName": job,
            "metadata": {"executor": executor},
        },
    )
    if "ghData" in metadata:
        metadata["ghData"] = {"runId": run_id, "job": job}
    return metadata


def oidc_token() -> str:
    url = os.environ["ACTIONS_ID_TOKEN_REQUEST_URL"]
    url += "&" if "?" in url else "?"
    url += urllib.parse.urlencode({"audience": "codspeed.io"})
    return json.loads(
        request(
            url,
            headers={
                "Authorization": f"Bearer {os.environ['ACTIONS_ID_TOKEN_REQUEST_TOKEN']}",
                "Accept": "application/json",
            },
        )
    )["value"]


def import_profiles(directory: Path, source_run: str, source_sha: str) -> None:
    prepared = []
    for mode, artifact in ARTIFACTS.items():
        bundle = directory / artifact
        source = json.loads((bundle / "metadata.json").read_text())
        profile = bundle / "profile.tar"
        verify_source(source, source_sha, source_run, mode)
        verify_profile(profile, source)
        metadata = destination_metadata(source, os.environ)
        prepared.append((mode, profile, metadata))
    for mode, profile, metadata in prepared:
        response = upload_request(metadata, oidc_token())
        if metadata["version"] == 12:
            upload = MultipartUpload(metadata, response["multipartUpload"])
            with profile.open("rb") as stream:
                for index in range(len(upload.parts)):
                    upload.put(index, stream)
            upload.finish(retries=3)
        else:
            upload_profile(response["uploadUrl"], profile, metadata)
        print(f"Imported {mode} profiles from uv run {source_run}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("start", "serve"):
        command = commands.add_parser(name)
        command.add_argument("directory", type=Path)
        command.add_argument("mode", choices=EXECUTORS)
    commands.add_parser("finish").add_argument("directory", type=Path)
    command = commands.add_parser("find")
    command.add_argument("sha")
    command.add_argument("--run-id")
    command = commands.add_parser("import")
    command.add_argument("directory", type=Path)
    command.add_argument("source_run")
    command.add_argument("source_sha")
    args = parser.parse_args()
    match args.command:
        case "start":
            start_capture(args.directory, args.mode)
        case "serve":
            serve_capture(args.directory, args.mode)
        case "finish":
            finish_capture(args.directory)
        case "find":
            source_run = None
            if commit_is_mirrored(args.sha):
                source_run = find_source_run(args.sha, args.run_id)
            print(f"run-id={source_run or ''}")
        case "import":
            import_profiles(args.directory, args.source_run, args.source_sha)


if __name__ == "__main__":
    main()
