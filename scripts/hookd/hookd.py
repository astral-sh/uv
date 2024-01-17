#!/usr/bin/env python3
"""
A daemon process for PEP 517 build hook requests.

See the `README` for details.
"""
from __future__ import annotations

import enum
import importlib
import io
import errno
import os
import sys
import time
from types import ModuleType
from contextlib import ExitStack, contextmanager
from typing import Any, TextIO, TYPE_CHECKING

Path = str  # We don't use `pathlib` for a modest speedup

if TYPE_CHECKING:
    if sys.version_info > (3, 7):
        from typing import Literal, Self

        StreamName = Literal["stdout", "stderr"]
    else:
        StreamName = str
        Self = Any

DEBUG = os.getenv("HOOKD_DEBUG")
INIT_MODULES = set(sys.modules.keys())


def main():
    # First, duplicate the original stdout since it will be redirected later
    stdout_fd = os.dup(sys.stdout.fileno())
    stdout = os.fdopen(stdout_fd, "wt")

    # TODO: Close `sys.stdin` and create a duplicate file for ourselves so hooks don't read from our stream
    stdin = sys.stdin

    while True:
        try:
            start = time.perf_counter()

            if not stdin.readable():
                raise UnreadableInput()

            send_ready(stdout)

            send_expect(stdout, "action")
            action = parse_action(stdin)
            if action == Action.shutdown:
                send_shutdown(stdout)
                break

            run_once(stdin, stdout)
            end = time.perf_counter()
            send_debug(stdout, f"Ran hook in {(end - start)*1000.0:.2f}ms")

        except HookdError as exc:
            # These errors are "handled" and non-fatal
            try:
                send_error(stdout, exc)
            except Exception as exc:
                # Failures to report errors are a fatal error
                send_fatal(stdout, exc)
                raise exc
        except BaseException as exc:
            # All other exceptions result in a crash of the daemon
            send_fatal(stdout, exc)
            raise


def run_once(stdin: TextIO, stdout: TextIO):
    start = time.perf_counter()

    send_expect(stdout, "build-backend")
    build_backend_name = parse_build_backend(stdin)

    send_expect(stdout, "backend-path")
    backend_path = parse_backend_path(stdin)

    send_expect(stdout, "hook-name")
    hook_name = parse_hook_name(stdin)
    if hook_name not in HookArguments:
        raise FatalError(f"No arguments defined for hook {hook_name!r}")

    # Parse arguments for the given hook
    def parse(argument: str):
        send_expect(stdout, argument.name)
        return parse_hook_argument(argument, stdin)

    args = tuple(parse(argument) for argument in HookArguments[hook_name])

    send_debug(
        stdout,
        "Calling {}.{}({})".format(
            build_backend_name,
            hook_name.value,
            ", ".join(
                f"{name.value}={value!r}"
                for name, value in zip(HookArguments[hook_name], args)
            ),
        ),
    )

    end = time.perf_counter()
    send_debug(stdout, f"Parsed hook inputs in {(end - start)*1000.0:.2f}ms")

    with ExitStack() as hook_ctx:
        hook_ctx.enter_context(update_sys_path(backend_path))
        hook_stdout = hook_ctx.enter_context(redirect_sys_stream("stdout"))
        hook_stderr = hook_ctx.enter_context(redirect_sys_stream("stderr"))
        send_redirect(stdout, "stdout", str(hook_stdout))
        send_redirect(stdout, "stderr", str(hook_stderr))

        try:
            build_backend = import_build_backend(
                stdout, build_backend_name, backend_path
            )
        except Exception as exc:
            if not isinstance(exc, HookdError):
                # Wrap unhandled errors in a generic one
                raise BackendImportError(exc) from exc
            raise

        try:
            hook = getattr(build_backend, hook_name.value)
        except AttributeError:
            raise UnsupportedHook(build_backend, hook_name)

        try:
            result = hook(*args)
        except BaseException as exc:
            # Respect SIGTERM and SIGINT

            if (
                isinstance(exc, (SystemExit, KeyboardInterrupt))
                and build_backend_name != "setuptools.build_meta:__legacy__"
            ):
                raise

            raise HookRuntimeError(exc) from exc
        else:
            send_ok(stdout, result)


