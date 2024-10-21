# /// script
# requires-python = ">=3.12"
# dependencies = [
#     "httpx>=0.27.2,<0.28",
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
import time
from argparse import ArgumentParser
from pathlib import Path
from shutil import rmtree
from subprocess import PIPE, check_call, check_output, run
from time import sleep

import httpx
from packaging.utils import parse_sdist_filename, parse_wheel_filename
from packaging.version import Version

TEST_PYPI_PUBLISH_URL = "https://test.pypi.org/legacy/"

cwd = Path(__file__).parent

# Map CLI target name to package name and index url.
# Trusted publishing can only be tested on GitHub Actions, so we have separate local
# and all targets.
local_targets: dict[str, tuple[str, str]] = {
    "pypi-token": ("astral-test-token", "https://test.pypi.org/simple/"),
    "pypi-password-env": ("astral-test-password", "https://test.pypi.org/simple/"),
    "pypi-keyring": ("astral-test-keyring", "https://test.pypi.org/simple/"),
    "gitlab": (
        "astral-test-token",
        "https://gitlab.com/api/v4/projects/61853105/packages/pypi/simple/",
    ),
    "codeberg": (
        "astral-test-token",
        "https://codeberg.org/api/packages/astral-test-user/pypi/simple/",
    ),
    "cloudsmith": (
        "astral-test-token",
        "https://dl.cloudsmith.io/public/astral-test/astral-test-1/python/simple/",
    ),
}
all_targets: dict[str, tuple[str, str]] = local_targets | {
    "pypi-trusted-publishing": (
        "astral-test-trusted-publishing",
        "https://test.pypi.org/simple/",
    )
}


def get_new_version(project_name: str, client: httpx.Client) -> Version:
    """Return the next free patch version on all indexes of the package."""
    # To keep the number of packages small we reuse them across targets, so we have to
    # pick a version that doesn't exist on any target yet
    versions = set()
    for project_name_, index_url in all_targets.values():
        if project_name_ != project_name:
            continue
        for filename in get_filenames((index_url + project_name + "/"), client):
            if filename.endswith(".whl"):
                [_name, version, _build, _tags] = parse_wheel_filename(filename)
            else:
                [_name, version] = parse_sdist_filename(filename)
            versions.add(version)
    max_version = max(versions)

    # Bump the path version to obtain an empty version
    release = list(max_version.release)
    release[-1] += 1
    return Version(".".join(str(i) for i in release))


def get_filenames(url: str, client: httpx.Client) -> list[str]:
    """Get the filenames (source dists and wheels) from an index URL."""
    # Get with retries
    error = None
    for _ in range(5):
        try:
            response = client.get(url)
            data = response.text
            break
        except httpx.HTTPError as err:
            error = err
            print(f"Error getting version, sleeping for 1s: {err}")
            time.sleep(1)
    else:
        raise RuntimeError(f"Failed to fetch {url}") from error
    # Works for the indexes in the list
    href_text = r"<a(?: +[\w-]+=(?:'[^']+'|\"[^\"]+\"))* *>([^<>]+)</a>"
    return [m.group(1) for m in re.finditer(href_text, data)]


def build_new_version(project_name: str, uv: Path, client: httpx.Client) -> Version:
    """Build a source dist and a wheel with the project name and an unclaimed
    version."""
    if cwd.joinpath(project_name).exists():
        rmtree(cwd.joinpath(project_name))
    check_call([uv, "init", "--lib", project_name], cwd=cwd)
    pyproject_toml = cwd.joinpath(project_name).joinpath("pyproject.toml")

    # Set to an unclaimed version
    toml = pyproject_toml.read_text()
    new_version = get_new_version(project_name, client)
    toml = re.sub('version = ".*"', f'version = "{new_version}"', toml)
    pyproject_toml.write_text(toml)

    # Build the project
    check_call([uv, "build"], cwd=cwd.joinpath(project_name))

    return new_version


