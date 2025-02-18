# /// script
# requires-python = ">=3.12"
# dependencies = [
#     "httpx>=0.28.1,<0.29",
#     "packaging>=24.1,<25",
# ]
# ///

"""Test `uv publish`.

Upload a new version of astral-test-<test case> to one of multiple indexes, exercising
different options of passing credentials.

Locally, execute the credentials setting script, then run:
```shell
uv run scripts/publish/test_publish.py local
```

# Setup

**pypi-token**
Set the `UV_TEST_PUBLISH_TOKEN` environment variables.

**pypi-password-env**
Set the `UV_TEST_PUBLISH_PASSWORD` environment variable.
This project also uses token authentication since it's the only thing that PyPI
supports, but they both CLI options.

**pypi-keyring**
```console
uv pip install keyring
keyring set https://test.pypi.org/legacy/?astral-test-keyring __token__
```
The query parameter a horrible hack stolen from
https://github.com/pypa/twine/issues/565#issue-555219267
to prevent the other projects from implicitly using the same credentials.

**pypi-trusted-publishing**
This one only works in GitHub Actions on astral-sh/uv in `ci.yml` - sorry!

**gitlab**
The username is astral-test-user, the password is a token.
Web: https://gitlab.com/astral-test-publish/astral-test-token/-/packages
Docs: https://docs.gitlab.com/ee/user/packages/pypi_repository/

**codeberg**
The username is astral-test-user, the password is a token (the actual account password
would also work).
Web: https://codeberg.org/astral-test-user/-/packages/pypi/astral-test-token/0.1.0
Docs: https://forgejo.org/docs/latest/user/packages/pypi/
"""

import os
import re
import shutil
import sys
import time
from argparse import ArgumentParser
from dataclasses import dataclass
from pathlib import Path
from shutil import rmtree
from subprocess import PIPE, check_call, run
from time import sleep

import httpx
from packaging.utils import (
    InvalidSdistFilename,
    parse_sdist_filename,
    parse_wheel_filename,
)
from packaging.version import Version

TEST_PYPI_PUBLISH_URL = "https://test.pypi.org/legacy/"
PYTHON_VERSION = os.environ.get("UV_TEST_PUBLISH_PYTHON_VERSION", "3.12")
# `pyproject.toml` contents using all supported metadata fields, except for the
# generated header with `[project]`, name and version.
PYPROJECT_TAIL = """
authors = [{ name = "konstin", email = "konstin@mailbox.org" }]
classifiers = ["Topic :: Software Development :: Testing"]
# Empty for simplicity with the `uv compile` check, anyio still tests,
# optional-dependencies still test the `Requires-Dist` field.
dependencies = []
description = "Add your description here"
dynamic = ["gui-scripts", "scripts"]
keywords = ["test", "publish"]
license = "MIT OR Apache-2.0"
license-files = ["LICENSE*"]
maintainers = [{ name = "konstin", email = "konstin@mailbox.org" }]
optional-dependencies = { "async" = ["anyio>=4,<5"] }
readme = "README.md"
requires-python = ">=3.12"
urls = { "github" = "https://github.com/astral-sh/uv" }

# https://github.com/pypa/hatch/issues/1828
[build-system]
requires = ["pdm-backend"]
build-backend = "pdm.backend"
""".lstrip()

cwd = Path(__file__).parent


@dataclass
class TargetConfiguration:
    project_name: str
    publish_url: str
    index_url: str
    index: str | None = None

    def index_declaration(self) -> str | None:
        if not self.index:
            return None
        return (
            "[[tool.uv.index]]\n"
            + f'name = "{self.index}"\n'
            + f'url = "{self.index_url}"\n'
            + f'publish-url = "{self.publish_url}"\n'
        )


