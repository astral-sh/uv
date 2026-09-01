"""Bind the release signing workflow's artifacts; wheel rewriting remains in `uv-dev wheel-replace`.

Manifests are integrity bindings carried by job outputs, not signatures or attestations. A retry
must rerun preparation: artifacts from a different run attempt are deliberately rejected. This
caller owns uv's wheel layout and platform policy; the Rust helper has no repository knowledge.
"""

import base64
import csv
import hashlib
import io
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tarfile
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile

from check_uv_wheel_contents import check_uv_wheel, uv_build_expected, uv_expected

PLATFORMS = {
    "aarch64-apple-darwin": (
        "py3-none-macosx_11_0_arm64",
        {"uv": ["uv", "uvx"], "uv_build": ["uv-build"]},
    ),
    "x86_64-pc-windows-msvc": (
        "py3-none-win_amd64",
        {"uv": ["uv.exe", "uvx.exe", "uvw.exe"], "uv_build": ["uv-build.exe"]},
    ),
}
TARGET = os.environ.get("TARGET", "aarch64-apple-darwin")
TAG, BINARIES = PLATFORMS[TARGET]
TOOLS = ("uv-dev", "release_signing.py", "check_uv_wheel_contents.py")


def require(condition, message):
    """Reject an artifact that does not match the workflow's expectations."""
    if not condition:
        raise ValueError(message)


def digest(stream):
    """Return the SHA-256 digest and byte length of a binary stream."""
    hasher = hashlib.sha256()
    size = 0
    while chunk := stream.read(128 * 1024):
        hasher.update(chunk)
        size += len(chunk)
    return hasher.hexdigest(), size


def file_digest(path):
    """Return the SHA-256 digest of an artifact file."""
    require(stat.S_ISREG(path.lstat().st_mode), f"Not a regular file: {path}")
    with path.open("rb") as stream:
        return digest(stream)[0]


def context():
    """Identify the source revision, workflow attempt, and signing target."""
    return {
        "repository": os.environ["GITHUB_REPOSITORY"],
        "workflow_ref": os.environ["GITHUB_WORKFLOW_REF"],
        "workflow_sha": os.environ["GITHUB_WORKFLOW_SHA"],
        "run_id": os.environ["GITHUB_RUN_ID"],
        "run_attempt": os.environ["GITHUB_RUN_ATTEMPT"],
        "source_sha": os.environ["GITHUB_SHA"],
        "signing_workflow": ".github/workflows/sign-release-binaries.yml",
        "target": TARGET,
    }


def load(path, expected_digest):
    """Read a manifest bound to a job output and this workflow attempt."""
    require(file_digest(path) == expected_digest, f"Manifest digest mismatch: {path}")
    manifest = json.loads(path.read_bytes())
    require(
        manifest["schema"] == 1 and manifest["context"] == context(),
        "Wrong workflow context",
    )
    return manifest


def save(path, manifest):
    """Write a manifest and expose its digest as a job output."""
    with path.open("x") as stream:
        json.dump(manifest, stream, sort_keys=True, separators=(",", ":"))
        stream.write("\n")
    with Path(os.environ["GITHUB_OUTPUT"]).open("a") as stream:
        stream.write(f"manifest-sha256={file_digest(path)}\n")


