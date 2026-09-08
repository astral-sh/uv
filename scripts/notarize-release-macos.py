# /// script
# requires-python = ">=3.12"
# dependencies = ["boto3"]
#
# [tool.uv]
# no-build = true
# ///
"""Notarize uv's signed macOS release binaries with Astral's Azure key."""

import argparse
import base64
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile

import boto3
from botocore.config import Config
from botocore.exceptions import BotoCoreError, ClientError

NOTARY_URL = "https://appstoreconnect.apple.com/notary/v2/submissions"


def base64url(value: bytes) -> str:
    """Encode a JWT component without padding."""
    return base64.urlsafe_b64encode(value).decode("ascii").rstrip("=")


def azure_json(*arguments: str) -> dict:
    """Run an Azure CLI request without printing key-vault details to CI logs."""
    try:
        result = subprocess.run(
            ["az", *arguments, "--output", "json", "--only-show-errors"],
            check=True,
            capture_output=True,
            text=True,
        )
    except subprocess.CalledProcessError:
        raise RuntimeError("Azure Key Vault request failed") from None
    return json.loads(result.stdout)


def notarization_key() -> tuple[str, str, str]:
    """Resolve the current key version and its Apple key and issuer IDs."""
    vault = os.environ["AZURE_KEYVAULT_NAME"]
    name = os.environ["APPLE_NOTARIZATION_AKV_KEY_NAME"]
    key = azure_json(
        "keyvault",
        "key",
        "show",
        "--vault-name",
        vault,
        "--name",
        name,
        "--query",
        '{url:key.kid,key_id:tags."apple-key-id",issuer:tags."apple-issuer-id"}',
    )
    return key["url"], key["key_id"], key["issuer"]


def apple_token(key_url: str, key_id: str, issuer: str) -> str:
    """Sign a short-lived Notary API token without exporting the Apple private key."""
    now = int(time.time())
    header = {"alg": "ES256", "kid": key_id, "typ": "JWT"}
    claims = {
        "iss": issuer,
        "iat": now,
        "exp": now + 15 * 60,
        "aud": "appstoreconnect-v1",
        "scope": ["/notary/v2"],
    }
    signing_input = ".".join(
        base64url(json.dumps(value, separators=(",", ":")).encode())
        for value in (header, claims)
    )
    signature = azure_json(
        "rest",
        "--method",
        "post",
        "--url",
        f"{key_url}/sign?api-version=7.4",
        "--resource",
        "https://vault.azure.net",
        "--headers",
        "Content-Type=application/json",
        "--body",
        json.dumps(
            {
                "alg": "ES256",
                "value": base64url(
                    hashlib.sha256(signing_input.encode("ascii")).digest()
                ),
            }
        ),
    )
    if signature["kid"] != key_url:
        raise ValueError("Azure signed with an unexpected notarization key")
    if len(base64.urlsafe_b64decode(signature["value"] + "==")) != 64:
        raise ValueError("Azure returned an invalid ES256 signature")
    return f"{signing_input}.{signature['value']}"


def apple_json(token: str, suffix: str = "", body: dict | None = None) -> dict:
    """Send a JSON request to Apple's notary service."""
    request = urllib.request.Request(
        NOTARY_URL + suffix,
        data=json.dumps(body).encode() if body is not None else None,
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json",
        },
        method="POST" if body is not None else "GET",
    )
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        raise RuntimeError(f"Apple Notary API returned HTTP {error.code}") from None


def notarize(signed: Path) -> None:
    """Submit all targets' signed binaries together and wait for Apple's acceptance."""
    key_url, key_id, issuer = notarization_key()
    with tempfile.TemporaryDirectory(dir=os.environ.get("RUNNER_TEMP")) as temporary:
        archive = Path(temporary) / "uv-notarization.zip"
        with ZipFile(archive, "w", compression=ZIP_DEFLATED) as output:
            for target in sorted(signed.iterdir()):
                for path in sorted(target.iterdir()):
                    path.chmod(0o755)
                    output.write(path, path.relative_to(signed))

        submission = apple_json(
            apple_token(key_url, key_id, issuer),
            body={
                "submissionName": archive.name,
                "sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
            },
        )["data"]
        submission_id = submission["id"]
        upload = submission["attributes"]
        print(f"Uploading Apple notarization submission {submission_id}", flush=True)
        s3 = boto3.client(
            "s3",
            aws_access_key_id=upload["awsAccessKeyId"],
            aws_secret_access_key=upload["awsSecretAccessKey"],
            aws_session_token=upload["awsSessionToken"],
            config=Config(s3={"use_accelerate_endpoint": True}),
        )
        try:
            s3.upload_file(str(archive), upload["bucket"], upload["object"])
        except (BotoCoreError, ClientError):
            raise RuntimeError("Apple notarization upload failed") from None

        token = apple_token(key_url, key_id, issuer)
        deadline = time.monotonic() + 600
        while True:
            status = apple_json(token, f"/{submission_id}")["data"]["attributes"][
                "status"
            ]
            if status != "In Progress":
                log_url = apple_json(token, f"/{submission_id}/logs")["data"][
                    "attributes"
                ]["developerLogUrl"]
                if not log_url.startswith("https://"):
                    raise ValueError("Apple returned an invalid notarization log URL")
                with urllib.request.urlopen(log_url, timeout=60) as response:
                    log = json.load(response)
                for issue in log.get("issues") or []:
                    print(
                        f"Apple notarization {issue['severity']}: "
                        f"{issue['path']}: {issue['message']}",
                        file=sys.stderr,
                    )
                if status != "Accepted":
                    raise ValueError(f"Apple notarization {status}: {submission_id}")
                print(f"Apple notarization accepted: {submission_id}")
                return
            if time.monotonic() >= deadline:
                raise TimeoutError(f"Apple notarization timed out: {submission_id}")
            time.sleep(10)


def main() -> None:
    """Notarize the signed executables from uv's protected release job."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("signed", type=Path)
    args = parser.parse_args()
    try:
        notarize(args.signed)
    except (KeyError, OSError, ValueError, RuntimeError, TimeoutError) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
