# /// script
# requires-python = ">=3.12"
# dependencies = [
#     "httpx[socks]>=0.28.1,<0.29",
#     "keyring",
#     "keyrings-alt",
#     "packaging>=24.1,<25",
#     "pypi-attestations>=0.0.28",
#     "sigstore>=4.4.0",
# ]
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///

"""Test `uv publish`.

Upload a new version of astral-test-<test case> to one of multiple indexes, exercising
different options of passing credentials.

Locally, execute the credentials setting script, then run:
```shell
uv run --locked scripts/publish/test_publish.py local
```

# Setup

**pypi-token**
Set the `UV_TEST_PUBLISH_TOKEN` environment variable.

**pypi-password-env**
Set the `UV_TEST_PUBLISH_PASSWORD` environment variable.
This project also uses token authentication since it's the only thing that PyPI
supports, but this tests the username and password CLI options.

**pypi-keyring**
Set `UV_TEST_PUBLISH_KEYRING` to the dedicated TestPyPI token. The harness stores it
in a temporary keyring.
The query parameter is a horrible hack stolen from
https://github.com/pypa/twine/issues/565#issue-555219267
to prevent the other projects from implicitly using the same credentials.

**pypi-text-store**
```console
uv auth login https://test.pypi.org/legacy/?astral-test-text-store --token <token>
```
The query parameter is a horrible hack stolen from
https://github.com/pypa/twine/issues/565#issue-555219267
to prevent the other projects from implicitly using the same credentials.

**pypi-trusted-publishing-github**
This one only works in GitHub Actions on astral-sh/uv in `ci.yml` - sorry!

**pypi-trusted-publishing-gitlab**
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

import logging
import os
import shlex
import shutil
import sys
import time
import traceback
from argparse import ArgumentParser
from collections.abc import Iterator
from contextlib import ExitStack, redirect_stderr, redirect_stdout
from dataclasses import dataclass
from pathlib import Path
from subprocess import CalledProcessError, CompletedProcess, check_call, run
from tempfile import SpooledTemporaryFile, TemporaryDirectory, gettempdir
from typing import IO

import httpx
from keyrings.alt.file import PlaintextKeyring
from packaging.version import Version
from pypi_attestations import Attestation, Distribution
from sigstore import oidc
from sigstore.models import ClientTrustConfig
from sigstore.sign import SigningContext

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

SCRIPT_DIR = Path(__file__).parent
REPOSITORY_ROOT = SCRIPT_DIR.parent.parent


@dataclass(frozen=True, slots=True)
class Target:
    """Configuration for a publish test target."""

    project_name: str
    publish_url: str
    index_url: str
    index: str | None = None
    publish_args: tuple[str, ...] = ()
    environment: tuple[tuple[str, str], ...] = ()
    secrets: tuple[tuple[str, str], ...] = ()
    keyring_variable: str | None = None
    attestations: bool = False
    reusable_credentials: bool = True
    local: bool = True
    ci: bool = True

    def index_declaration(self) -> str | None:
        if not self.index:
            return None
        return (
            "[[tool.uv.index]]\n"
            + f'name = "{self.index}"\n'
            + f'url = "{self.index_url}"\n'
            + f'publish-url = "{self.publish_url}"\n'
        )


@dataclass(frozen=True, slots=True)
class BuiltProject:
    """A built publish fixture."""

    root: Path
    version: Version
    filenames: tuple[str, ...]


# Map each CLI target to its registry, credentials, and test capabilities.
TARGETS: dict[str, Target] = {
    "pypi-token": Target(
        "astral-test-token",
        TEST_PYPI_PUBLISH_URL,
        "https://test.pypi.org/simple/",
        index="test-pypi",
        secrets=(("UV_PUBLISH_TOKEN", "UV_TEST_PUBLISH_TOKEN"),),
    ),
    "pypi-password-env": Target(
        "astral-test-password",
        TEST_PYPI_PUBLISH_URL,
        "https://test.pypi.org/simple/",
        publish_args=("--username", "__token__"),
        secrets=(("UV_PUBLISH_PASSWORD", "UV_TEST_PUBLISH_PASSWORD"),),
    ),
    "pypi-keyring": Target(
        "astral-test-keyring",
        "https://test.pypi.org/legacy/?astral-test-keyring",
        "https://test.pypi.org/simple/",
        publish_args=("--username", "__token__", "--keyring-provider", "subprocess"),
        keyring_variable="UV_TEST_PUBLISH_KEYRING",
    ),
    "pypi-text-store": Target(
        "astral-test-text-store",
        "https://test.pypi.org/legacy/?astral-test-text-store",
        "https://test.pypi.org/simple/",
        publish_args=("--username", "__token__"),
    ),
    "gitlab": Target(
        "astral-test-token",
        "https://gitlab.com/api/v4/projects/61853105/packages/pypi",
        "https://gitlab.com/api/v4/projects/61853105/packages/pypi/simple/",
        publish_args=("--username", "astral-test-gitlab-pat"),
        secrets=(("UV_PUBLISH_PASSWORD", "UV_TEST_PUBLISH_GITLAB_PAT"),),
    ),
    "codeberg": Target(
        "astral-test-token",
        "https://codeberg.org/api/packages/astral-test-user/pypi",
        "https://codeberg.org/api/packages/astral-test-user/pypi/simple/",
        environment=(("UV_PUBLISH_USERNAME", "astral-test-user"),),
        secrets=(("UV_PUBLISH_PASSWORD", "UV_TEST_PUBLISH_CODEBERG_TOKEN"),),
        # Temporarily disabled on CI due to unreliability.
        ci=False,
    ),
    "cloudsmith": Target(
        "astral-test-token",
        "https://python.cloudsmith.io/astral-test/astral-test-1/",
        "https://dl.cloudsmith.io/public/astral-test/astral-test-1/python/simple/",
        secrets=(("UV_PUBLISH_TOKEN", "UV_TEST_PUBLISH_CLOUDSMITH_TOKEN"),),
    ),
    "pypi-trusted-publishing-github": Target(
        "astral-test-trusted-publishing",
        TEST_PYPI_PUBLISH_URL,
        "https://test.pypi.org/simple/",
        index=None,
        publish_args=("--trusted-publishing", "always"),
        attestations=True,
        local=False,
    ),
    "pypi-trusted-publishing-gitlab": Target(
        "astral-test-pypi-trusted-publishing-gitlab",
        publish_url=TEST_PYPI_PUBLISH_URL,
        index_url="https://test.pypi.org/simple/",
        index=None,
        publish_args=("--trusted-publishing", "always"),
        environment=(
            ("CI", "true"),
            ("GITLAB_CI", "true"),
            # The test can run in GitHub Actions, so explicitly disable detection.
            ("GITHUB_ACTIONS", "false"),
        ),
        secrets=(("TESTPYPI_ID_TOKEN", "UV_TEST_PUBLISH_GITLAB_PYPI_OIDC_TOKEN"),),
        # We're impersonating GitLab, so attestations remain disabled.
        # TODO: In principle we could test this by having GitLab issue us an `aud:sigstore`
        # OIDC token in addition to the `aud:testpypi` one.
        reusable_credentials=False,
        local=False,
    ),
}

LOCAL_TARGETS = tuple(name for name, target in TARGETS.items() if target.local)
CI_TARGETS = tuple(name for name, target in TARGETS.items() if target.ci)


class TargetSession:
    """Manage credentials, output, and subprocesses for one target."""

    def __init__(self, name: str, uv: Path, keyring_directory: Path):
        self.name = name
        self.uv = uv
        self.target = TARGETS[name]
        self.keyring_directory = keyring_directory
        self.failed = False

        self._environment: dict[str, str] | None = None
        self._grouped = os.environ.get("GITHUB_ACTIONS", "").lower() == "true"
        self._redirects = ExitStack()
        self._output_stack = ExitStack()
        self._root_logger = logging.getLogger()
        self._previous_handlers: list[logging.Handler] = []
        self._previous_level = self._root_logger.level
        self._handler: logging.Handler | None = None
        self.output: IO[str]

    def __enter__(self):
        self.output = self._output_stack.enter_context(
            SpooledTemporaryFile(
                max_size=1024 * 1024,
                mode="w+",
                encoding="utf-8",
            )
        )
        if self._grouped:
            print(f"::group::uv publish: {self.name}", flush=True)
        else:
            print(f"Testing uv publish target: {self.name}", flush=True)

        self._previous_handlers = self._root_logger.handlers[:]
        for handler in self._previous_handlers:
            self._root_logger.removeHandler(handler)

        self._handler = logging.StreamHandler(self.output)
        self._handler.setFormatter(
            logging.Formatter(
                "%(levelname)s [%(asctime)s] %(name)s - %(message)s",
                datefmt="%Y-%m-%d %H:%M:%S",
            )
        )
        self._root_logger.addHandler(self._handler)
        self._root_logger.setLevel(logging.INFO)
        self._redirects.enter_context(redirect_stdout(self.output))
        self._redirects.enter_context(redirect_stderr(self.output))
        return self

    def __exit__(self, _exception_type, exception, _exception_traceback) -> bool:
        if exception is not None:
            traceback.print_exception(exception, file=self.output)
            self.failed = True

        if self._handler is not None:
            self._handler.flush()
            self._root_logger.removeHandler(self._handler)
        self._root_logger.setLevel(self._previous_level)
        for handler in self._previous_handlers:
            self._root_logger.addHandler(handler)
        self._redirects.close()

        if self.failed:
            print(f"Target failed: {self.name}", flush=True)
            self.output.flush()
            self.output.seek(0)
            shutil.copyfileobj(self.output, sys.stdout)
            sys.stdout.flush()
        else:
            print(f"Target passed: {self.name}", flush=True)

        self._output_stack.close()
        if self._grouped:
            print("::endgroup::", flush=True)

        return exception is not None and isinstance(exception, Exception)

    def run_command(
        self,
        command: list[str | Path],
        *,
        cwd: str | Path,
        input: str | None = None,
        check: bool = True,
    ) -> CompletedProcess[str]:
        """Run and record a subprocess without streaming its output."""
        self.output.write(f"$ {shlex.join(str(argument) for argument in command)}\n")
        try:
            result = run(
                command,
                cwd=cwd,
                env=self.full_environment(),
                text=True,
                input=input,
                capture_output=True,
                check=check,
            )
        except CalledProcessError as error:
            self._write_process_output(error.stdout)
            self._write_process_output(error.stderr)
            raise

        self._write_process_output(result.stdout)
        self._write_process_output(result.stderr)
        return result

    def publish(
        self,
        project: BuiltProject,
        destination: tuple[str, ...],
        *,
        check: bool = True,
    ) -> CompletedProcess[str]:
        """Run uv publish with the target's credentials and arguments."""
        return self.run_command(
            [self.uv, "publish", *destination, *self.target.publish_args],
            cwd=project.root,
            check=check,
        )

    def full_environment(self) -> dict[str, str]:
        """Return the process environment, resolving credentials on first use."""
        if self._environment is None:
            self._environment = dict(self.target.environment)
            for variable, source_variable in self.target.secrets:
                self._environment[variable] = os.environ[source_variable]

            if self.target.keyring_variable:
                keyring_file = str(self.keyring_directory / "keyring.cfg")
                keyring = PlaintextKeyring().with_properties(file_path=keyring_file)
                keyring.set_password(
                    self.target.publish_url,
                    "__token__",
                    os.environ[self.target.keyring_variable],
                )
                self._environment.update(
                    {
                        "PYTHON_KEYRING_BACKEND": (
                            "keyrings.alt.file.PlaintextKeyring"
                        ),
                        "KEYRING_PROPERTY_FILE_PATH": keyring_file,
                    }
                )

        return {**os.environ, **self._environment}

    def _write_process_output(self, content: str | None):
        if not content:
            return
        self.output.write(content)
        if not content.endswith("\n"):
            self.output.write("\n")