def wheel_members(path):
    """Independent ZIP/RECORD validation for the fixed uv wheel layout."""
    check_uv_wheel(path)
    match = re.fullmatch(r"(uv|uv_build)-(\d+\.\d+\.\d+)-" + TAG + r"\.whl", path.name)
    require(match is not None, f"Unexpected wheel filename: {path.name}")
    distribution, version = match.groups()
    templates = uv_expected if distribution == "uv" else uv_build_expected
    if TARGET == "x86_64-pc-windows-msvc":
        if distribution == "uv":
            templates = templates | {"uv-VERSION.data/scripts/uvw"}
        templates = {
            name + ".exe" if ".data/scripts/" in name else name for name in templates
        }
    expected_names = {name.replace("VERSION", version) for name in templates}
    with ZipFile(path) as wheel:
        infos = wheel.infolist()
        require(
            len(infos) == len({info.filename for info in infos}),
            "Duplicate ZIP members",
        )
        require(
            {info.filename for info in infos} == expected_names,
            "Wheel version/layout mismatch",
        )
        members = {}
        for info in infos:
            require(not info.is_dir(), "Unexpected directory member")
            require(
                stat.S_IFMT(info.external_attr >> 16) in (0, stat.S_IFREG),
                "Special ZIP member",
            )
            require(info.file_size <= 512 * 1024 * 1024, "Oversized ZIP member")
            with wheel.open(info) as stream:
                sha256, size = digest(stream)
            members[info.filename] = {
                "sha256": sha256,
                "size": size,
                "method": info.compress_type,
                "time": list(info.date_time),
                "mode": info.external_attr,
                "internal": info.internal_attr,
                "comment": info.comment.hex(),
            }
        record_names = [name for name in members if name.endswith(".dist-info/RECORD")]
        require(len(record_names) == 1, "Expected one RECORD")
        record = record_names[0]
        require(members[record]["size"] <= 8 * 1024 * 1024, "Oversized RECORD")
        rows = list(csv.reader(io.StringIO(wheel.read(record).decode())))
        require(all(len(row) == 3 for row in rows), "Invalid RECORD field count")
        require(
            len(rows) == len(members) and {row[0] for row in rows} == set(members),
            "Invalid RECORD membership",
        )
        for name, hashed, size in rows:
            if name == record:
                require((hashed, size) == ("", ""), "Invalid RECORD self entry")
                continue
            algorithm, separator, value = hashed.partition("=")
            require(
                separator and algorithm in ("sha256", "sha384", "sha512"),
                "Insecure RECORD hash",
            )
            with wheel.open(name) as stream:
                hasher = hashlib.new(algorithm)
                while chunk := stream.read(128 * 1024):
                    hasher.update(chunk)
            encoded = base64.urlsafe_b64encode(hasher.digest()).decode().rstrip("=")
            require(
                value == encoded and size == str(members[name]["size"]),
                f"Invalid RECORD row: {name}",
            )
    return members


def prepare():
    """Extract the known executable members and bind the wheel and tooling inputs."""
    Path("unsigned").mkdir()
    manifest = {
        "schema": 1,
        "context": context(),
        "wheels": [],
        "tools": {name: file_digest(Path("tools") / name) for name in TOOLS},
    }
    paths = sorted(Path("wheels").glob("*.whl"))
    require(len(paths) == 2, "Expected two wheels")
    distributions = set()
    for path in paths:
        match = re.fullmatch(
            r"(uv|uv_build)-(\d+\.\d+\.\d+)-" + TAG + r"\.whl", path.name
        )
        require(match is not None, f"Unexpected wheel filename: {path.name}")
        distribution, version = match.groups()
        require(distribution not in distributions, "Duplicate distribution")
        distributions.add(distribution)
        members = wheel_members(path)
        replacements = {
            f"{distribution}-{version}.data/scripts/{binary}": binary
            for binary in BINARIES[distribution]
        }
        with ZipFile(path) as wheel:
            wheel_metadata = wheel.read(
                f"{distribution}-{version}.dist-info/WHEEL"
            ).decode()
            require(
                [
                    line
                    for line in wheel_metadata.splitlines()
                    if line.startswith("Tag:")
                ]
                == [f"Tag: {TAG}"],
                "Unexpected wheel tag",
            )
            for member, binary in replacements.items():
                with (
                    wheel.open(member) as source,
                    (Path("unsigned") / binary).open("xb") as output,
                ):
                    shutil.copyfileobj(source, output)
                require(
                    file_digest(Path("unsigned") / binary) == members[member]["sha256"],
                    "Extraction changed bytes",
                )
        manifest["wheels"].append(
            {
                "filename": path.name,
                "tag": TAG,
                "input_sha256": file_digest(path),
                "members": members,
                "replacements": replacements,
            }
        )
    save(Path("unsigned/manifest.json"), manifest)
    shutil.copyfile("unsigned/manifest.json", "manifest.json")


