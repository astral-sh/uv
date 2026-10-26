"""
Queries information about the current Python interpreter and prints it as JSON.

The script will exit with status 0 on known error that are turned into rust errors.
"""

import importlib.machinery
import json
import os
import platform
import site
import struct
import sys
import sysconfig
from os import name as os_name


def format_full_version(info):
    version = f"{info.major}.{info.minor}.{info.micro}"
    kind = info.releaselevel
    if kind != "final":
        version += kind[0] + str(info.serial)
    return version


if sys.version_info[0] < 3:
    print(
        json.dumps(
            {
                "result": "error",
                "kind": "unsupported_python_version",
                "python_version": format_full_version(sys.version_info),
            }
        )
    )
    sys.exit(0)

if hasattr(sys, "implementation"):
    implementation_name = sys.implementation.name
    if implementation_name == "graalpy":
        # GraalPy reports the CPython version as sys.implementation.version,
        # so we need to discover the GraalPy version from the cache_tag
        import re

        implementation_version = re.sub(
            r"graalpy(\d)(\d+)(?:dev[\da-f]+)?-\d+",
            r"\1.\2",
            sys.implementation.cache_tag,
        )
    elif implementation_name == "pyston":
        # Pyston reports the CPython version as sys.implementation.version,
        # so we need to discover the Pyston version from the cache_tag
        import re

        implementation_version = re.sub(
            r"pyston-(\d)(\d+)",
            r"\1.\2",
            sys.implementation.cache_tag,
        )
    else:
        implementation_version = format_full_version(sys.implementation.version)
else:
    implementation_version = "0"
    implementation_name = ""

python_full_version = platform.python_version()
# For local builds of Python, at time of writing, the version numbers end with
# a `+`. This makes the version non-PEP-440 compatible since a `+` indicates
# the start of a local segment which must be non-empty. Thus, `uv` chokes on it
# and spits out an error[1] when trying to create a venv using a "local" build
# of Python. Arguably, the right fix for this is for CPython to use a PEP-440
# compatible version number[2].
#
# However, as a work-around for now, as suggested by pradyunsg[3] as one
# possible direction forward, we strip the `+`.
#
# This fix does unfortunately mean that one cannot specify a Python version
# constraint that specifically selects a local version[4]. But at the time of
# writing, it seems reasonable to block such functionality on this being fixed
# upstream (in some way).
#
# Another alternative would be to treat such invalid versions as strings (which
# is what PEP-508 suggests), but this leads to undesirable behavior in this
# case. For example, let's say you have a Python constraint of `>=3.9.1` and
# a local build of Python with a version `3.11.1+`. Using string comparisons
# would mean the constraint wouldn't be satisfied:
#
#     >>> "3.9.1" < "3.11.1+"
#     False
#
# So in the end, we just strip the trailing `+`, as was done in the days of old
# for legacy version numbers[5].
#
# [1]: https://github.com/astral-sh/uv/issues/1357
# [2]: https://github.com/python/cpython/issues/99968
# [3]: https://github.com/pypa/packaging/issues/678#issuecomment-1436033646
# [4]: https://github.com/astral-sh/uv/issues/1357#issuecomment-1947645243
# [5]: https://github.com/pypa/packaging/blob/085ff41692b687ae5b0772a55615b69a5b677be9/packaging/version.py#L168-L193
if len(python_full_version) > 0 and python_full_version[-1] == "+":
    python_full_version = python_full_version[:-1]


def _running_under_venv() -> bool:
    """Checks if sys.base_prefix and sys.prefix match.

    This handles PEP 405 compliant virtual environments.
    """
    return sys.prefix != getattr(sys, "base_prefix", sys.prefix)


def _running_under_legacy_virtualenv() -> bool:
    """Checks if sys.real_prefix is set.

    This handles virtual environments created with pypa's virtualenv.
    """
    # pypa/virtualenv case
    return hasattr(sys, "real_prefix")


def running_under_virtualenv() -> bool:
    """True if we're running inside a virtual environment, False otherwise."""
    return _running_under_venv() or _running_under_legacy_virtualenv()


def get_major_minor_version() -> str:
    """
    Return the major-minor version of the current Python as a string, e.g.
    "3.7" or "3.10".
    """
    return "{}.{}".format(*sys.version_info)


