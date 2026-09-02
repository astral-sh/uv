"""Build publishing fixtures inside the dependency container."""

import os
import shutil
import tarfile
from pathlib import Path

from pdm.backend import build_sdist, build_wheel

# Only the generated fixture is mounted, read-only. Build in a disposable directory,
# and expose only the resulting distributions to the host.
shutil.copytree("/project", "/tmp/project", ignore=shutil.ignore_patterns("dist"))
os.chdir("/tmp/project")
sdist = build_sdist("/dist")
with tarfile.open(Path("/dist") / sdist) as archive:
    archive.extractall("/tmp/sdist", filter="data")
os.chdir(Path("/tmp/sdist") / sdist.removesuffix(".tar.gz"))
build_wheel("/dist")
