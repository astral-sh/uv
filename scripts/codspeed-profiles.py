# /// script
# requires-python = ">=3.10"
# dependencies = []
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Retain CodSpeed uploads and import public-main measurements into uv mirrors.

The upload protocol sends measurement metadata, then PUTs an archive to a signed
URL. Relaying both requests keeps the original machine and runner metadata with
the measured profile instead of reconstructing it on the importing runner.
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
from collections.abc import Mapping
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

UPLOAD_URL = "https://api.codspeed.io/upload"
SOURCE_REPOSITORY = "astral-sh/uv"
DESTINATIONS = {"astral-sh/uv-dev", "astral-sh/uv-security"}
EXECUTORS = {"simulation": "valgrind", "walltime": "walltime"}
ARTIFACTS = {mode: f"codspeed-profiles-{mode}" for mode in EXECUTORS}
SOURCE_JOBS = {"bench / simulated", "bench / walltime on aarch64 linux"}


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


def upload_profile(url: str, profile: Path, metadata: dict) -> None:
    parsed = urllib.parse.urlsplit(url)
    if parsed.scheme != "https" or not parsed.hostname or parsed.username:
        raise ValueError("CodSpeed returned an invalid profile upload URL")
    headers = {
        "Content-Type": "application/x-tar",
        "Content-Length": str(profile.stat().st_size),
        "Content-MD5": metadata["profileMd5"],
    }
    if encoding := metadata.get("profileEncoding"):
        headers["Content-Encoding"] = encoding
    connection = http.client.HTTPSConnection(parsed.hostname, parsed.port, timeout=600)
    try:
        with profile.open("rb") as stream:
            connection.request(
                "PUT",
                urllib.parse.urlunsplit(("", "", parsed.path, parsed.query, "")),
                body=stream,
                headers=headers,
            )
        response = connection.getresponse()
        if not 200 <= response.status < 300:
            raise RuntimeError(f"Profile upload failed with status {response.status}")
        response.read()
    except (OSError, http.client.HTTPException):
        raise RuntimeError("Profile upload failed") from None
    finally:
        connection.close()


def verify_profile(profile: Path, metadata: dict) -> None:
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
    if metadata.get("profileEncoding") not in (None, "gzip"):
        raise ValueError("Unsupported profile encoding")


def verify_source(metadata: dict, sha: str, run_id: str, mode: str) -> None:
    verify_recording(metadata, sha, run_id, mode, "refs/heads/main", "push")


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
        self.pending: tuple[dict, str] | None = None
        self.complete = False

    @property
    def endpoint(self) -> str:
        return f"http://127.0.0.1:{self.server_port}/{self.nonce}"


class CaptureHandler(BaseHTTPRequestHandler):
    server: CaptureServer

    def log_message(self, format, *args):
        # HTTP paths contain the relay nonce; never log requests or headers.
        pass

    def respond(self, status: int, value: dict) -> None:
        body = json.dumps(value).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        try:
            if self.path == f"/{self.server.nonce}/finish":
                self.respond(200, {"complete": self.server.complete})
                threading.Thread(target=self.server.shutdown, daemon=True).start()
                return
            if self.path != f"/{self.server.nonce}/upload":
                self.respond(404, {})
                return
            length = int(self.headers.get("Content-Length", "0"))
            if not 0 < length <= 1024 * 1024 or self.server.complete:
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
            response = upload_request(metadata, token)
            self.server.pending = metadata, response["uploadUrl"]
            response["uploadUrl"] = f"{self.server.endpoint}/profile"
            self.respond(200, response)
        except (KeyError, OSError, RuntimeError, ValueError) as error:
            print(f"CodSpeed capture failed: {type(error).__name__}", flush=True)
            self.respond(502, {"error": "Could not prepare CodSpeed profile capture"})

    def do_PUT(self):
        try:
            if self.path != f"/{self.server.nonce}/profile" or not self.server.pending:
                self.respond(404, {})
                return
            metadata, upload_url = self.server.pending
            length = int(self.headers.get("Content-Length", "0"))
            if length <= 0:
                raise ValueError("Empty profile upload")
            partial = self.server.directory / "profile.partial"
            with partial.open("wb") as output:
                remaining = length
                while remaining:
                    chunk = self.rfile.read(min(remaining, 1024 * 1024))
                    if not chunk:
                        raise ValueError("Incomplete profile upload")
                    output.write(chunk)
                    remaining -= len(chunk)
            verify_profile(partial, metadata)
            upload_profile(upload_url, partial, metadata)
            partial.replace(self.server.directory / "profile.tar")
            (self.server.directory / "metadata.json").write_text(
                json.dumps(metadata) + "\n", encoding="utf-8"
            )
            self.server.complete = True
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
    if repository not in DESTINATIONS or os.environ["GITHUB_REF"] != "refs/heads/main":
        raise ValueError("Expected a mirror main-branch workflow")
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        raise ValueError("Expected a full commit SHA")
    comparison = github_object(
        f"repos/{repository}/compare/{sha}...{os.environ['GITHUB_SHA']}",
        missing_ok=True,
    )
    return comparison is not None and comparison["status"] in {"ahead", "identical"}


def destination_metadata(source: dict, environment: Mapping[str, str]) -> dict:
    repository = environment["GITHUB_REPOSITORY"]
    if repository not in DESTINATIONS:
        raise ValueError("Only uv mirrors may import benchmark profiles")
    if environment["GITHUB_REF"] != "refs/heads/main" or environment[
        "GITHUB_EVENT_NAME"
    ] not in {"push", "workflow_dispatch"}:
        raise ValueError("Benchmark profiles must be imported on mirror main")
    if source["version"] not in {10, 11}:
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
    command.add_argument("--require-mirrored", action="store_true")
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
            if not args.require_mirrored or commit_is_mirrored(args.sha):
                source_run = find_source_run(args.sha, args.run_id)
            print(f"run-id={source_run or ''}")
        case "import":
            import_profiles(args.directory, args.source_run, args.source_sha)


if __name__ == "__main__":
    main()