def get_virtualenv():
    """Return the expected Scheme for virtualenvs created by this interpreter.

    The paths returned should be relative to a root directory.

    This is based on virtualenv's path discovery logic:
        https://github.com/pypa/virtualenv/blob/5cd543fdf8047600ff2737babec4a635ad74d169/src/virtualenv/discovery/py_info.py#L80C9-L80C17
    """
    scheme_names = sysconfig.get_scheme_names()

    # Determine the scheme to use, if any.
    if "venv" in scheme_names:
        sysconfig_scheme = "venv"
    elif sys.version_info[:2] == (3, 10) and "deb_system" in scheme_names:
        # debian / ubuntu python 3.10 without `python3-distutils` will report
        # mangled `local/bin` / etc. names for the default prefix
        # intentionally select `posix_prefix` which is the unaltered posix-like paths
        sysconfig_scheme = "posix_prefix"
    else:
        sysconfig_scheme = None

    # Use `sysconfig`, if available.
    if sysconfig_scheme:
        import re

        sysconfig_paths = {
            i: sysconfig.get_path(i, expand=False, scheme=sysconfig_scheme)
            for i in sysconfig.get_path_names()
        }

        # Determine very configuration variable that we need to resolve.
        config_var_keys = set()

        conf_var_re = re.compile(r"\{\w+}")
        for element in sysconfig_paths.values():
            for k in conf_var_re.findall(element):
                config_var_keys.add(k[1:-1])
        config_var_keys.add("PYTHONFRAMEWORK")

        # Look them up.
        sysconfig_vars = {i: sysconfig.get_config_var(i or "") for i in config_var_keys}

        # Information about the prefix (determines the Python home).
        prefix = os.path.abspath(sys.prefix)
        base_prefix = os.path.abspath(sys.base_prefix)

        # Information about the exec prefix (dynamic stdlib modules).
        base_exec_prefix = os.path.abspath(sys.base_exec_prefix)
        exec_prefix = os.path.abspath(sys.exec_prefix)

        # Set any prefixes to empty, which makes the resulting paths relative.
        prefixes = prefix, exec_prefix, base_prefix, base_exec_prefix
        sysconfig_vars.update(
            {k: "" if v in prefixes else v for k, v in sysconfig_vars.items()}
        )

        def expand_path(path: str) -> str:
            return path.format(**sysconfig_vars).replace("/", os.sep).lstrip(os.sep)

        return {
            "purelib": expand_path(sysconfig_paths["purelib"]),
            "platlib": expand_path(sysconfig_paths["platlib"]),
            "include": os.path.join(
                "include", "site", f"python{get_major_minor_version()}"
            ),
            "scripts": expand_path(sysconfig_paths["scripts"]),
            "data": expand_path(sysconfig_paths["data"]),
        }
    else:
        # Use distutils primarily because that's what pip does.
        # https://github.com/pypa/pip/blob/ae5fff36b0aad6e5e0037884927eaa29163c0611/src/pip/_internal/locations/__init__.py#L249

        # Disable the use of the setuptools shim, if it's injected. Per pip:
        #
        # > If pip's going to use distutils, it should not be using the copy that setuptools
        # > might have injected into the environment. This is done by removing the injected
        # > shim, if it's injected.
        #
        # > See https://github.com/pypa/pip/issues/8761 for the original discussion and
        # > rationale for why this is done within pip.
        try:
            __import__("_distutils_hack").remove_shim()
        except (ImportError, AttributeError):
            pass

        import warnings

        with warnings.catch_warnings():  # disable warning for PEP-632
            warnings.simplefilter("ignore")
            from distutils import dist
            from distutils.command.install import SCHEME_KEYS

        d = dist.Distribution({"script_args": "--no-user-cfg"})
        if hasattr(sys, "_framework"):
            sys._framework = None

        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            i = d.get_command_obj("install", create=True)

        i.prefix = os.sep
        i.finalize_options()
        distutils_paths = {
            key: (getattr(i, f"install_{key}")[1:]).lstrip(os.sep)
            for key in SCHEME_KEYS
        }

        return {
            "purelib": distutils_paths["purelib"],
            "platlib": distutils_paths["platlib"],
            "include": os.path.join(
                "include", "site", f"python{get_major_minor_version()}"
            ),
            "scripts": distutils_paths["scripts"],
            "data": distutils_paths["data"],
        }


