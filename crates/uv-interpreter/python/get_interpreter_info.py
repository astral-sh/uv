"""
Queries information about the current Python interpreter and prints it as JSON.

The script will exit with status 0 on known error that are turned into rust errors.
"""

import json
import os
import platform
import sys
import sysconfig


def format_full_version(info):
    version = "{0.major}.{0.minor}.{0.micro}".format(info)
    kind = info.releaselevel
    if kind != "final":
        version += kind[0] + str(info.serial)
    return version


if sys.version_info[0] < 3:
    print(json.dumps({"result": "error", "kind": "unsupported_python_version"}))
    sys.exit(0)

if hasattr(sys, "implementation"):
    implementation_version = format_full_version(sys.implementation.version)
    implementation_name = sys.implementation.name
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

        # Use distutils primarily because that's what pip does.
        # https://github.com/pypa/pip/blob/ae5fff36b0aad6e5e0037884927eaa29163c0611/src/pip/_internal/locations/__init__.py#L249
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


def get_scheme():
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

    # By default, pip uses sysconfig on Python 3.10+.
    # But Python distributors can override this decision by setting:
    #     sysconfig._PIP_USE_SYSCONFIG = True / False
    # Rationale in https://github.com/pypa/pip/issues/10647
    use_sysconfig = bool(
        getattr(sysconfig, "_PIP_USE_SYSCONFIG", sys.version_info >= (3, 10))
    )

    if use_sysconfig:
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
    [operating_system, version_arch] = sysconfig.get_platform().split("-", 1)
    if "-" in version_arch:
        # Ex: macosx-11.2-arm64
        version, architecture = version_arch.rsplit("-", 1)
    else:
        # Ex: linux-x86_64
        version = None
        architecture = version_arch

    if operating_system == "linux":
        # noinspection PyProtectedMember
        from .packaging._manylinux import _get_glibc_version

        # noinspection PyProtectedMember
        from .packaging._musllinux import _get_musl_version

        musl_version = _get_musl_version(sys.executable)
        glibc_version = _get_glibc_version()
        if musl_version:
            operating_system = {
                "name": "musllinux",
                "major": musl_version[0],
                "minor": musl_version[1],
            }
        elif glibc_version:
            operating_system = {
                "name": "manylinux",
                "major": glibc_version[0],
                "minor": glibc_version[1],
            }
        else:
            print(json.dumps({"result": "error", "kind": "libc_not_found"}))
            sys.exit(0)
    elif operating_system == "win":
        operating_system = {
            "name": "windows",
        }
    elif operating_system == "macosx":
        # GitHub Actions python seems to be doing this.
        if architecture == "universal2":
            if platform.processor() == "arm":
                architecture = "aarch64"
            else:
                architecture = platform.processor()
        version = platform.mac_ver()[0].split(".")
        operating_system = {
            "name": "macos",
            "major": int(version[0]),
            "minor": int(version[1]),
        }
    elif operating_system in [
        "freebsd",
        "netbsd",
        "openbsd",
        "dragonfly",
        "illumos",
        "haiku",
    ]:
        version = platform.mac_ver()[0].split(".")
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
    return {"os": operating_system, "arch": architecture}


markers = {
    "implementation_name": implementation_name,
    "implementation_version": implementation_version,
    "os_name": os.name,
    "platform_machine": platform.machine(),
    "platform_python_implementation": platform.python_implementation(),
    "platform_release": platform.release(),
    "platform_system": platform.system(),
    "platform_version": platform.version(),
    "python_full_version": python_full_version,
    "python_version": ".".join(platform.python_version_tuple()[:2]),
    "sys_platform": sys.platform,
}
interpreter_info = {
    "result": "success",
    "markers": markers,
    "base_prefix": sys.base_prefix,
    "base_exec_prefix": sys.base_exec_prefix,
    "prefix": sys.prefix,
    "base_executable": getattr(sys, "_base_executable", None),
    "sys_executable": sys.executable,
    "stdlib": sysconfig.get_path("stdlib"),
    "scheme": get_scheme(),
    "virtualenv": get_virtualenv(),
    "platform": get_operating_system_and_architecture(),
}
print(json.dumps(interpreter_info))
