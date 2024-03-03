import sys
import os
import sysconfig
import typing


# Notes on _infer_* functions.
# Unfortunately ``get_default_scheme()`` didn't exist before 3.10, so there's no
# way to ask things like "what is the '_prefix' scheme on this platform". These
# functions try to answer that with some heuristics while accounting for ad-hoc
# platforms not covered by CPython's default sysconfig implementation. If the
# ad-hoc implementation does not fully implement sysconfig, we'll fall back to
# a POSIX scheme.

_AVAILABLE_SCHEMES = set(sysconfig.get_scheme_names())

_PREFERRED_SCHEME_API = getattr(sysconfig, "get_preferred_scheme", None)


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


def _running_under_virtualenv() -> bool:
    """True if we're running inside a virtual environment, False otherwise."""
    return _running_under_venv() or _running_under_legacy_virtualenv()


def _is_osx_framework() -> bool:
    return bool(sysconfig.get_config_var("PYTHONFRAMEWORK"))


def _get_major_minor_version() -> str:
    """
    Return the major-minor version of the current Python as a string, e.g.
    "3.7" or "3.10".
    """
    return "{}.{}".format(*sys.version_info)


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
        and not _running_under_virtualenv()
        and _is_osx_framework()
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


SCHEME_KEYS = ["platlib", "purelib", "headers", "scripts", "data"]


class Scheme:
    """A Scheme holds paths which are used as the base directories for
    artifacts associated with a Python package.
    """

    __slots__ = SCHEME_KEYS

    def __init__(
        self,
        platlib: str,
        purelib: str,
        include: str,
        scripts: str,
        data: str,
    ) -> None:
        self.platlib = platlib
        self.purelib = purelib
        self.include = include
        self.scripts = scripts
        self.data = data


def _sysconfig_get_scheme():
    """Get the "scheme" corresponding to the input parameters."""
    scheme_name = _infer_prefix()
    paths =  sysconfig.get_paths(scheme=scheme_name)

    # Logic here is very arbitrary, we're doing it for compatibility, don't ask.
    # 1. Pip historically uses a special header path in virtual environments.
    # 2. If the distribution name is not known, distutils uses 'UNKNOWN'. We
    #    only do the same when not running in a virtual environment because
    #    pip's historical header path logic (see point 1) did not do this.
    if _running_under_virtualenv():
        python_xy = f"python{_get_major_minor_version()}"
        paths["include"] = os.path.join(sys.prefix, "include", "site", python_xy)

    return Scheme(
        platlib=paths["platlib"],
        purelib=paths["purelib"],
        include=paths["include"],
        scripts=paths["scripts"],
        data=paths["data"],
    )



_USE_SYSCONFIG_DEFAULT = sys.version_info >= (3, 10)


def _should_use_sysconfig() -> bool:
    """This function determines the value of _USE_SYSCONFIG.

    By default, pip uses sysconfig on Python 3.10+.
    But Python distributors can override this decision by setting:
        sysconfig._PIP_USE_SYSCONFIG = True / False
    Rationale in https://github.com/pypa/pip/issues/10647

    This is a function for testability, but should be constant during any one
    run.
    """
    return bool(getattr(sysconfig, "_PIP_USE_SYSCONFIG", _USE_SYSCONFIG_DEFAULT))


_USE_SYSCONFIG = _should_use_sysconfig()


def get_scheme() -> Scheme:
    new = _sysconfig_get_scheme()
    if _USE_SYSCONFIG:
        scheme = _sysconfig_get_scheme()
    else:
        scheme = _distutils.get_scheme(
            dist_name,
            user=user,
            home=home,
            root=root,
            isolated=isolated,
            prefix=prefix,
        )