def get_scheme(use_sysconfig_scheme: bool):
    """Return the Scheme for the current interpreter.

    The paths returned should be absolute.

    This is based on pip's path discovery logic:
        https://github.com/pypa/pip/blob/ae5fff36b0aad6e5e0037884927eaa29163c0611/src/pip/_internal/locations/__init__.py#L230
    """

    def get_sysconfig_scheme():
        """Get the "scheme" corresponding to the input parameters.

        Uses the `sysconfig` module to get the scheme.

        Based on (with default arguments):
            https://github.com/pypa/pip/blob/ae5fff36b0aad6e5e0037884927eaa29163c0611/src/pip/_internal/locations/_sysconfig.py#L124
        """

        def is_osx_framework() -> bool:
            return bool(sysconfig.get_config_var("PYTHONFRAMEWORK"))

        # Notes on _infer_* functions.
        # Unfortunately ``get_default_scheme()`` didn't exist before 3.10, so there's no
        # way to ask things like "what is the '_prefix' scheme on this platform". These
        # functions try to answer that with some heuristics while accounting for ad-hoc
        # platforms not covered by CPython's default sysconfig implementation. If the
        # ad-hoc implementation does not fully implement sysconfig, we'll fall back to
        # a POSIX scheme.

        _AVAILABLE_SCHEMES = set(sysconfig.get_scheme_names())

        _PREFERRED_SCHEME_API = getattr(sysconfig, "get_preferred_scheme", None)

        def _should_use_osx_framework_prefix() -> bool:
            """Check for Apple's ``osx_framework_library`` scheme.

            Python distributed by Apple's Command Line Tools has this special scheme
            that's used when:

            * This is a framework build.
            * We are installing into the system prefix.

            This does not account for ``pip install --prefix`` (also means we're not
            installing to the system prefix), which should use ``posix_prefix``, but
            logic here means ``_infer_prefix()`` outputs ``osx_framework_library``. But
            since ``prefix`` is not available for ``sysconfig.get_default_scheme()``,
            which is the stdlib replacement for ``_infer_prefix()``, presumably Apple
            wouldn't be able to magically switch between ``osx_framework_library`` and
            ``posix_prefix``. ``_infer_prefix()`` returning ``osx_framework_library``
            means its behavior is consistent whether we use the stdlib implementation
            or our own, and we deal with this special case in ``get_scheme()`` instead.
            """
            return (
                "osx_framework_library" in _AVAILABLE_SCHEMES
                and not running_under_virtualenv()
                and is_osx_framework()
            )

        def _infer_prefix() -> str:
            """Try to find a prefix scheme for the current platform.

            This tries:

            * A special ``osx_framework_library`` for Python distributed by Apple's
              Command Line Tools, when not running in a virtual environment.
            * Implementation + OS, used by PyPy on Windows (``pypy_nt``).
            * Implementation without OS, used by PyPy on POSIX (``pypy``).
            * OS + "prefix", used by CPython on POSIX (``posix_prefix``).
            * Just the OS name, used by CPython on Windows (``nt``).

            If none of the above works, fall back to ``posix_prefix``.
            """
            if _PREFERRED_SCHEME_API:
                return _PREFERRED_SCHEME_API("prefix")
            if _should_use_osx_framework_prefix():
                return "osx_framework_library"
            implementation_suffixed = f"{sys.implementation.name}_{os.name}"
            if implementation_suffixed in _AVAILABLE_SCHEMES:
                return implementation_suffixed
            if sys.implementation.name in _AVAILABLE_SCHEMES:
                return sys.implementation.name
            suffixed = f"{os.name}_prefix"
            if suffixed in _AVAILABLE_SCHEMES:
                return suffixed
            if os.name in _AVAILABLE_SCHEMES:  # On Windows, prefx is just called "nt".
                return os.name
            return "posix_prefix"

        scheme_name = _infer_prefix()
        paths = sysconfig.get_paths(scheme=scheme_name)

        # Logic here is very arbitrary, we're doing it for compatibility, don't ask.
        # 1. Pip historically uses a special header path in virtual environments.
        if running_under_virtualenv():
            python_xy = f"python{get_major_minor_version()}"
            paths["include"] = os.path.join(sys.prefix, "include", "site", python_xy)

        return {
            "platlib": paths["platlib"],
            "purelib": paths["purelib"],
            "include": paths["include"],
            "scripts": paths["scripts"],
            "data": paths["data"],
        }

    def get_distutils_scheme():
        """Get the "scheme" corresponding to the input parameters.

        Uses the deprecated `distutils` module to get the scheme.

        Based on (with default arguments):
            https://github.com/pypa/pip/blob/ae5fff36b0aad6e5e0037884927eaa29163c0611/src/pip/_internal/locations/_distutils.py#L115
        """
        import warnings

        with warnings.catch_warnings():  # disable warning for PEP-632
            warnings.simplefilter("ignore")
            from distutils.dist import Distribution

        dist_args = {}

        d = Distribution(dist_args)
        try:
            d.parse_config_files()
        except UnicodeDecodeError:
            pass

        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            i = d.get_command_obj("install", create=True)

        i.finalize_options()

        scheme = {}
        for key in ("purelib", "platlib", "headers", "scripts", "data"):
            scheme[key] = getattr(i, "install_" + key)

        # install_lib specified in setup.cfg should install *everything*
        # into there (i.e. it takes precedence over both purelib and
        # platlib).  Note, i.install_lib is *always* set after
        # finalize_options(); we only want to override here if the user
        # has explicitly requested it hence going back to the config
        if "install_lib" in d.get_option_dict("install"):
            # noinspection PyUnresolvedReferences
            scheme.update({"purelib": i.install_lib, "platlib": i.install_lib})

        if running_under_virtualenv():
            # noinspection PyUnresolvedReferences
            scheme["headers"] = os.path.join(
                i.prefix,
                "include",
                "site",
                f"python{get_major_minor_version()}",
                "UNKNOWN",
            )

        return {
            "platlib": scheme["platlib"],
            "purelib": scheme["purelib"],
            "include": os.path.dirname(scheme["headers"]),
            "scripts": scheme["scripts"],
            "data": scheme["data"],
        }

    if use_sysconfig_scheme:
        return get_sysconfig_scheme()
    else:
        return get_distutils_scheme()