def import_build_backend(
    stdout,
    backend_name: str,
    backend_path: tuple[str],
) -> object:
    """
    See: https://peps.python.org/pep-0517/#source-trees
    """
    # Invalidate the module caches before resetting modules
    importlib.invalidate_caches()

    for module in tuple(sys.modules.keys()):
        # We remove all of the modules that have been added since the daemon started
        # If a new dependency is added without resetting modules, build backends
        # can end up in a broken state. We cannot simply reset the backend module
        # using `importtools.reload` because types can become out of sync across
        # packages e.g. breaking `isinstance` calls.
        if module not in INIT_MODULES:
            sys.modules.pop(module)

    parts = backend_name.split(":")
    if len(parts) == 1:
        module_name = parts[0]
        attribute = None
    elif len(parts) == 2:
        module_name = parts[0]
        attribute = parts[1]

        # Check for malformed attribute
        if not attribute:
            raise MalformedBackendName(backend_name)
    else:
        raise MalformedBackendName(backend_name)

    module = None
    backend = None

    try:
        module = importlib.import_module(module_name)
    except ImportError:
        # If they could not have meant `<module>.<attribute>`, raise
        if "." not in module_name:
            raise MissingBackendModule(module_name)

    if module is None:
        # Otherwise, we'll try to load it as an attribute of a module
        parent_name, child_name = module_name.rsplit(".", 1)

        try:
            module = importlib.import_module(parent_name)
        except ImportError:
            raise MissingBackendModule(module_name)

        try:
            backend = getattr(module, child_name)
        except AttributeError:
            raise MissingBackendAttribute(module_name, child_name)

    if attribute is not None:
        try:
            backend = getattr(module, attribute)
        except AttributeError:
            raise MissingBackendAttribute(module_name, backend_name)

    if backend is None:
        backend = module

    return backend


@contextmanager
def update_sys_path(extensions: tuple[Path]):
    """
    Temporarily update `sys.path`.

    WARNING: This function is not safe to concurrent usage.
    """
    if not extensions:
        yield
        return

    previous_path = sys.path.copy()

    sys.path = list(extensions) + sys.path
    yield
    sys.path = previous_path


@contextmanager
def redirect_sys_stream(name: StreamName):
    """
    Redirect a system stream to a temporary file.

    Deletion of the temporary file is deferred to the caller.

    WARNING: This function is not safe to concurrent usage.
    """
    stream: TextIO = getattr(sys, name)

    # We use an optimized version of `NamedTemporaryFile`
    fd, path = tmpfile()

    # Copy the temporary fd to sys.stdout's fd
    # Using `dup2` instead of `setatrr(sys, name, open(fd, "wt"))` ensures that
    # subprocess write to the redirected file
    os.dup2(fd, stream.fileno())

    yield path

    # Ensure the stream is fully written at the end
    stream.flush()

    # Restore the previous stream
    os.dup2(stream.fileno(), fd)


######################
###### PARSERS #######
######################


class Hook(enum.Enum):
    build_wheel = "build_wheel"
    prepare_metadata_for_build_wheel = "prepare_metadata_for_build_wheel"
    get_requires_for_build_wheel = "get_requires_for_build_wheel"

    build_editable = "build_editable"
    prepare_metadata_for_build_editable = "prepare_metadata_for_build_editable"
    get_requires_for_build_editable = "get_requires_for_build_editable"

    build_sdist = "build_sdist"
    get_requires_for_build_sdist = "get_requires_for_build_sdist"

    @classmethod
    def from_str(cls: type[Self], name: str) -> Self:
        try:
            return Hook(name)
        except ValueError:
            raise InvalidHookName(name) from None


class HookArgument(enum.Enum):
    wheel_directory = "wheel_directory"
    config_settings = "config_settings"
    metadata_directory = "metadata_directory"
    sdist_directory = "sdist_directory"