# Map CLI target name to package name and index url.
# Trusted publishing can only be tested on GitHub Actions, so we have separate local
# and all targets.
local_targets: dict[str, TargetConfiguration] = {
    "pypi-token": TargetConfiguration(
        "astral-test-token",
        TEST_PYPI_PUBLISH_URL,
        "https://test.pypi.org/simple/",
        "test-pypi",
    ),
    "pypi-password-env": TargetConfiguration(
        "astral-test-password",
        TEST_PYPI_PUBLISH_URL,
        "https://test.pypi.org/simple/",
    ),
    "pypi-keyring": TargetConfiguration(
        "astral-test-keyring",
        "https://test.pypi.org/legacy/?astral-test-keyring",
        "https://test.pypi.org/simple/",
    ),
    "gitlab": TargetConfiguration(
        "astral-test-token",
        "https://gitlab.com/api/v4/projects/61853105/packages/pypi",
        "https://gitlab.com/api/v4/projects/61853105/packages/pypi/simple/",
    ),
    "codeberg": TargetConfiguration(
        "astral-test-token",
        "https://codeberg.org/api/packages/astral-test-user/pypi",
        "https://codeberg.org/api/packages/astral-test-user/pypi/simple/",
    ),
    "cloudsmith": TargetConfiguration(
        "astral-test-token",
        "https://python.cloudsmith.io/astral-test/astral-test-1/",
        "https://dl.cloudsmith.io/public/astral-test/astral-test-1/python/simple/",
    ),
}
all_targets: dict[str, TargetConfiguration] = local_targets | {
    "pypi-trusted-publishing": TargetConfiguration(
        "astral-test-trusted-publishing",
        TEST_PYPI_PUBLISH_URL,
        "https://test.pypi.org/simple/",
    )
}


def get_latest_version(project_name: str, client: httpx.Client) -> Version:
    """Return the latest version on all indexes of the package."""
    # To keep the number of packages small we reuse them across targets, so we have to
    # pick a version that doesn't exist on any target yet
    versions = set()
    for target_config in all_targets.values():
        if target_config.project_name != project_name:
            continue
        url = target_config.index_url + project_name + "/"

        # Get with retries
        error = None
        for _ in range(5):
            try:
                versions.update(collect_versions(url, client))
                break
            except httpx.HTTPError as err:
                error = err
                print(f"Error getting version, sleeping for 1s: {err}", file=sys.stderr)
                time.sleep(1)
            except InvalidSdistFilename as err:
                # Sometimes there's a link that says "status page"
                error = err
                print(f"Invalid index page, sleeping for 1s: {err}", file=sys.stderr)
                time.sleep(1)
        else:
            raise RuntimeError(f"Failed to fetch {url}") from error
    return max(versions)


def get_new_version(latest_version: Version) -> Version:
    """Bump the path version to obtain an empty version."""
    release = list(latest_version.release)
    release[-1] += 1
    return Version(".".join(str(i) for i in release))


def collect_versions(url: str, client: httpx.Client) -> set[Version]:
    """Return all version from an index page."""
    versions = set()
    for filename in get_filenames(url, client):
        if filename.endswith(".whl"):
            [_name, version, _build, _tags] = parse_wheel_filename(filename)
        else:
            [_name, version] = parse_sdist_filename(filename)
        versions.add(version)
    return versions


def get_filenames(url: str, client: httpx.Client) -> list[str]:
    """Get the filenames (source dists and wheels) from an index URL."""
    response = client.get(url)
    data = response.text
    # Works for the indexes in the list
    href_text = r"<a(?: +[\w-]+=(?:'[^']+'|\"[^\"]+\"))* *>([^<>]+)</a>"
    return [m.group(1) for m in re.finditer(href_text, data)]


def build_project_at_version(
    target: str, version: Version, uv: Path, modified: bool = False
) -> Path:
    """Build a source dist and a wheel with the project name and an unclaimed
    version."""
    project_name = all_targets[target].project_name

    if modified:
        dir_name = f"{project_name}-modified"
    else:
        dir_name = project_name
    project_root = cwd.joinpath(dir_name)

    if project_root.exists():
        rmtree(project_root)
    check_call(
        [uv, "init", "-p", PYTHON_VERSION, "--lib", "--name", project_name, dir_name],
        cwd=cwd,
    )
    toml = (
        "[project]\n"
        + f'name = "{project_name}"\n'
        # Set to an unclaimed version
        + f'version = "{version}"\n'
        # Add all supported metadata
        + PYPROJECT_TAIL
    )
    if index_declaration := all_targets[target].index_declaration():
        toml += index_declaration

    project_root.joinpath("pyproject.toml").write_text(toml)
    shutil.copy(
        cwd.parent.parent.joinpath("LICENSE-APACHE"),
        cwd.joinpath(dir_name).joinpath("LICENSE-APACHE"),
    )
    shutil.copy(
        cwd.parent.parent.joinpath("LICENSE-MIT"),
        cwd.joinpath(dir_name).joinpath("LICENSE-MIT"),
    )

    # Modify the code so we get a different source dist and wheel
    if modified:
        init_py = (
            project_root.joinpath("src")
            # dist info naming
            .joinpath(project_name.replace("-", "_"))
            .joinpath("__init__.py")
        )
        init_py.write_text("x = 1")

    # Build the project
    check_call([uv, "build"], cwd=project_root)
    # Test that we ignore unknown any file.
    project_root.joinpath("dist").joinpath(".DS_Store").touch()

    return project_root