def get_operating_system_and_architecture():
    """Determine the Python interpreter architecture and operating system.

    This can differ from uv's architecture and operating system. For example, Apple
    Silicon Macs can run both x86_64 and aarch64 binaries transparently.
    """
    # https://github.com/pypa/packaging/blob/cc938f984bbbe43c5734b9656c9837ab3a28191f/src/packaging/_musllinux.py#L84
    # Note that this is not `os.name`.
    # https://docs.python.org/3/library/sysconfig.html#sysconfig.get_platform
    # windows x86 will return win32
    platform_info = sysconfig.get_platform().split("-", 1)
    if len(platform_info) == 1:
        if platform_info[0] == "win32":
            operating_system, version_arch = "win", "i386"
        else:
            # unknown_operating_system will flow to the final error print
            operating_system, version_arch = platform_info[0], ""
    else:
        [operating_system, version_arch] = platform_info
    if "-" in version_arch:
        # Ex: macosx-11.2-arm64
        version, architecture = version_arch.rsplit("-", 1)
    else:
        # Ex: linux-x86_64
        version = None
        architecture = version_arch

    if sys.version_info < (3, 6):
        print(
            json.dumps(
                {
                    "result": "error",
                    "kind": "unsupported_python_version",
                    "python_version": format_full_version(sys.version_info),
                }
            )
        )
        sys.exit(0)

    if operating_system == "linux":
        # noinspection PyProtectedMember
        from .packaging._manylinux import _get_glibc_version

        # noinspection PyProtectedMember
        from .packaging._musllinux import _get_musl_version

        # https://github.com/pypa/packaging/blob/4dc334c86d43f83371b194ca91618ed99e0e49ca/src/packaging/tags.py#L539-L543
        # https://github.com/astral-sh/uv/issues/9842
        if struct.calcsize("P") == 4:
            if architecture == "x86_64":
                architecture = "i686"
            elif architecture == "aarch64":
                architecture = "armv8l"

        musl_version = _get_musl_version(sys.executable)
        glibc_version = _get_glibc_version()

        if musl_version:
            operating_system = {
                "name": "musllinux",
                "major": musl_version[0],
                "minor": musl_version[1],
            }
        elif glibc_version != (-1, -1):
            operating_system = {
                "name": "manylinux",
                "major": glibc_version[0],
                "minor": glibc_version[1],
            }
        elif hasattr(sys, "getandroidapilevel"):
            # On Python <3.13, Android reports itself as "linux"
            # See `operation_system == "android"` branch for Python 3.13+ below
            operating_system = {
                "name": "android",
                "api_level": sys.getandroidapilevel(),
            }
        else:
            print(json.dumps({"result": "error", "kind": "libc_not_found"}))
            sys.exit(0)
    elif operating_system == "win":
        operating_system = {
            "name": "windows",
        }
    elif operating_system == "macosx":
        # Apparently, Mac OS is reporting i386 sometimes in sysconfig.get_platform even
        # though that's not a thing anymore.
        # https://github.com/astral-sh/uv/issues/2450
        version, _, architecture = platform.mac_ver()

        if not version or not architecture:
            print(json.dumps({"result": "error", "kind": "broken_mac_ver"}))
            sys.exit(0)

        # https://github.com/pypa/packaging/blob/cc938f984bbbe43c5734b9656c9837ab3a28191f/src/packaging/tags.py#L356-L363
        is_32bit = struct.calcsize("P") == 4
        if is_32bit:
            if architecture.startswith("ppc"):
                architecture = "ppc"
            else:
                architecture = "i386"

        version = version.split(".")
        operating_system = {
            "name": "macos",
            "major": int(version[0]),
            "minor": int(version[1]),
        }
    elif operating_system == "ios":
        ios_ver = platform.ios_ver()
        version = ios_ver.release.split(".")
        operating_system = {
            "name": "ios",
            "major": int(version[0]),
            "minor": int(version[1]),
            "simulator": ios_ver.is_simulator,
        }
        [_version, architecture, _platform] = version_arch.split("-")
    elif operating_system == "emscripten":
        pyemscripten_platform_version = sysconfig.get_config_var(
            "PYEMSCRIPTEN_PLATFORM_VERSION"
        )
        # fallback to PYODIDE_ABI_VERSION for backward compatibility
        pyodide_abi_version = sysconfig.get_config_var("PYODIDE_ABI_VERSION")
        if pyemscripten_platform_version:
            version = pyemscripten_platform_version.split("_")
            operating_system = {
                "name": "pyemscripten",
                "major": int(version[0]),
                "minor": int(version[1]),
            }
        elif pyodide_abi_version:
            version = pyodide_abi_version.split("_")
            operating_system = {
                "name": "pyodide",
                "major": int(version[0]),
                "minor": int(version[1]),
            }
        else:
            print(
                json.dumps(
                    {
                        "result": "error",
                        "kind": "emscripten_not_pyodide",
                    }
                )
            )
            sys.exit(0)
    elif operating_system == "android":
        # Python 3.13+ supports Android. We map the Android ABIs to our standard architectures.
        #
        # See:
        #
        # - https://peps.python.org/pep-0738/
        # - https://packaging.python.org/en/latest/specifications/platform-compatibility-tags/#android
        # - https://developer.android.com/ndk/guides/abis#sa
        android_abi_to_arch = {
            "arm64_v8a": "aarch64",
            "armeabi_v7a": "armv7l",
            "x86": "i686",
            "x86_64": "x86_64",
        }
        architecture = android_abi_to_arch.get(architecture, architecture)

        operating_system = {
            "name": "android",
            "api_level": sys.getandroidapilevel(),
        }
    elif operating_system in [
        "freebsd",
        "netbsd",
        "openbsd",
        "dragonfly",
        "illumos",
        "haiku",
    ]:
        operating_system = {
            "name": operating_system,
            "release": version,
        }
    else:
        print(
            json.dumps(
                {
                    "result": "error",
                    "kind": "unknown_operating_system",
                    "operating_system": operating_system,
                }
            )
        )
        sys.exit(0)
    return operating_system, architecture


