import collections
import contextlib
import functools
import os
import re
import sys
import warnings
from typing import Generator, Iterator, NamedTuple, Sequence

from ._elffile import EIClass, EIData, ELFFile, EMachine

EF_ARM_ABIMASK = 0xFF000000
EF_ARM_ABI_VER5 = 0x05000000
EF_ARM_ABI_FLOAT_HARD = 0x00000400


# `os.PathLike` not a generic type until Python 3.9, so sticking with `str`
# as the type for `path` until then.
@contextlib.contextmanager
def _parse_elf(path: str) -> "Generator[ELFFile | None, None, None]":
    try:
        with open(path, "rb") as f:
            yield ELFFile(f)
    except (OSError, TypeError, ValueError):
        yield None


def _is_linux_armhf(executable: str) -> bool:
    # hard-float ABI can be detected from the ELF header of the running
    # process
    # https://static.docs.arm.com/ihi0044/g/aaelf32.pdf
    with _parse_elf(executable) as f:
        return (
                f is not None
                and f.capacity == EIClass.C32
                and f.encoding == EIData.Lsb
                and f.machine == EMachine.Arm
                and f.flags & EF_ARM_ABIMASK == EF_ARM_ABI_VER5
                and f.flags & EF_ARM_ABI_FLOAT_HARD == EF_ARM_ABI_FLOAT_HARD
        )


def _is_linux_i686(executable: str) -> bool:
    with _parse_elf(executable) as f:
        return (
                f is not None
                and f.capacity == EIClass.C32
                and f.encoding == EIData.Lsb
                and f.machine == EMachine.I386
        )


def _have_compatible_abi(executable: str, archs: Sequence[str]) -> bool:
    if "armv7l" in archs:
        return _is_linux_armhf(executable)
    if "i686" in archs:
        return _is_linux_i686(executable)
    allowed_archs = {
        "x86_64",
        "aarch64",
        "ppc64",
        "ppc64le",
        "s390x",
        "loongarch64",
        "riscv64",
    }
    return any(arch in allowed_archs for arch in archs)


# If glibc ever changes its major version, we need to know what the last
# minor version was, so we can build the complete list of all versions.
# For now, guess what the highest minor version might be, assume it will
# be 50 for testing. Once this actually happens, update the dictionary
# with the actual value.
_LAST_GLIBC_MINOR: "dict[int, int]" = collections.defaultdict(lambda: 50)


class _GLibCVersion(NamedTuple):
    major: int
    minor: int
    interpreter: "str | None"


def _glibc_version_string_confstr() -> "str | None":
    """Read the glibc version without requiring ctypes."""
    # This strategy is used in the standard library platform module.
    # https://github.com/python/cpython/blob/fcf1d003bf4f0100c/Lib/platform.py#L175-L183
    try:
        # Should be a string like "glibc 2.17".
        version_string: "str | None" = os.confstr("CS_GNU_LIBC_VERSION")
        assert version_string is not None
        _, version = version_string.rsplit()
    except (AssertionError, AttributeError, OSError, ValueError):
        # os.confstr() or CS_GNU_LIBC_VERSION not available (or a bad value)...
        return None
    return version


@functools.lru_cache()
def _get_glibc_version() -> "_GLibCVersion | None":
    """Detect the version and path of the glibc loaded by this process."""
    version_str = _glibc_version_string_confstr()
    if version_str is not None:
        version = _parse_glibc_version(version_str)
    else:
        version = None

    try:
        import ctypes
    except (ImportError, OSError):
        return version

    # CDLL(None) calls dlopen(NULL), so we can look up symbols in the libraries
    # loaded by this Python process. On statically linked musl builds, this
    # can raise OSError without a useful errno or portable error string.
    try:
        process_namespace = ctypes.CDLL(None)
        gnu_get_libc_version = process_namespace.gnu_get_libc_version
    except (OSError, AttributeError):
        return version

    if version_str is None:
        gnu_get_libc_version.restype = ctypes.c_char_p
        try:
            version_bytes = gnu_get_libc_version()
        except PermissionError:
            return None
        version = _parse_glibc_version(version_bytes.decode("ascii"))
    if version is None:
        return None

    # Read the location of the ELF interpreter too.
    class DlInfo(ctypes.Structure):
        _fields_ = [
            ("dli_fname", ctypes.c_char_p),
            ("dli_fbase", ctypes.c_void_p),
            ("dli_sname", ctypes.c_char_p),
            ("dli_saddr", ctypes.c_void_p),
        ]

    try:
        dladdr = getattr(process_namespace, "dladdr", None)
        if dladdr is None:
            # Before glibc 2.34, dladdr was provided by libdl.
            dladdr = ctypes.CDLL("libdl.so.2").dladdr
    except (OSError, AttributeError):
        return version
    dladdr.argtypes = (ctypes.c_void_p, ctypes.POINTER(DlInfo))
    dladdr.restype = ctypes.c_int

    info = DlInfo()
    try:
        address = ctypes.cast(gnu_get_libc_version, ctypes.c_void_p)
        found = dladdr(address, ctypes.byref(info))
    except PermissionError:
        return version
    if not found or not info.dli_fname:
        return version

    path = os.fsdecode(info.dli_fname)
    if not os.path.isabs(path):
        try:
            path = os.path.join(os.getcwd(), path)
        except OSError:
            return version
    return _GLibCVersion(major=version.major, minor=version.minor, interpreter=path)


