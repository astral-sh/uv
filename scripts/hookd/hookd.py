#!/usr/bin/env python3
"""
A daemon process for PEP 517 build hook requests.

See the `README` for details.
"""
from __future__ import annotations

import enum
import importlib
import io
import json
import os
import sys
import tempfile
import time
import traceback
from contextlib import ExitStack, contextmanager
from functools import cache
from pathlib import Path
from typing import Any, Literal, Self, TextIO

SOURCE_TREE = os.getenv("HOOKD_SOURCE_TREE")

# Arbitrary nesting is allowed, but all keys and terminal values are strings
StringDict = dict[str, "str | StringDict"]


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


class Hook(enum.StrEnum):
    build_wheel = enum.auto()
    build_sdist = enum.auto()
    prepare_metadata_for_build_wheel = enum.auto()
    get_requires_for_build_wheel = enum.auto()
    get_requires_for_build_sdist = enum.auto()

    @classmethod
    def from_str(cls: type[Self], name: str) -> Self:
        try:
            return Hook(name)
        except ValueError:
            raise InvalidHookName(name) from None


def parse_build_backend(buffer: TextIO) -> str:
    # TODO: Add support for `build-path`
    name = buffer.readline().rstrip("\n")

    if not name:
        # Default to the legacy build name
        name = "setuptools.build_meta:__legacy__"

    return name


@cache
def import_build_backend(backend_name: str) -> object:
    """
    See: https://peps.python.org/pep-0517/#source-trees
    """
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


class Action(enum.StrEnum):
    run = enum.auto()
    shutdown = enum.auto()

    @classmethod
    def from_str(cls: type[Self], action: str) -> Self:
        try:
            return Action(action)
        except ValueError:
            raise InvalidAction(action) from None


def parse_action(buffer: TextIO) -> Action:
    action = buffer.readline().rstrip("\n")
    return Action.from_str(action)


def parse_hook_name(buffer: TextIO) -> Hook:
    name = buffer.readline().rstrip("\n")
    return Hook.from_str(name)


def parse_path(buffer: TextIO) -> Path:
    path = Path(buffer.readline().rstrip("\n"))
    # TODO(zanieb): Consider validating the path here
    return path


def parse_optional_path(buffer: TextIO) -> Path | None:
    data = buffer.readline().rstrip("\n")
    if not data:
        return None
    # TODO(zanieb): Consider validating the path here
    return Path(data)


def parse_config_settings(buffer: TextIO) -> StringDict | None:
    """
    See https://peps.python.org/pep-0517/#config-settings
    """
    data = buffer.readline().rstrip("\n")
    if not data:
        return None

    try:
        # TODO(zanieb): Consider using something faster than JSON here since we _should_
        #               be restricted to strings
        return json.loads(data)
    except json.decoder.JSONDecodeError as exc:
        raise MalformedHookArgument(data, HookArgument.config_settings) from exc


@contextmanager
def redirect_sys_stream(name: Literal["stdout", "stderr"]):
    """
    Redirect a system stream to a temporary file.

    Deletion of the temporary file is deferred to the caller.

    WARNING: This function is not safe to concurrent usage.
    """
    stream: TextIO = getattr(sys, name)

    # We intentionally do not context manage this file so it is around
    # as long as the parent needs to read from it
    redirect_file = tempfile.NamedTemporaryFile(delete=False)

    setattr(sys, name, io.TextIOWrapper(redirect_file))
    yield redirect_file.name

    # Restore to the previous stream
    setattr(sys, name, stream)


class HookArgument(enum.StrEnum):
    wheel_directory = enum.auto()
    config_settings = enum.auto()
    metadata_directory = enum.auto()
    sdist_directory = enum.auto()


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
    Hook.build_wheel: (
        HookArgument.wheel_directory,
        HookArgument.config_settings,
        HookArgument.metadata_directory,
    ),
    Hook.prepare_metadata_for_build_wheel: (
        HookArgument.metadata_directory,
        HookArgument.config_settings,
    ),
    Hook.get_requires_for_build_sdist: (HookArgument.config_settings,),
    Hook.get_requires_for_build_wheel: (HookArgument.config_settings,),
}