def _get_cpu_info(*, arm64):
    """Detect CPU features, following archspec's detection method on x86-64.

    Returns raw data to avoid bumping the cache when we want to do more detection in Rust.
    """
    if sys.platform == "linux":
        return _linux_aarch64_cpu_info() if arm64 else _linux_x86_64_cpu_info()
    if sys.platform == "darwin":
        return _macos_cpu_info(arm64=arm64)
    if os.name == "nt":
        return _windows_arm64_cpu_info() if arm64 else _windows_x86_64_cpu_info()
    return {"unavailable": f"No CPU probe for {sys.platform}"}


def _linux_x86_64_cpu_info():
    """Read the first processor's raw flags, following archspec's Linux detection.

    https://github.com/archspec/archspec/blob/ebd3eb502d9205f1033ffc028d661f2fbece7dfc/archspec/cpu/detect.py#L78-L104
    """
    try:
        with open("/proc/cpuinfo", encoding="utf-8") as cpuinfo:
            for line in cpuinfo:
                if not line.strip():
                    break
                key, _, value = line.partition(":")
                if key.strip() == "flags":
                    flags = value.strip()
                    if flags:
                        return {"detected": {"kind": "flags", "flags": flags}}
                    break
    except (FileNotFoundError, NotADirectoryError, PermissionError) as error:
        # A chroot may lack procfs, and a sandbox can hide or restrict /proc/cpuinfo.
        return {"unavailable": str(error)}
    # An emulated interpreter may see a non-x86 host's /proc/cpuinfo, which lacks x86 flags.
    return {"unavailable": "No x86 flags in `/proc/cpuinfo`"}


def _macos_cpu_info(*, arm64):
    import errno

    try:
        import ctypes
    except ImportError as error:
        # Python can be built without the optional _ctypes extension.
        return {"unavailable": str(error)}

    # Query sysctl from this process because a separate executable could run under a
    # different architecture. The x86 feature keys follow archspec's macOS probe.
    # https://github.com/archspec/archspec/blob/ebd3eb502d9205f1033ffc028d661f2fbece7dfc/archspec/cpu/detect.py#L290-L300
    sysctl = getattr(ctypes.CDLL(None, use_errno=True), "sysctlbyname", None)
    if sysctl is None:
        return {"unavailable": "sysctlbyname is unavailable"}
    sysctl.argtypes = [
        ctypes.c_char_p,
        ctypes.c_void_p,
        ctypes.POINTER(ctypes.c_size_t),
        ctypes.c_void_p,
        ctypes.c_size_t,
    ]
    missing_key = (errno.ENOENT, errno.ENOTSUP, errno.EOPNOTSUPP)
    unavailable = (errno.EPERM, errno.EACCES, errno.ENOSYS)
    if arm64:
        keys = (b"machdep.cpu.brand_string",)
    else:
        keys = (
            b"machdep.cpu.features",
            b"machdep.cpu.leaf7_features",
            b"machdep.cpu.extfeatures",
        )
    values = []
    for key in keys:
        size = ctypes.c_size_t()
        result = sysctl(key, None, ctypes.byref(size), None, 0)
        if result == 0 and size.value:
            buffer = ctypes.create_string_buffer(size.value)
            result = sysctl(key, buffer, ctypes.byref(size), None, 0)
            if result == 0 and buffer.value:
                values.append(buffer.value.decode("ascii", errors="replace"))
        if result != 0:
            error = ctypes.get_errno()
            # An unsupported key must not hide features available through another key.
            if error in missing_key:
                continue
            failure = OSError(error, os.strerror(error), key.decode("ascii"))
            if error in unavailable:
                return {"unavailable": str(failure)}
            raise failure
    if arm64:
        if values:
            return {"detected": {"kind": "apple_silicon", "brand": values[0]}}
        return {"unavailable": "No Apple Silicon CPU brand reported by sysctl"}
    flags = " ".join(values).split()
    if "AVX512F" in flags:
        # machdep reports hardware flags even when the kernel disables AVX-512 register state.
        # hw.optional.avx512f includes the kernel's check for usable AVX-512.
        # https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/osfmk/i386/commpage/commpage.c#L362-L372
        key = b"hw.optional.avx512f"
        enabled = ctypes.c_int()
        size = ctypes.c_size_t(ctypes.sizeof(enabled))
        result = sysctl(key, ctypes.byref(enabled), ctypes.byref(size), None, 0)
        if result != 0:
            error = ctypes.get_errno()
            if error not in missing_key + unavailable:
                raise OSError(error, os.strerror(error), key.decode("ascii"))
        if result != 0 or not enabled.value:
            flags = [flag for flag in flags if not flag.startswith("AVX512")]
    if flags:
        return {"detected": {"kind": "flags", "flags": " ".join(flags)}}
    return {"unavailable": "No x86 feature flags reported by sysctl"}