def wait_for_index(
    index_url: str,
    project_name: str,
    version: Version,
    uv: Path,
):
    """Check that the index URL was updated, wait up to 100s if necessary.

    Often enough the index takes a few seconds until the index is updated after an
    upload. We need to specifically run this through uv since to query the same cache
    (invalidation) as the registry client in skip existing in uv publish will later,
    just `get_filenames` fails non-deterministically.
    """
    for _ in range(50):
        result = run(
            [
                uv,
                "pip",
                "compile",
                "-p",
                PYTHON_VERSION,
                "--index",
                index_url,
                "--quiet",
                "--generate-hashes",
                "--no-header",
                "--refresh-package",
                project_name,
                "-",
            ],
            text=True,
            input=f"{project_name}",
            stdout=PIPE,
        )
        # codeberg sometimes times out
        if result.returncode != 0:
            print(
                f"uv pip compile not updated, missing 2 files for {version}, "
                + f"sleeping for 2s: `{index_url}`:\n",
                file=sys.stderr,
            )
            sleep(2)
            continue

        if (
            f"{project_name}=={version}" in result.stdout
            and result.stdout.count("--hash") == 2
        ):
            break

        print(
            f"uv pip compile not updated, missing 2 files for {version}, "
            + f"sleeping for 2s: `{index_url}`:\n"
            + "```\n"
            + result.stdout.replace("\\\n    ", "")
            + "```",
            file=sys.stderr,
        )
        sleep(2)


def publish_project(target: str, uv: Path, client: httpx.Client):
    """Test that:

    1. An upload with a fresh version succeeds.
    2. If we're using PyPI, uploading the same files again succeeds.
    3. Check URL works and reports the files as skipped.
    """
    project_name = all_targets[target].project_name

    # If a version was recently uploaded by another run of this script,
    # `get_latest_version` may get a cached version and uploading fails. In this case
    # we wait and try again.
    retries = 3
    while True:
        print(f"\nPublish {project_name} for {target}", file=sys.stderr)

        # The distributions are build to the dist directory of the project.
        previous_version = get_latest_version(project_name, client)
        version = get_new_version(previous_version)
        project_dir = build_project_at_version(target, version, uv)

        # Upload configuration
        publish_url = all_targets[target].publish_url
        index_url = all_targets[target].index_url
        env, extra_args = target_configuration(target)
        env = {**os.environ, **env}
        expected_filenames = [
            path.name for path in project_dir.joinpath("dist").iterdir()
        ]
        # Ignore the gitignore file in dist
        expected_filenames.remove(".gitignore")
        # Ignore our test file
        expected_filenames.remove(".DS_Store")

        print(
            f"\n=== 1. Publishing a new version: {project_name} {version} {publish_url} ===",
            file=sys.stderr,
        )

        args = [uv, "publish", "--publish-url", publish_url, *extra_args]
        result = run(args, cwd=project_dir, env=env, text=True, stderr=PIPE)
        if result.returncode == 0:
            # Successful upload
            break

        retries -= 1
        if retries > 0:
            print(
                f"Publish failed, retrying after 10s:\n---\n{result.stderr}\n---",
                file=sys.stderr,
            )
            sleep(10)
        else:
            # Raise the error after three failures
            result.check_returncode()

    if publish_url == TEST_PYPI_PUBLISH_URL:
        # Confirm pypi behaviour: Uploading the same file again is fine.
        print(
            f"\n=== 2. Publishing {project_name} {version} again (PyPI) ===",
            file=sys.stderr,
        )
        wait_for_index(index_url, project_name, version, uv)
        args = [uv, "publish", "--publish-url", publish_url, *extra_args]
        output = run(
            args, cwd=project_dir, env=env, text=True, check=True, stderr=PIPE
        ).stderr
        if (
            output.count("Uploading") != len(expected_filenames)
            or output.count("already exists") != 0
        ):
            raise RuntimeError(
                f"PyPI re-upload of the same files failed: "
                f"{output.count('Uploading')} != {len(expected_filenames)}, "
                f"{output.count('already exists')} != 0\n"
                f"---\n{output}\n---"
            )

    mode = "index" if all_targets[target].index else "check URL"
    print(
        f"\n=== 3. Publishing {project_name} {version} again with {mode} ===",
        file=sys.stderr,
    )
    wait_for_index(index_url, project_name, version, uv)
    # Test twine-style and index-style uploads for different packages.
    if index := all_targets[target].index:
        args = [
            uv,
            "publish",
            "--index",
            index,
            *extra_args,
        ]
    else:
        args = [
            uv,
            "publish",
            "--publish-url",
            publish_url,
            "--check-url",
            index_url,
            *extra_args,
        ]
    output = run(
        args, cwd=project_dir, env=env, text=True, check=True, stderr=PIPE
    ).stderr

    if output.count("Uploading") != 0 or output.count("already exists") != len(
        expected_filenames
    ):
        raise RuntimeError(
            f"Re-upload with check URL failed: "
            f"{output.count('Uploading')} != 0, "
            f"{output.count('already exists')} != {len(expected_filenames)}\n"
            f"---\n{output}\n---"
        )

    # Build a different source dist and wheel at the same version, so the upload fails
    del project_dir
    modified_project_dir = build_project_at_version(target, version, uv, modified=True)

    print(
        f"\n=== 4. Publishing modified {project_name} {version} "
        f"again with skip existing (error test) ===",
        file=sys.stderr,
    )
    wait_for_index(index_url, project_name, version, uv)
    args = [
        uv,
        "publish",
        "--publish-url",
        publish_url,
        "--check-url",
        index_url,
        *extra_args,
    ]
    result = run(args, cwd=modified_project_dir, env=env, text=True, stderr=PIPE)

    if (
        result.returncode == 0
        or "Local file and index file do not match for" not in result.stderr
    ):
        raise RuntimeError(
            f"Re-upload with mismatching files should not have been started: "
            f"Exit code {result.returncode}\n"
            f"---\n{result.stderr}\n---"
        )