def signed_manifest():
    """Check that signing only added binary digests and their certificate identities."""
    source = load(Path("manifest.json"), os.environ["INPUT_MANIFEST_SHA256"])
    signed = load(Path("signed/manifest.json"), os.environ["SIGNED_MANIFEST_SHA256"])
    unsigned_fields = {
        key: value
        for key, value in signed.items()
        if key not in ("signed", "certificates")
    }
    require(unsigned_fields == source, "Signing changed the input manifest")
    require(
        set(signed["signed"])
        == {binary for names in BINARIES.values() for binary in names},
        "Unexpected signed binaries",
    )
    require(
        set(signed["certificates"]) == set(signed["signed"])
        and all(
            re.fullmatch(r"[0-9a-f]{64}", certificate)
            for certificate in signed["certificates"].values()
        ),
        "Invalid signing certificate map",
    )
    return signed


def verify_wheels(manifest, directory):
    """Check replaced bytes, untouched members, metadata, and regenerated RECORD files."""
    require(
        {path.name for path in directory.iterdir()}
        == {wheel["filename"] for wheel in manifest["wheels"]},
        "Unexpected output wheels",
    )
    for wheel in manifest["wheels"]:
        actual = wheel_members(directory / wheel["filename"])
        require(actual.keys() == wheel["members"].keys(), "Output membership changed")
        for name, before in wheel["members"].items():
            after = actual[name]
            if name.endswith(".dist-info/RECORD"):
                require(
                    after["method"] == ZIP_DEFLATED
                    and after["mode"] >> 16 == 0o100644
                    and after["time"] == [1980, 1, 1, 0, 0, 0]
                    and not after["comment"]
                    and after["internal"] == 0,
                    "Unexpected RECORD metadata",
                )
                continue
            expected = dict(before)
            if binary := wheel["replacements"].get(name):
                expected.update(manifest["signed"][binary])
            require(after == expected, f"Unexpected output member: {name}")


def write_archive():
    """Package the signed uv executables in the platform's release archive layout."""
    if TARGET == "x86_64-pc-windows-msvc":
        archive = f"uv-{TARGET}.zip"
        with ZipFile(Path("dist") / archive, "x", compression=ZIP_DEFLATED) as output:
            for binary in BINARIES["uv"]:
                output.writestr(binary, (Path("signed") / binary).read_bytes())
        (Path("dist") / f"{archive}.sha256").write_text(
            f"{file_digest(Path('dist') / archive)}  {archive}\n"
        )
    else:
        archive = f"uv-{TARGET}.tar.gz"
        with tarfile.open(Path("dist") / archive, "x:gz") as output:
            for binary in BINARIES["uv"]:
                path = Path("signed") / binary
                info = tarfile.TarInfo(f"uv-{TARGET}/{binary}")
                info.size = path.stat().st_size
                info.mode = 0o755
                with path.open("rb") as stream:
                    output.addfile(info, stream)
    return {"filename": archive, "sha256": file_digest(Path("dist") / archive)}