def parse_hook_argument(hook_arg: HookArgument, buffer: TextIO) -> Any:
    if hook_arg == HookArgument.wheel_directory:
        return parse_path(buffer)
    if hook_arg == HookArgument.metadata_directory:
        return parse_optional_path(buffer)
    if hook_arg == HookArgument.sdist_directory:
        return parse_path(buffer)
    if hook_arg == HookArgument.config_settings:
        return parse_config_settings(buffer)

    raise FatalError(f"No parser for hook argument kind {hook_arg.name!r}")


HookArguments = {
    Hook.build_sdist: (
        HookArgument.sdist_directory,
        HookArgument.config_settings,
    ),
    Hook.get_requires_for_build_sdist: (HookArgument.config_settings,),
    Hook.build_wheel: (
        HookArgument.wheel_directory,
        HookArgument.config_settings,
        HookArgument.metadata_directory,
    ),
    Hook.prepare_metadata_for_build_wheel: (
        HookArgument.metadata_directory,
        HookArgument.config_settings,
    ),
    Hook.get_requires_for_build_wheel: (HookArgument.config_settings,),
    Hook.build_editable: (
        HookArgument.wheel_directory,
        HookArgument.config_settings,
        HookArgument.metadata_directory,
    ),
    Hook.prepare_metadata_for_build_editable: (
        HookArgument.metadata_directory,
        HookArgument.config_settings,
    ),
    Hook.get_requires_for_build_editable: (HookArgument.config_settings,),
}


class Action(enum.Enum):
    run = "run"
    shutdown = "shutdown"

    @classmethod
    def from_str(cls: type[Self], action: str) -> Self:
        try:
            return Action(action)
        except ValueError:
            raise InvalidAction(action) from None


def parse_action(buffer: TextIO) -> Action:
    # Wait until we receive non-empty content
    action = None
    while not action:
        action = buffer.readline().rstrip("\n")
    return Action.from_str(action)


def parse_hook_name(buffer: TextIO) -> Hook:
    name = buffer.readline().rstrip("\n")
    return Hook.from_str(name)


def parse_path(buffer: TextIO) -> Path:
    path = os.path.abspath(buffer.readline().rstrip("\n"))
    return path


def parse_optional_path(buffer: TextIO) -> Path | None:
    data = buffer.readline().rstrip("\n")
    if not data:
        return None
    # TODO(zanieb): Consider validating the path here
    return os.path.abspath(data)


def parse_config_settings(buffer: TextIO) -> dict | None:
    """
    See https://peps.python.org/pep-0517/#config-settings
    """
    data = buffer.readline().rstrip("\n")
    if not data:
        return None

    # We defer the import of `json` until someone actually passes us a `config_settings`
    # object since it's not necessarily common
    import json

    try:
        # TODO(zanieb): Consider using something faster than JSON here since we _should_
        #               be restricted to strings
        return json.loads(data)
    except json.decoder.JSONDecodeError as exc:
        raise MalformedHookArgument(data, HookArgument.config_settings) from exc


def parse_build_backend(buffer: TextIO) -> str:
    name = buffer.readline().rstrip("\n")

    if not name:
        # Default to the legacy build name
        name = "setuptools.build_meta:__legacy__"

    return name


def parse_backend_path(buffer: TextIO) -> tuple[Path]:
    """
    Directories in backend-path are interpreted as relative to the project root, and MUST refer to a location within the source tree (after relative paths and symbolic links have been resolved).
    The backend code MUST be loaded from one of the directories specified in backend-path (i.e., it is not permitted to specify backend-path and not have in-tree backend code).
    """
    paths = []

    while True:
        path = parse_optional_path(buffer)
        if not path:
            return tuple(paths)

        paths.append(path)


######################
####### OUTPUT #######
######################


def send_ready(file: TextIO):
    write_safe(file, "READY")


def send_expect(file: TextIO, name: str):
    write_safe(file, "EXPECT", name)


def send_redirect(file: TextIO, name: StreamName, path: str):
    write_safe(file, name.upper(), path)


def send_ok(file: TextIO, result: str):
    write_safe(file, "OK", result)


def send_error(file: TextIO, exc: HookdError):
    write_safe(file, "ERROR", type(exc).__name__, str(exc))
    send_traceback(file, exc)


