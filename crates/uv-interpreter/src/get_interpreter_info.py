""""
Queries information about the current Python interpreter and prints it as JSON.

Exit Codes:
    0: Success
    1: General failure
    3: Python version 3 or newer is required
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
    sys.exit(3)


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
            "include": expand_path(sysconfig_paths["include"]),
            "scripts": expand_path(sysconfig_paths["scripts"]),
            "data": expand_path(sysconfig_paths["data"]),
        }
    else:
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
            "include": os.path.dirname(distutils_paths["headers"]),
            "scripts": distutils_paths["scripts"],
            "data": distutils_paths["data"],
        }


def get_scheme():
    """Return the Scheme for the current interpreter.

    The paths returned should be absolute.
    """
    # TODO(charlie): Use distutils on required Python distributions.
    paths = sysconfig.get_paths()
    return {
        "purelib": paths["purelib"],
        "platlib": paths["platlib"],
        "include": paths["include"],
        "scripts": paths["scripts"],
        "data": paths["data"],
    }


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
    "markers": markers,
    "base_prefix": sys.base_prefix,
    "base_exec_prefix": sys.base_exec_prefix,
    "prefix": sys.prefix,
    "base_executable": getattr(sys, "_base_executable", None),
    "sys_executable": sys.executable,
    "stdlib": sysconfig.get_path("stdlib"),
    "scheme": get_scheme(),
    "virtualenv": get_virtualenv(),
}
print(json.dumps(interpreter_info))