def _linux_aarch64_cpu_info():
    import ctypes

    # The auxiliary vector describes instructions available to this process, even under
    # emulation. /proc/cpuinfo can reflect the host and is only used to identify core models.
    getauxval = getattr(ctypes.CDLL(None), "getauxval", None)
    if getauxval is None:
        return {"unavailable": "getauxval is unavailable"}
    getauxval.argtypes = [ctypes.c_ulong]
    getauxval.restype = ctypes.c_ulong
    cores = set()
    try:
        with open("/proc/cpuinfo", encoding="ascii", errors="replace") as cpuinfo:
            entries = cpuinfo.read().split("\n\n")
    except (FileNotFoundError, NotADirectoryError, PermissionError) as error:
        return {"unavailable": str(error)}
    for entry in entries:
        fields = {}
        for line in entry.splitlines():
            key, _, value = line.partition(":")
            key = key.strip()
            if key in ("processor", "CPU implementer", "CPU part"):
                fields[key] = value.strip()
        if "processor" in fields:
            cores.add((fields.get("CPU implementer", ""), fields.get("CPU part", "")))
    return {
        "detected": {
            "kind": "linux_aarch64",
            "cores": sorted(cores),
            # AT_HWCAP through AT_HWCAP4. Missing entries read as zero on older kernels.
            "hwcaps": [getauxval(key) for key in (16, 26, 29, 30)],
        }
    }


def _windows_arm64_cpu_info():
    import ctypes

    present = ctypes.WinDLL("kernel32").IsProcessorFeaturePresent
    present.argtypes = [ctypes.c_uint32]
    present.restype = ctypes.c_int
    # Windows allocates PROCESSOR_FEATURE_MAX (64) entries for processor feature IDs.
    features = sum(1 << feature for feature in range(64) if present(feature))
    return {"detected": {"kind": "windows_arm64", "features": features}}