def send_traceback(file: TextIO, exc: BaseException):
    # Defer import of traceback until an exception occurs
    import traceback

    if sys.version_info < (3, 8):
        tb = traceback.format_exception(type(exc), exc, exc.__traceback__)
    else:
        tb = traceback.format_exception(exc)

    write_safe(file, "TRACEBACK", "\n".join(tb))


def send_fatal(file: TextIO, exc: BaseException):
    write_safe(file, "FATAL", type(exc).__name__, str(exc))
    send_traceback(file, exc)


def send_debug(file: TextIO, *args):
    write_safe(file, "DEBUG", *args)


def send_shutdown(file: TextIO):
    write_safe(file, "SHUTDOWN")


def write_safe(file: TextIO, *args: str):
    # Ensures thre are no newlines in the output
    args = [str(arg).replace("\n", "\\n") for arg in args]
    print(*args, file=file, flush=True)

    if DEBUG:
        print(*args, file=sys.stderr, flush=True)


#######################
####### ERRORS ########
#######################


class FatalError(Exception):
    """An unrecoverable error in the daemon"""

    def __init__(self, *args) -> None:
        super().__init__(*args)


class UnreadableInput(FatalError):
    """Standard input is not readable"""

    def __init__(self, reason: str) -> None:
        super().__init__("Standard input is not readable " + reason)


class HookdError(Exception):
    """A non-fatal exception related the this program"""

    def message(self) -> str:
        pass

    def __repr__(self) -> str:
        attributes = ", ".join(
            f"{key}={value!r}" for key, value in self.__dict__.items()
        )
        return f"{type(self)}({attributes})"

    def __str__(self) -> str:
        return self.message()


class MissingBackendModule(HookdError):
    """A backend was not found"""

    def __init__(self, name: str) -> None:
        self.name = name
        super().__init__()

    def message(self) -> str:
        return f"Failed to import the backend {self.name!r}"


class MissingBackendAttribute(HookdError):
    """A backend attribute was not found"""

    def __init__(self, module: str, attr: str) -> None:
        self.attr = attr
        self.module = module
        super().__init__()

    def message(self) -> str:
        return f"Failed to find attribute {self.attr!r} in the backend module {self.module!r}"


class MalformedBackendName(HookdError):
    """A backend is not valid"""

    def __init__(self, name: str) -> None:
        self.name = name
        super().__init__()

    def message(self) -> str:
        return f"Backend {self.name!r} is malformed"


class BackendImportError(HookdError):
    """A backend raised an exception on import"""

    def __init__(self, exc: Exception) -> None:
        self.exc = exc
        super().__init__()

    def message(self) -> str:
        return f"Backend threw an exception during import: {self.exc}"


class InvalidHookName(HookdError):
    """A parsed hook name is not valid"""

    def __init__(self, name: str) -> None:
        self.name = name
        super().__init__()

    def message(self) -> str:
        names = ", ".join(repr(name) for name in Hook._member_names_)
        return f"The name {self.name!r} is not valid hook. Expected one of: {names}"


class InvalidAction(HookdError):
    """The given action is not valid"""

    def __init__(self, name: str) -> None:
        self.name = name
        super().__init__()

    def message(self) -> str:
        names = ", ".join(repr(name) for name in Action._member_names_)
        return f"Received invalid action {self.name!r}. Expected one of: {names}"


class UnsupportedHook(HookdError):
    """A hook is not supported by the backend"""

    def __init__(self, backend: object, hook: Hook) -> None:
        self.backend = backend
        self.hook = hook
        super().__init__()

    def message(self) -> str:
        hook_names = set(Hook._member_names_)
        names = ", ".join(
            repr(name) for name in dir(self.backend) if name in hook_names
        )
        hint = (
            f"The backend supports: {names}"
            if names
            else "The backend does not support any known hooks."
        )
        return f"The hook {self.hook.value!r} is not supported by the backend. {hint}"


class MalformedHookArgument(HookdError):
    """A parsed hook argument was not in the expected format"""

    def __init__(self, raw: str, argument: HookArgument) -> None:
        self.raw = raw
        self.argument = argument
        super().__init__()

    def message(self) -> str:
        # TODO(zanieb): Consider display an expected type
        return f"Malformed content for argument {self.argument.name!r}: {self.raw!r}"