def wait_for_index(index_url: str, project_name: str, version: Version, uv: Path):
    """Check that the index URL was updated, wait up to 10s if necessary.

    Often enough the index takes a few seconds until the index is updated after an
    upload. We need to specifically run this through uv since to query the same cache
    (invalidation) as the registry client in skip existing in uv publish will later,
    just `get_filenames` fails non-deterministically.
    """
    for _ in range(10):
        output = check_output(
            [
                uv,
                "pip",
                "compile",
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
            input=project_name,
        )
        if f"{project_name}=={version}" in output and output.count("--hash") == 2:
            break

        print(
            f"uv pip compile not updated, missing 2 files for {version}: `{output.replace("\\\n    ", "")}`, "
            f"sleeping for 1s: `{index_url}`"
        )
        sleep(1)


def publish_project(target: str, uv: Path, client: httpx.Client):
    """Test that:

    1. An upload with a fresh version succeeds.
    2. If we're using PyPI, uploading the same files again succeeds.
    3. Check URL works and reports the files as skipped.
    """
    project_name = all_targets[target][0]

    print(f"\nPublish {project_name} for {target}")

    # The distributions are build to the dist directory of the project.
    version = build_new_version(project_name, uv, client)

    # Upload configuration
    env, extra_args, publish_url = target_configuration(target, client)
    index_url = all_targets[target][1]
    env = {**os.environ, **env}
    uv_cwd = cwd.joinpath(project_name)
    expected_filenames = [path.name for path in uv_cwd.joinpath("dist").iterdir()]
    # Ignore the gitignore file in dist
    expected_filenames.remove(".gitignore")

    print(
        f"\n=== 1. Publishing a new version: {project_name} {version} {publish_url} ==="
    )
    args = [uv, "publish", "--publish-url", publish_url, *extra_args]
    check_call(args, cwd=uv_cwd, env=env)

    if publish_url == TEST_PYPI_PUBLISH_URL:
        # Confirm pypi behaviour: Uploading the same file again is fine.
        print(f"\n=== 2. Publishing {project_name} {version} again (PyPI) ===")
        wait_for_index(index_url, project_name, version, uv)
        args = [uv, "publish", "-v", "--publish-url", publish_url, *extra_args]
        output = run(
            args, cwd=uv_cwd, env=env, text=True, check=True, stderr=PIPE
        ).stderr
        if (
            output.count("Uploading") != len(expected_filenames)
            or output.count("already exists") != 0
        ):
            raise RuntimeError(
                f"PyPI re-upload of the same files failed: "
                f"{output.count("Uploading")}, {output.count("already exists")}\n"
                f"---\n{output}\n---"
            )

    print(f"\n=== 3. Publishing {project_name} {version} again with check URL ===")
    wait_for_index(index_url, project_name, version, uv)
    args = [
        uv,
        "publish",
        "-v",
        "--publish-url",
        publish_url,
        "--check-url",
        index_url,
        *extra_args,
    ]
    output = run(args, cwd=uv_cwd, env=env, text=True, check=True, stderr=PIPE).stderr

    if output.count("Uploading") != 0 or output.count("already exists") != len(
        expected_filenames
    ):
        raise RuntimeError(
            f"Re-upload with check URL failed: "
            f"{output.count("Uploading")}, {output.count("already exists")}\n"
            f"---\n{output}\n---"
        )


def target_configuration(
    target: str, client: httpx.Client
) -> tuple[dict[str, str], list[str], str]:
    if target == "pypi-token":
        publish_url = TEST_PYPI_PUBLISH_URL
        extra_args = []
        env = {"UV_PUBLISH_TOKEN": os.environ["UV_TEST_PUBLISH_TOKEN"]}
    elif target == "pypi-password-env":
        publish_url = TEST_PYPI_PUBLISH_URL
        extra_args = ["--username", "__token__"]
        env = {"UV_PUBLISH_PASSWORD": os.environ["UV_TEST_PUBLISH_PASSWORD"]}
    elif target == "pypi-keyring":
        publish_url = "https://test.pypi.org/legacy/?astral-test-keyring"
        extra_args = ["--username", "__token__", "--keyring-provider", "subprocess"]
        env = {}
    elif target == "pypi-trusted-publishing":
        publish_url = TEST_PYPI_PUBLISH_URL
        extra_args = ["--trusted-publishing", "always"]
        env = {}
    elif target == "gitlab":
        env = {"UV_PUBLISH_PASSWORD": os.environ["UV_TEST_PUBLISH_GITLAB_PAT"]}
        publish_url = "https://gitlab.com/api/v4/projects/61853105/packages/pypi"
        extra_args = ["--username", "astral-test-gitlab-pat"]
    elif target == "codeberg":
        publish_url = "https://codeberg.org/api/packages/astral-test-user/pypi"
        extra_args = []
        env = {
            "UV_PUBLISH_USERNAME": "astral-test-user",
            "UV_PUBLISH_PASSWORD": os.environ["UV_TEST_PUBLISH_CODEBERG_TOKEN"],
        }
    elif target == "cloudsmith":
        publish_url = "https://python.cloudsmith.io/astral-test/astral-test-1/"
        extra_args = []
        env = {
            "UV_PUBLISH_TOKEN": os.environ["UV_TEST_PUBLISH_CLOUDSMITH_TOKEN"],
        }
    else:
        raise ValueError(f"Unknown target: {target}")
    return env, extra_args, publish_url


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
