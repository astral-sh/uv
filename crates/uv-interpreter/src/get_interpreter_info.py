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
if len(python_full_version) > 0 and python_full_version[-1] == '+':
    python_full_version = python_full_version[:-1]

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
    "sys_executable": sys.executable,
    "sysconfig_paths": sysconfig.get_paths(),
}
print(json.dumps(interpreter_info))
