from __future__ import annotations

import os
import sys
import sysconfig


class UvNotFound(FileNotFoundError): ...


def find_uv_bin() -> str:
    """Return the uv binary path."""

    uv_exe = "uv" + sysconfig.get_config_var("EXE")

    targets = [
        # The scripts directory for the current Python
        sysconfig.get_path("scripts"),
        # The scripts directory for the base prefix
        sysconfig.get_path("scripts", vars={"base": sys.base_prefix}),
        # The user scheme scripts directory, e.g., `~/.local/bin`
        sysconfig.get_path("scripts", scheme=_user_scheme()),
        # Above the package root, e.g., from `pip install --prefix`
        # with module path `<prefix>/lib/python3.13/site-packages/uv`
        _join(_parents(_module_path(), 4), "bin"),
        # Adjacent to the package root, e.g., from `pip install --target`
        # with module path `<target>/uv`
        _join(_parents(_module_path(), 1), "bin"),
    ]

    seen = set()
    for target in targets:
        if not target:
            continue
        if target in seen:
            continue
        seen.add(target)
        path = os.path.join(target, uv_exe)
        if os.path.isfile(path):
            return path

    raise UvNotFound(
        f"Could not find the uv binary in any of the following locations:\n"
        f"{os.linesep.join(f' - {target}' for target in seen)}\n"
    )


def _module_path() -> str | None:
    path = os.path.dirname(__file__)
    return path


def _parents(path: str | None, n: int) -> str | None:
    if not path:
        return None
    parts = path.split(os.sep)
    if len(parts) < n:
        return None
    return os.sep.join(parts[:-n])


def _join(path: str | None, *parts: str) -> str | None:
    if not path:
        return None
    return os.path.join(path, *parts)


def _user_scheme() -> str:
    if sys.version_info >= (3, 10):
        user_scheme = sysconfig.get_preferred_scheme("user")
    elif os.name == "nt":
        user_scheme = "nt_user"
    elif sys.platform == "darwin" and sys._framework:
        user_scheme = "osx_framework_user"
    else:
        user_scheme = "posix_user"
    return user_scheme
