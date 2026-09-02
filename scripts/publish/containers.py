"""Isolate publishing-test dependencies from host credentials and processes."""

import os
import shlex
import shutil
import sys
from dataclasses import dataclass
from pathlib import Path
from stat import S_ISREG
from subprocess import DEVNULL, check_call, run
from tempfile import TemporaryDirectory


def container_command(image: str, *, mounts: tuple[str, ...] = ()) -> list[str]:
    return [
        "docker",
        "run",
        "--rm",
        "--network=none",
        "--cap-drop=ALL",
        "--security-opt=no-new-privileges",
        "--read-only",
        "--tmpfs=/tmp:rw,nosuid,nodev,size=256m",
        # Docker can inject proxies from its client configuration, including
        # credentials in proxy URLs. Neither container needs a proxy.
        *[
            f"--env={name}="
            for proxy in (
                "http_proxy",
                "https_proxy",
                "ftp_proxy",
                "all_proxy",
                "no_proxy",
            )
            for name in (proxy, proxy.upper())
        ],
        # Match ownership of the output directory, including on Docker Desktop.
        f"--user={os.getuid()}:{os.getgid()}",
        "--interactive",
        *[argument for mount in mounts for argument in ("--mount", mount)],
        image,
    ]


def validate_distributions(directory: Path, distribution: str):
    """Admit only the expected regular archives from the backend container."""
    expected = {f"{distribution}.tar.gz", f"{distribution}-py3-none-any.whl"}
    for path in directory.iterdir():
        if path.name not in expected or not S_ISREG(path.lstat().st_mode):
            raise RuntimeError(f"Unexpected build output: {path.name}")
        expected.remove(path.name)
    if expected:
        raise RuntimeError(f"Missing build outputs: {', '.join(sorted(expected))}")


@dataclass
class PublishContainer:
    build_image: str
    keyring_image: str

    @classmethod
    def prepare(cls) -> "PublishContainer":
        # Send only these checked-in files to Docker, never the checkout, home,
        # generated fixtures, or credential stores. Use the resulting immutable
        # image ID rather than a mutable tag for every subsequent invocation.
        source = Path(__file__).parent / "container"
        with TemporaryDirectory(prefix="uv-publish-image-") as temporary:
            context = Path(temporary)
            for filename in (
                "Dockerfile",
                "build-requirements.txt",
                "keyring-requirements.txt",
                "build.py",
                "keyring.py",
            ):
                shutil.copyfile(source / filename, context / filename)
            images = []
            for target in ("build", "keyring"):
                image_id = context / f"{target}-image-id"
                check_call(
                    [
                        "docker",
                        "build",
                        "--target",
                        target,
                        "--iidfile",
                        str(image_id),
                        str(context),
                    ]
                )
                images.append(image_id.read_text().strip())
                image_id.unlink()
            return cls(*images)

    def build(self, project: Path, distribution: str):
        distributions = project / "dist"
        distributions.mkdir()
        command = container_command(
            self.build_image,
            mounts=(
                f"type=bind,source={project},target=/project,readonly",
                f"type=bind,source={distributions},target=/dist",
            ),
        )
        check_call([*command, "python", "-I", "/scripts/build.py"], stdin=DEVNULL)
        # The container has exited. Reject links and extra files before host code
        # reads archives, touches .DS_Store, or creates attestation sidecars.
        validate_distributions(distributions, distribution)

    def keyring_environment(self, directory: Path) -> dict[str, str]:
        # This host-side proxy contains only repository code. Third-party keyring
        # code runs in a fresh container with no host mounts or inherited env.
        proxy = directory / "keyring"
        command = shlex.join(
            [sys.executable, "-I", str(Path(__file__).resolve()), self.keyring_image]
        )
        proxy.write_text(f'#!/bin/sh\nexec {command} "$@"\n')
        proxy.chmod(0o755)
        return {"PATH": str(directory) + os.pathsep + os.environ["PATH"]}


if __name__ == "__main__":
    image, *arguments = sys.argv[1:]
    result = run(
        [*container_command(image), "python", "-I", "/scripts/keyring.py", *arguments],
        input=os.environ["UV_TEST_PUBLISH_KEYRING"],
        text=True,
        check=False,
    )
    sys.exit(result.returncode)