class HookRuntimeError(HookdError):
    """Execution of a hook failed"""

    def __init__(self, exc: BaseException) -> None:
        self.exc = exc
        super().__init__()

    def message(self) -> str:
        return str(self.exc)


##########################
#### TEMPORARY FILES #####
##########################


_text_openflags = os.O_RDWR | os.O_CREAT | os.O_EXCL
if hasattr(os, "O_NOFOLLOW"):
    _text_openflags |= os.O_NOFOLLOW


def _candidate_tempdirs():
    """
    Generate a list of candidate temporary directories
    """
    dirlist = []

    # First, try the environment.
    for envname in "TMPDIR", "TEMP", "TMP":
        dirname = os.getenv(envname)
        if dirname:
            dirlist.append(dirname)

    # Failing that, try OS-specific locations.
    if os.name == "nt":
        dirlist.extend(
            [
                os.path.expanduser(r"~\AppData\Local\Temp"),
                os.path.expandvars(r"%SYSTEMROOT%\Temp"),
                r"c:\temp",
                r"c:\tmp",
                r"\temp",
                r"\tmp",
            ]
        )
    else:
        dirlist.extend(["/tmp", "/var/tmp", "/usr/tmp"])

    # As a last resort, the current directory.
    try:
        dirlist.append(os.getcwd())
    except (AttributeError, OSError):
        dirlist.append(os.curdir)

    return dirlist


_default_tmpdir = None
_max_tmpfile_attempts = 10000


def tmpfile():
    """
    Optimized version of temporary file creation based on CPython's `NamedTemporaryFile`.

    Profiling shows that temporary file creation for stdout and stderr is the most expensive
    part of running a build hook.

    Notable differences:

    - Uses UUIDs instead of the CPython random name generator
    - Finds a valid temporary directory at the same time as creating the temporary file
        - Avoids having to unlink a file created just to test if the directory is valid
    - Only finds the default temporary directory _once_ then caches it
    - Does not manage deletion of the file
    """
    global _default_tmpdir

    # Use the default directory if previously found, otherwise we will
    # find
    if not _default_tmpdir:
        tmpdir = None
        candidate_tempdirs = iter(_candidate_tempdirs())
    else:
        tmpdir = _default_tmpdir
        candidate_tempdirs = None

    for attempt in range(_max_tmpfile_attempts):
        # Generate a random hex string, similar to a UUID without version and variant information
        name = "%032x" % int.from_bytes(os.urandom(16), sys.byteorder)

        # Every one hundred attempts, switch to another candidate directory
        if not _default_tmpdir and attempt % 100 == 0:
            try:
                tmpdir = next(candidate_tempdirs)
            except StopIteration:
                raise FileNotFoundError(
                    errno.ENOENT,
                    f"No usable temporary directory found in {_candidate_tempdirs()}",
                ) from None

        file = os.path.join(tmpdir, name)
        try:
            fd = os.open(file, _text_openflags, 0o600)
        except FileExistsError:
            continue  # try again
        except PermissionError:
            # This exception is thrown when a directory with the chosen name
            # already exists on windows.
            if (
                os.name == "nt"
                and os.path.isdir(_default_tmpdir)
                and os.access(dir, os.W_OK)
            ):
                continue
            else:
                raise

        _default_tmpdir = tmpdir
        return fd, file

    raise FileExistsError(errno.EEXIST, "No usable temporary file name found")


#########################
#### CLI ENTRYPOINT #####
#########################


if __name__ == "__main__":
    if len(sys.argv) > 2:
        print(
            "Invalid usage. Expected one argument specifying the path to the source tree.",
            file=sys.stderr,
        )
        sys.exit(1)

    try:
        source_tree = os.path.abspath(sys.argv[1])
        os.chdir(source_tree)
        send_debug(sys.stdout, "Changed working directory to", source_tree)
    except IndexError:
        pass
    except ValueError as exc:
        print(
            f"Invalid usage. Expected path argument but validation failed: {exc}",
            file=sys.stderr,
        )
        sys.exit(1)

    main()