def target_configuration(target: str) -> tuple[dict[str, str], list[str]]:
    if target == "pypi-token":
        extra_args = []
        env = {"UV_PUBLISH_TOKEN": os.environ["UV_TEST_PUBLISH_TOKEN"]}
    elif target == "pypi-password-env":
        extra_args = ["--username", "__token__"]
        env = {"UV_PUBLISH_PASSWORD": os.environ["UV_TEST_PUBLISH_PASSWORD"]}
    elif target == "pypi-keyring":
        extra_args = ["--username", "__token__", "--keyring-provider", "subprocess"]
        env = {}
    elif target == "pypi-trusted-publishing":
        extra_args = ["--trusted-publishing", "always"]
        env = {}
    elif target == "gitlab":
        env = {"UV_PUBLISH_PASSWORD": os.environ["UV_TEST_PUBLISH_GITLAB_PAT"]}
        extra_args = ["--username", "astral-test-gitlab-pat"]
    elif target == "codeberg":
        extra_args = []
        env = {
            "UV_PUBLISH_USERNAME": "astral-test-user",
            "UV_PUBLISH_PASSWORD": os.environ["UV_TEST_PUBLISH_CODEBERG_TOKEN"],
        }
    elif target == "cloudsmith":
        extra_args = []
        env = {
            "UV_PUBLISH_TOKEN": os.environ["UV_TEST_PUBLISH_CLOUDSMITH_TOKEN"],
        }
    else:
        raise ValueError(f"Unknown target: {target}")
    return env, extra_args


def main():
    parser = ArgumentParser()
    target_choices = [*all_targets, "local", "all"]
    parser.add_argument("targets", choices=target_choices, nargs="+")
    parser.add_argument("--uv")
    args = parser.parse_args()

    if args.uv:
        # We change the working directory for the subprocess calls, so we have to
        # absolutize the path.
        uv = Path.cwd().joinpath(args.uv)
    else:
        check_call(["cargo", "build"])
        executable_suffix = ".exe" if os.name == "nt" else ""
        uv = cwd.parent.parent.joinpath(f"target/debug/uv{executable_suffix}")

    if args.targets == ["local"]:
        targets = list(local_targets)
    elif args.targets == ["all"]:
        targets = list(all_targets)
    else:
        targets = args.targets

    with httpx.Client(timeout=120) as client:
        for project_name in targets:
            publish_project(project_name, uv, client)


if __name__ == "__main__":
    main()