def _windows_x86_64_cpu_info():
    try:
        import ctypes
    except ImportError as error:
        # Python can be built without the optional _ctypes extension.
        return {"unavailable": str(error)}

    # Probe inside the interpreter so emulation and per-process settings are reflected. Python
    # has no CPUID API, so the stub bridges the Windows x64 calling convention to CPUID's register
    # inputs and preserves RBX for its caller.
    cpuid = bytes.fromhex(
        "49 89 CA "  # mov r10, rcx
        "89 D0 "  # mov eax, edx
        "44 89 C1 "  # mov ecx, r8d
        "49 89 D9 "  # mov r9, rbx (callee-saved)
        "0F A2 "  # cpuid
        "41 89 02 "  # mov [r10], eax
        "41 89 5A 04 "  # mov [r10+4], ebx
        "41 89 4A 08 "  # mov [r10+8], ecx
        "41 89 52 0C "  # mov [r10+12], edx
        "4C 89 CB "  # mov rbx, r9
        "C3"  # ret
    )
    # CPUID alone cannot establish AVX usability: XCR0 (ECX = 0) records the register state
    # enabled by the OS. XGETBV returns EDX:EAX, but the callable stub must return it in RAX.
    xgetbv = bytes.fromhex(
        "31 C9 "  # xor ecx, ecx
        "0F 01 D0 "  # xgetbv
        "48 C1 E2 20 "  # shl rdx, 32
        "48 09 D0 "  # or rax, rdx
        "C3"  # ret
    )
    # Use x86 NOP padding (0x90) to align XGETBV to Control Flow Guard's 16-byte granularity.
    # https://learn.microsoft.com/en-us/windows/win32/secbp/pe-metadata#function-alignment
    xgetbv_offset = (len(cpuid) + 15) & ~15
    code = cpuid.ljust(xgetbv_offset, b"\x90") + xgetbv

    # Follow archspec's Windows x64 CPUID setup: CDLL avoids the allocation failures it
    # observed with windll on some 64-bit Python builds.
    # https://github.com/archspec/cpuid/blob/5b381ed2af05ad9ead46a136b43c2f7021bd44bb/cpuid.py#L85-L147
    kernel32 = ctypes.CDLL("kernel32.dll", use_last_error=True)
    pointer = ctypes.c_void_p
    size = ctypes.c_size_t
    word = ctypes.c_uint32
    kernel32.VirtualAlloc.argtypes = (pointer, size, word, word)
    kernel32.VirtualAlloc.restype = pointer
    # The remaining APIs return BOOL, which matches ctypes' default c_int return type.
    kernel32.VirtualProtect.argtypes = (pointer, size, word, ctypes.POINTER(word))
    kernel32.FlushInstructionCache.argtypes = (pointer, pointer, size)
    kernel32.VirtualFree.argtypes = (pointer, size, word)
    # Avoid memory that is writable and executable at once. Windows also requires instruction
    # cache coherence before executing dynamically generated code:
    # https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-virtualalloc#remarks
    # 0x3000 = MEM_COMMIT | MEM_RESERVE; 0x04 = PAGE_READWRITE.
    address = kernel32.VirtualAlloc(None, len(code), 0x3000, 0x04)
    if not address:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        ctypes.memmove(address, code, len(code))
        # 0x20 = PAGE_EXECUTE_READ.
        if not kernel32.VirtualProtect(address, len(code), 0x20, ctypes.byref(word())):
            error = ctypes.WinError(ctypes.get_last_error())
            # Process policy can deny executable memory (ERROR_ACCESS_DENIED or
            # ERROR_DYNAMIC_CODE_BLOCKED).
            if error.winerror in (5, 1655):
                return {"unavailable": str(error)}
            raise error
        # -1 is the current-process pseudo-handle.
        if not kernel32.FlushInstructionCache(pointer(-1), address, len(code)):
            raise ctypes.WinError(ctypes.get_last_error())
        return _query_cpuid(address, xgetbv_offset)
    finally:
        # MEM_RELEASE (0x8000) requires size 0.
        if not kernel32.VirtualFree(address, 0, 0x8000):
            raise ctypes.WinError(ctypes.get_last_error())


def _query_cpuid(address, xgetbv_offset):
    import ctypes

    probe = ctypes.CFUNCTYPE(
        None, ctypes.POINTER(ctypes.c_uint32), ctypes.c_uint32, ctypes.c_uint32
    )(address)
    xgetbv = ctypes.CFUNCTYPE(ctypes.c_uint64)(address + xgetbv_offset)
    leaves = []

    # Query advertised limits (basic: 0; extended: 0x80000000) to avoid unsupported leaves,
    # and cap each range at 64 so a bogus emulator response cannot trigger an excessive scan.
    xstate = 0  # Without XGETBV, Rust must not infer OS support for AVX.
    # XGETBV can fault without XSAVE (CPUID leaf 1 ECX bit 26) and OSXSAVE (bit 27).
    xsave_flags = (1 << 26) | (1 << 27)
    for base in (0, 0x80000000):
        limit = _query_cpuid_leaf(probe, base, 0)
        leaves.append(limit)
        maximum = min(limit["registers"][0], base + 64)
        for leaf in range(base + 1, maximum + 1):
            entry = _query_cpuid_leaf(probe, leaf, 0)
            leaves.append(entry)
            registers = entry["registers"]
            if leaf == 1 and registers[2] & xsave_flags == xsave_flags:
                xstate = xgetbv()
            elif leaf == 7:  # EAX gives the highest structured-feature subleaf.
                for subleaf in range(1, min(registers[0], 64) + 1):
                    leaves.append(_query_cpuid_leaf(probe, leaf, subleaf))
            elif leaf == 13:  # Leaf 0xD, subleaf 1: XSAVE feature flags.
                leaves.append(_query_cpuid_leaf(probe, leaf, 1))

    return {"detected": {"kind": "cpuid", "leaves": leaves, "xstate": xstate}}


def _query_cpuid_leaf(probe, leaf, subleaf):
    import ctypes

    # Instantiate the four-element uint32 array type as the probe's output buffer.
    # Its register order must match Rust's CpuidLeaf: EAX, EBX, ECX, EDX.
    registers = (ctypes.c_uint32 * 4)()
    probe(registers, leaf, subleaf)
    return {"leaf": leaf, "subleaf": subleaf, "registers": list(registers)}