class PublishTest:
    """Exercise fresh and repeated uploads for one publish target."""

    def __init__(self, session: TargetSession):
        self.session = session
        self.target = session.target

    def run(self):
        """Run the publish scenarios in dependency order."""
        project = self._publish_fresh()
        self._verify_same_file_reupload(project)
        self._verify_existing_files_skipped(project)
        self._verify_modified_files_rejected(project)

    def _publish_fresh(self) -> BuiltProject:
        """Publish a new project version and verify its attestations."""
        project_name = self.target.project_name

        print(f"\nPublish {project_name} for {self.session.name}", file=sys.stderr)

        project = self._build_project(self._fresh_version())

        if self.target.attestations:
            self._create_attestations(project)

        print(
            f"\n=== 1. Publishing a new version: "
            f"{project_name} {project.version} {self.target.publish_url} ===",
            file=sys.stderr,
        )

        self.session.publish(
            project,
            ("--publish-url", self.target.publish_url),
        )

        if self.target.attestations:
            self._wait_for_index(project.version)
            self._check_index_for_provenance(project)

        return project

    def _verify_same_file_reupload(self, project: BuiltProject):
        """Test that re-uploading the same files works on PyPI."""
        # PyPI is the only index known to have this behavior. GitLab Trusted
        # Publishing uses a static OIDC token that cannot be reused.
        if (
            self.target.publish_url != TEST_PYPI_PUBLISH_URL
            or not self.target.reusable_credentials
        ):
            return

        print(
            f"\n=== 2. Publishing {self.target.project_name} "
            f"{project.version} again (PyPI) ===",
            file=sys.stderr,
        )
        self._wait_for_index(project.version)
        result = self.session.publish(
            project,
            ("--publish-url", self.target.publish_url),
        )
        output = result.stderr or ""
        if (
            output.count("Uploading") != len(project.filenames)
            or output.count("already exists") != 0
        ):
            raise RuntimeError(
                f"PyPI re-upload of the same files failed for {self.session.name} "
                f"({self.target.publish_url}): "
                f"{output.count('Uploading')} != {len(project.filenames)}, "
                f"{output.count('already exists')} != 0\n"
                f"---\n{output}\n---"
            )

    def _verify_existing_files_skipped(self, project: BuiltProject):
        """Test that a check URL or index skips existing files."""
        if not self.target.reusable_credentials:
            return

        mode = "index" if self.target.index else "check URL"
        print(
            f"\n=== 3. Publishing {self.target.project_name} "
            f"{project.version} again with {mode} ===",
            file=sys.stderr,
        )
        if index := self.target.index:
            destination = ("--index", index)
        else:
            destination = (
                "--publish-url",
                self.target.publish_url,
                "--check-url",
                self.target.index_url,
            )

        output = ""
        for _ in self._index_attempts(project.version, "check URL upload"):
            result = self.session.publish(project, destination)
            output = result.stderr or ""
            if output.count("Uploading") == 0 and output.count("already exists") == len(
                project.filenames
            ):
                return

        raise RuntimeError(
            f"Re-upload with check URL failed for {self.session.name} "
            f"({self.target.publish_url}): "
            f"{output.count('Uploading')} != 0, "
            f"{output.count('already exists')} != {len(project.filenames)}\n"
            f"---\n{output}\n---"
        )

    def _verify_modified_files_rejected(self, project: BuiltProject):
        """Test that modified files at the same version are rejected."""
        if not self.target.reusable_credentials:
            return

        modified_project = self._build_project(project.version, modified=True)

        print(
            f"\n=== 4. Publishing modified {self.target.project_name} "
            f"{project.version} again with skip existing (error test) ===",
            file=sys.stderr,
        )
        destination = (
            "--publish-url",
            self.target.publish_url,
            "--check-url",
            self.target.index_url,
        )
        returncode = 0
        output = ""
        for _ in self._index_attempts(project.version, "modified file check"):
            result = self.session.publish(
                modified_project,
                destination,
                check=False,
            )
            returncode = result.returncode
            output = result.stderr or ""

            if (
                returncode != 0
                and "Local file and index file do not match for" in output
            ):
                return

        raise RuntimeError(
            f"Re-upload with mismatching files should not have been started "
            f"for {self.session.name} ({self.target.publish_url}): "
            f"Exit code {returncode}\n"
            f"---\n{output}\n---"
        )

    def _build_project(self, version: Version, modified: bool = False) -> BuiltProject:
        """Build a source distribution and wheel at an unclaimed version."""
        project_name = self.target.project_name
        dir_name = f"{project_name}-modified" if modified else project_name
        project_root = SCRIPT_DIR / dir_name

        if project_root.exists():
            shutil.rmtree(project_root)
        self.session.run_command(
            [
                self.session.uv,
                "init",
                "-p",
                PYTHON_VERSION,
                "--lib",
                "--no-workspace",
                "--name",
                project_name,
                dir_name,
            ],
            cwd=SCRIPT_DIR,
        )
        toml = (
            "[project]\n"
            + f'name = "{project_name}"\n'
            + f'version = "{version}"\n'
            + PYPROJECT_TAIL
        )
        project_root.joinpath("pyproject.toml").write_text(toml)
        shutil.copy(
            REPOSITORY_ROOT / "LICENSE-APACHE",
            project_root / "LICENSE-APACHE",
        )
        shutil.copy(
            REPOSITORY_ROOT / "LICENSE-MIT",
            project_root / "LICENSE-MIT",
        )

        if modified:
            init_py = (
                project_root / "src" / project_name.replace("-", "_") / "__init__.py"
            )
            init_py.write_text("x = 1")

        self.session.run_command(
            [
                self.session.uv,
                "build",
                "--build-constraint",
                SCRIPT_DIR / "build-requirements.txt",
                "--require-hashes",
            ],
            cwd=project_root,
        )
        # Publication-only indexes must not participate in building fixtures.
        if index_declaration := self.target.index_declaration():
            project_root.joinpath("pyproject.toml").write_text(toml + index_declaration)

        dist = project_root / "dist"
        # Test that uv ignores unknown files in the distribution directory.
        dist.joinpath(".DS_Store").touch()

        return BuiltProject(
            root=project_root,
            version=version,
            filenames=tuple(
                path.name
                for path in dist.iterdir()
                if path.name.endswith((".tar.gz", ".whl"))
            ),
        )

    def _wait_for_index(self, version: Version):
        """Wait for the index to consistently expose both distributions."""
        consecutive_successes = 0
        for _ in range(50):
            result = self.session.run_command(
                [
                    self.session.uv,
                    "pip",
                    "compile",
                    "-p",
                    PYTHON_VERSION,
                    "--index",
                    self.target.index_url,
                    "--quiet",
                    "--generate-hashes",
                    "--no-header",
                    "--refresh-package",
                    self.target.project_name,
                    "-",
                ],
                input=f"{self.target.project_name}=={version}",
                # Avoid applying the repository's exclude-newer setting to a
                # version that was just published.
                cwd=gettempdir(),
                check=False,
            )
            if result.returncode != 0:
                consecutive_successes = 0
            elif (
                f"{self.target.project_name}=={version}" in result.stdout
                and result.stdout.count("--hash") == 2
            ):
                consecutive_successes += 1
                if consecutive_successes == 3:
                    return
            else:
                consecutive_successes = 0

            print(
                f"Index not ready for {self.target.project_name}=={version}; "
                f"sleeping for 2s: {self.target.index_url}",
                file=sys.stderr,
            )
            time.sleep(2)

        raise RuntimeError(
            f"Index did not consistently expose both files for "
            f"{self.target.project_name}=={version}"
        )

    def _index_attempts(self, version: Version, operation: str) -> Iterator[None]:
        """Wait for the index before retrying an index-dependent operation."""
        for attempt in range(5):
            self._wait_for_index(version)
            yield
            if attempt < 4:
                print(
                    f"Index returned inconsistent files for "
                    f"{self.target.project_name}=={version}; "
                    f"retrying {operation} ({attempt + 1}/4)",
                    file=sys.stderr,
                )

    def _check_index_for_provenance(self, project: BuiltProject):
        """Check that every uploaded distribution has provenance."""
        url = self.target.index_url + self.target.project_name + "/"
        with httpx.Client(timeout=120) as client:
            response = client.get(
                url,
                follow_redirects=True,
                headers={"Accept": "application/vnd.pypi.simple.v1+json"},
            )
            response.raise_for_status()

        data = response.json()
        for file_data in data["files"]:
            if str(project.version) in file_data["filename"] and not file_data.get(
                "provenance"
            ):
                raise RuntimeError(
                    f"Missing provenance for {self.target.project_name} "
                    f"{project.version} file {file_data['filename']}"
                )

    @staticmethod
    def _create_attestations(project: BuiltProject):
        """Create a Sigstore attestation for each distribution."""
        trust = ClientTrustConfig.production()
        identity = oidc.detect_credential()

        if not identity:
            raise RuntimeError("Failed to detect OIDC credential for signing")

        identity_token = oidc.IdentityToken(identity)
        context = SigningContext.from_trust_config(trust)

        with context.signer(identity_token=identity_token) as signer:
            for filename in project.filenames:
                dist_path = project.root / "dist" / filename
                distribution = Distribution.from_file(dist_path)
                attestation = Attestation.sign(signer, distribution)
                attestation_path = dist_path.with_suffix(
                    dist_path.suffix + ".publish.attestation"
                )
                attestation_path.write_text(attestation.model_dump_json())

    @staticmethod
    def _fresh_version() -> Version:
        """Synthesize a unique version from the current time."""
        timestamp = time.strftime("%Y%m%d%H%M%S", time.gmtime())
        milliseconds = int((time.time() % 1) * 1000)
        return Version(f"0.{timestamp}.{milliseconds:03d}")


def select_targets(requested: list[str]) -> list[str]:
    """Expand the local and CI target groups."""
    if requested == ["local"]:
        return list(LOCAL_TARGETS)
    if requested == ["all"]:
        return list(CI_TARGETS)
    return requested


def main() -> int:
    parser = ArgumentParser()
    target_choices = [*TARGETS, "local", "all"]
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
        uv = REPOSITORY_ROOT.joinpath(f"target/debug/uv{executable_suffix}")

    targets = select_targets(args.targets)

    with TemporaryDirectory(prefix="uv-publish-keyring-") as temporary:
        for name in targets:
            session = TargetSession(name, uv, Path(temporary))
            with session:
                PublishTest(session).run()

            if session.failed:
                return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