def _parse_glibc_version(version_str: str) -> "_GLibCVersion | None":
    """Parse glibc version.

    We use a regexp instead of str.split because we want to discard any
    random junk that might come after the minor version -- this might happen
    in patched/forked versions of glibc (e.g. Linaro's version of glibc
    uses version strings like "2.20-2014.11"). See gh-3588.
    """
    m = re.match(r"(?P<major>[0-9]+)\.(?P<minor>[0-9]+)", version_str)
    if not m:
        warnings.warn(
            f"Expected glibc version with 2 components major.minor, got: {version_str}",
            RuntimeWarning,
        )
        return None
    return _GLibCVersion(int(m.group("major")), int(m.group("minor")), None)


# From PEP 513, PEP 600
def _is_compatible(arch: str, version: _GLibCVersion) -> bool:
    sys_glibc = _get_glibc_version()
    version_numbers = (version.major, version.minor)
    if sys_glibc is None or (sys_glibc.major, sys_glibc.minor) < version_numbers:
        return False
    # Check for presence of _manylinux module.
    try:
        import _manylinux
    except ImportError:
        return True
    if hasattr(_manylinux, "manylinux_compatible"):
        result = _manylinux.manylinux_compatible(version.major, version.minor, arch)
        if result is not None:
            return bool(result)
        return True
    if version_numbers == (2, 5):
        if hasattr(_manylinux, "manylinux1_compatible"):
            return bool(_manylinux.manylinux1_compatible)
    if version_numbers == (2, 12):
        if hasattr(_manylinux, "manylinux2010_compatible"):
            return bool(_manylinux.manylinux2010_compatible)
    if version_numbers == (2, 17):
        if hasattr(_manylinux, "manylinux2014_compatible"):
            return bool(_manylinux.manylinux2014_compatible)
    return True


_LEGACY_MANYLINUX_MAP: "dict[tuple[int, int], str]" = {
    # CentOS 7 w/ glibc 2.17 (PEP 599)
    (2, 17): "manylinux2014",
    # CentOS 6 w/ glibc 2.12 (PEP 571)
    (2, 12): "manylinux2010",
    # CentOS 5 w/ glibc 2.5 (PEP 513)
    (2, 5): "manylinux1",
}


def platform_tags(archs: Sequence[str]) -> Iterator[str]:
    """Generate manylinux tags compatible to the current platform.

    :param archs: Sequence of compatible architectures.
        The first one shall be the closest to the actual architecture and be the part of
        platform tag after the ``linux_`` prefix, e.g. ``x86_64``.
        The ``linux_`` prefix is assumed as a prerequisite for the current platform to
        be manylinux-compatible.

    :returns: An iterator of compatible manylinux tags.
    """
    if not _have_compatible_abi(sys.executable, archs):
        return
    # Oldest glibc to be supported regardless of architecture is (2, 17).
    too_old_glibc2 = _GLibCVersion(2, 16, None)
    if set(archs) & {"x86_64", "i686"}:
        # On x86/i686 also oldest glibc to be supported is (2, 5).
        too_old_glibc2 = _GLibCVersion(2, 4, None)
    current_glibc = _get_glibc_version()
    if current_glibc is None:
        return
    glibc_max_list = [current_glibc]
    # We can assume compatibility across glibc major versions.
    # https://sourceware.org/bugzilla/show_bug.cgi?id=24636
    #
    # Build a list of maximum glibc versions so that we can
    # output the canonical list of all glibc from current_glibc
    # down to too_old_glibc2, including all intermediary versions.
    for glibc_major in range(current_glibc.major - 1, 1, -1):
        glibc_minor = _LAST_GLIBC_MINOR[glibc_major]
        glibc_max_list.append(_GLibCVersion(glibc_major, glibc_minor, None))
    for arch in archs:
        for glibc_max in glibc_max_list:
            if glibc_max.major == too_old_glibc2.major:
                min_minor = too_old_glibc2.minor
            else:
                # For other glibc major versions oldest supported is (x, 0).
                min_minor = -1
            for glibc_minor in range(glibc_max.minor, min_minor, -1):
                glibc_version = _GLibCVersion(glibc_max.major, glibc_minor, None)
                if _is_compatible(arch, glibc_version):
                    yield "manylinux_{}_{}_{}".format(
                        glibc_version.major, glibc_version.minor, arch
                    )
                    # Handle the legacy manylinux1, manylinux2010, manylinux2014 tags.
                    legacy_tag = _LEGACY_MANYLINUX_MAP.get(glibc_version[:2])
                    if legacy_tag:
                        yield f"{legacy_tag}_{arch}"