def write_safe(file: TextIO, *args: str):
    args = [str(arg).replace("\n", "\\n") for arg in args]
    print(*args, file=file)


def send_expect(file: TextIO, name: str):
    write_safe(file, "EXPECT", name)


def send_ready(file: TextIO):
    write_safe(file, "READY")


def send_shutdown(file: TextIO):
    write_safe(file, "SHUTDOWN")


def send_error(file: TextIO, exc: HookdError):
    write_safe(file, "ERROR", type(exc).__name__, str(exc))
    send_traceback(file, exc)


def send_traceback(file: TextIO, exc: BaseException):
    tb = traceback.format_exception(exc)
    write_safe(file, "TRACEBACK", "\n".join(tb))


def send_ok(file: TextIO, result: str):
    write_safe(file, "OK", result)


def send_fatal(file: TextIO, exc: BaseException):
    write_safe(file, "FATAL", type(exc).__name__, str(exc))
    send_traceback(file, exc)


def send_debug(file: TextIO, *args):
    write_safe(file, "DEBUG", *args)


def send_redirect(file: TextIO, name: Literal["stdout", "stderr"], path: str):
    write_safe(file, name.upper(), path)


def run_once(stdin: TextIO, stdout: TextIO):
    start = time.perf_counter()

    send_expect(stdout, "build-backend")
    build_backend_name = parse_build_backend(stdin)

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
        build_backend_name,
        hook_name,
        *(f"{name}={value}" for name, value in zip(HookArguments[hook_name], args)),
    )

    end = time.perf_counter()
    send_debug(stdout, f"parsed hook inputs in {(end - start)*1000.0:.2f}ms")

    # All hooks are run with working directory set to the root of the source tree
    # TODO(zanieb): Where do we get the path of the source tree?

    with ExitStack() as hook_ctx:
        hook_stdout = hook_ctx.enter_context(redirect_sys_stream("stdout"))
        hook_stderr = hook_ctx.enter_context(redirect_sys_stream("stderr"))
        send_redirect(stdout, "stdout", str(hook_stdout))
        send_redirect(stdout, "stderr", str(hook_stderr))

        try:
            build_backend = import_build_backend(build_backend_name)
        except Exception as exc:
            if not isinstance(exc, HookdError):
                # Wrap unhandled errors in a generic one
                raise BackendImportError(exc) from exc
            raise

        try:
            hook = getattr(build_backend, hook_name)
        except AttributeError:
            raise UnsupportedHook(build_backend, hook_name)

        try:
            result = hook(*args)
        except BaseException as exc:
            # Respect SIGTERM and SIGINT
            if isinstance(exc, (SystemExit, KeyboardInterrupt)):
                raise

            raise HookRuntimeError(exc) from exc
        else:
            send_ok(stdout, result)


def main():
    # Create copies of standard streams since the `sys.<name>` will be redirected during
    # hook execution
    stdout = sys.stdout
    stdin = sys.stdin

    # TODO: Close `sys.stdin` and create a duplicate file for ourselves so hooks don't read from our stream

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
            send_debug(stdout, f"ran hook in {(end - start)*1000.0:.2f}ms")

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


if __name__ == "__main__":
    if len(sys.argv) > 2:
        print(
            "Invalid usage. Expected one argument specifying the path to the source tree.",
            file=sys.stderr,
        )
        sys.exit(1)

    try:
        source_tree = Path(sys.argv[1]).resolve()
        os.chdir(source_tree)
        send_debug(sys.stdout, "changed working directory to", source_tree)
    except IndexError:
        pass
    except ValueError as exc:
        print(
            f"Invalid usage. Expected path argument but validation failed: {exc}",
            file=sys.stderr,
        )
        sys.exit(1)

    main()