def verify_archive(manifest):
    """Check that the release archive contains the same signed bytes as the wheels."""
    extension = "zip" if TARGET == "x86_64-pc-windows-msvc" else "tar.gz"
    archive = f"uv-{TARGET}.{extension}"
    require(
        manifest["archive"]["filename"] == archive
        and file_digest(Path("dist") / archive) == manifest["archive"]["sha256"],
        "Final archive mismatch",
    )
    if extension == "zip":
        require(
            (Path("dist") / f"{archive}.sha256").read_text()
            == f"{manifest['archive']['sha256']}  {archive}\n",
            "Archive checksum mismatch",
        )
        with ZipFile(Path("dist") / archive) as source:
            require(
                sorted(source.namelist()) == sorted(BINARIES["uv"]),
                "Archive membership changed",
            )
            for binary in BINARIES["uv"]:
                with source.open(binary) as stream:
                    sha256, size = digest(stream)
                require(
                    {"sha256": sha256, "size": size} == manifest["signed"][binary],
                    "Archive binary mismatch",
                )
    else:
        with tarfile.open(Path("dist") / archive) as source:
            members = source.getmembers()
            require(
                len(members) == len(BINARIES["uv"])
                and {member.name for member in members}
                == {f"uv-{TARGET}/{binary}" for binary in BINARIES["uv"]},
                "Archive membership changed",
            )
            for member in members:
                require(
                    member.isfile() and member.mode == 0o755,
                    "Invalid archive member type/mode",
                )
                binary = member.name.rsplit("/", 1)[1]
                with source.extractfile(member) as stream:
                    sha256, size = digest(stream)
                require(
                    {"sha256": sha256, "size": size} == manifest["signed"][binary],
                    "Archive binary mismatch",
                )


def assemble():
    """Replace signed wheel members and create release artifacts without credentials."""
    manifest = signed_manifest()
    for name, expected in manifest["tools"].items():
        require(
            file_digest(Path("tools") / name) == expected,
            "Assembly tool digest mismatch",
        )
    for binary, expected in manifest["signed"].items():
        require(
            file_digest(Path("signed") / binary) == expected["sha256"],
            "Signed binary digest mismatch",
        )
    Path("dist/wheels").mkdir(parents=True)
    for wheel in manifest["wheels"]:
        source = Path("wheels") / wheel["filename"]
        require(
            file_digest(source) == wheel["input_sha256"], "Input wheel digest mismatch"
        )
        command = [
            str(Path("tools/uv-dev").resolve()),
            "wheel-replace",
            "--input",
            str(source),
            "--output",
            str(Path("dist/wheels") / source.name),
        ]
        for member, binary in wheel["replacements"].items():
            command.extend(["--replace", f"{member}=signed/{binary}"])
        subprocess.run(command, check=True)
    verify_wheels(manifest, Path("dist/wheels"))
    manifest["output_wheels"] = {
        wheel["filename"]: file_digest(Path("dist/wheels") / wheel["filename"])
        for wheel in manifest["wheels"]
    }
    manifest["archive"] = write_archive()
    verify_archive(manifest)
    save(Path("dist/manifest.json"), manifest)


def verify():
    """Validate the final artifacts and extract binaries for native signature checks."""
    signed = signed_manifest()
    final = load(Path("dist/manifest.json"), os.environ["FINAL_MANIFEST_SHA256"])
    require(
        {
            key: value
            for key, value in final.items()
            if key not in ("output_wheels", "archive")
        }
        == signed,
        "Assembly changed the signing manifest",
    )
    require(
        set(final["output_wheels"])
        == {wheel["filename"] for wheel in signed["wheels"]},
        "Unexpected final wheel map",
    )
    for filename, expected in final["output_wheels"].items():
        require(
            file_digest(Path("dist/wheels") / filename) == expected,
            "Final wheel digest mismatch",
        )
    verify_wheels(signed, Path("dist/wheels"))
    directory = Path(os.environ["RUNNER_TEMP"]) / "verified-binaries"
    directory.mkdir()
    for wheel in signed["wheels"]:
        with ZipFile(Path("dist/wheels") / wheel["filename"]) as source:
            for member, binary in wheel["replacements"].items():
                with (
                    source.open(member) as stream,
                    (directory / binary).open("xb") as output,
                ):
                    shutil.copyfileobj(stream, output)
    verify_archive(final)
    # Native signature/certificate verification runs separately on a fresh platform runner.


if __name__ == "__main__":
    {"prepare": prepare, "assemble": assemble, "verify": verify}[sys.argv[1]]()