def main() -> None:
    markers = {
        "implementation_name": implementation_name,
        "implementation_version": implementation_version,
        "os_name": os_name,
        "platform_machine": platform.machine(),
        "platform_python_implementation": platform.python_implementation(),
        "platform_release": platform.release(),
        "platform_system": platform.system(),
        "platform_version": platform.version(),
        "python_full_version": python_full_version,
        "python_version": ".".join(platform.python_version_tuple()[:2]),
        "sys_platform": sys.platform,
    }

    os, architecture = get_operating_system_and_architecture()

    manylinux_compatible = False

    if os["name"] == "manylinux":
        # noinspection PyProtectedMember
        from .packaging._manylinux import _get_glibc_version, _is_compatible

        manylinux_compatible = _is_compatible(
            arch=architecture, version=_get_glibc_version()
        )
    elif os["name"] == "musllinux":
        manylinux_compatible = True

    normalized_architecture = architecture.lower()
    arm64 = normalized_architecture in ("aarch64", "arm64")
    if arm64 or normalized_architecture in ("x86_64", "amd64"):
        try:
            cpu_info = _get_cpu_info(arm64=arm64)
        except Exception as error:  # noqa: BLE001
            # Rust applies the preview policy to unexpected failures.
            cpu_info = {"error": f"{type(error).__name__}: {error}"}
    else:
        cpu_info = {"unsupported_architecture": architecture}

    # By default, pip uses sysconfig on Python 3.10+.
    # But Python distributors can override this decision by setting:
    #     sysconfig._PIP_USE_SYSCONFIG = True / False
    # Rationale in https://github.com/pypa/pip/issues/10647
    use_sysconfig_scheme = bool(
        getattr(sysconfig, "_PIP_USE_SYSCONFIG", sys.version_info >= (3, 10))
    )

    # If we're not using sysconfig, make sure distutils is available.
    if not use_sysconfig_scheme:
        try:
            # Disable the use of the setuptools shim, if it's injected. Per pip:
            #
            # > If pip's going to use distutils, it should not be using the copy that setuptools
            # > might have injected into the environment. This is done by removing the injected
            # > shim, if it's injected.
            #
            # > See https://github.com/pypa/pip/issues/8761 for the original discussion and
            # > rationale for why this is done within pip.
            try:
                __import__("_distutils_hack").remove_shim()
            except (ImportError, AttributeError):
                pass

            import distutils.dist  # noqa: F401
        except ImportError:
            # We require distutils, but it's not installed; this is fairly
            # common in, e.g., deadsnakes where distutils is packaged
            # separately from Python.
            print(
                json.dumps(
                    {
                        "result": "error",
                        "kind": "missing_required_distutils",
                        "python_major": sys.version_info[0],
                        "python_minor": sys.version_info[1],
                    }
                )
            )
            sys.exit(0)

    interpreter_info = {
        "result": "success",
        "markers": markers,
        "sys_base_prefix": sys.base_prefix,
        "sys_base_exec_prefix": sys.base_exec_prefix,
        "sys_prefix": sys.prefix,
        "sys_base_executable": getattr(sys, "_base_executable", None),
        "sys_executable": sys.executable,
        # We prepend the location with the interpreter discovery script copied to a
        # temporary path to `sys.path` so we can import it, which we have to strip later
        # to avoid having this now-deleted path around.
        "sys_path": sys.path[1:],
        "site_packages": site.getsitepackages(),
        "stdlib": sysconfig.get_path("stdlib"),
        "extension_suffixes": importlib.machinery.EXTENSION_SUFFIXES,
        # Prior to the introduction of `sysconfig` patching, python-build-standalone installations would always use
        # "/install" as the prefix. With `sysconfig` patching, we rewrite the prefix to match the actual installation
        # location. So in newer versions, we also write a dedicated flag to indicate standalone builds.
        "standalone": (
            sysconfig.get_config_var("prefix") == "/install"
            or bool(sysconfig.get_config_var("PYTHON_BUILD_STANDALONE"))
        ),
        "scheme": get_scheme(use_sysconfig_scheme),
        "virtualenv": get_virtualenv(),
        "os": os,
        "arch": architecture,
        "cpu_info": cpu_info,
        "manylinux_compatible": manylinux_compatible,
        # The `t` abiflag for freethreading Python.
        # https://peps.python.org/pep-0703/#build-configuration-changes
        "gil_disabled": bool(sysconfig.get_config_var("Py_GIL_DISABLED")),
        # https://docs.python.org/3/using/configure.html#debug-build
        "debug_enabled": bool(sysconfig.get_config_var("Py_DEBUG")),
        # Determine if the interpreter is 32-bit or 64-bit.
        # https://github.com/python/cpython/blob/b228655c227b2ca298a8ffac44d14ce3d22f6faa/Lib/venv/__init__.py#L136
        "pointer_size": "64" if sys.maxsize > 2**32 else "32",
    }
    print(json.dumps(interpreter_info))


if __name__ == "__main__":
    main()
