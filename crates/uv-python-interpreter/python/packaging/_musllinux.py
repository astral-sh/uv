"""PEP 656 support.

This module implements logic to detect if the currently running Python is
linked against musl, and what musl version is used.
"""

import functools
import os
import re
import subprocess
import sys
from typing import Iterator, NamedTuple, Sequence

from ._elffile import ELFFile


class _MuslVersion(NamedTuple):
    major: int
    minor: int


def _parse_musl_version(interpreter: str, output: bytes) -> "_MuslVersion | None":
    lines = [n for n in (n.strip() for n in output.splitlines()) if n]
    has_musl_header = bool(lines and lines[0].startswith(b"musl"))
    # A directory name can match the loader search heuristic without identifying musl.
    if not has_musl_header and not re.fullmatch(
        r"ld-musl-.+\.so\.1", os.path.basename(interpreter)
    ):
        return None
    if has_musl_header and len(lines) >= 2:
        m = re.match(rb"Version (\d+)\.(\d+)", lines[1])
        if m:
            return _MuslVersion(major=int(m.group(1)), minor=int(m.group(2)))
    raise ValueError(f"Could not read the musl version from {interpreter!r}")


@functools.lru_cache()
def _get_musl_version(executable: str) -> "_MuslVersion | None":
    """Detect currently-running musl runtime version.

    This is done by checking the specified executable's dynamic linking
    information, and invoking the loader to parse its output for a version
    string. If the loader is musl, the output would be something like::

        musl libc (x86_64)
        Version 1.2.2
        Dynamic Program Loader
    """
    try:
        with open(executable, "rb") as f:
            ld = ELFFile(f).interpreter
    except (OSError, TypeError, ValueError):
        return None
    if ld is None or "musl" not in ld:
        return None
    proc = subprocess.run([ld], stderr=subprocess.PIPE)
    return _parse_musl_version(ld, proc.stderr)


def platform_tags(archs: Sequence[str]) -> Iterator[str]:
    """Generate musllinux tags compatible to the current platform.

    :param archs: Sequence of compatible architectures.
        The first one shall be the closest to the actual architecture and be the part of
        platform tag after the ``linux_`` prefix, e.g. ``x86_64``.
        The ``linux_`` prefix is assumed as a prerequisite for the current platform to
        be musllinux-compatible.

    :returns: An iterator of compatible musllinux tags.
    """
    sys_musl = _get_musl_version(sys.executable)
    if sys_musl is None:  # Python not dynamically linked against musl.
        return
    for arch in archs:
        for minor in range(sys_musl.minor, -1, -1):
            yield f"musllinux_{sys_musl.major}_{minor}_{arch}"


if __name__ == "__main__":  # pragma: no cover
    import sysconfig

    plat = sysconfig.get_platform()
    assert plat.startswith("linux-"), "not linux"

    print("plat:", plat)
    print("musl:", _get_musl_version(sys.executable))
    print("tags:", end=" ")
    for t in platform_tags(re.sub(r"[.-]", "_", plat.split("-", 1)[-1])):
        print(t, end="\n      ")
