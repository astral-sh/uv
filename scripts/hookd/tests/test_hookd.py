import importlib
import os
import re
import subprocess
import sys
import textwrap
from pathlib import Path

import pytest

PROJECT_DIR = Path(__file__).parent.parent

# Snapshot filters
TIME = (r"(\d+.)?\d+(ms|s)", "[TIME]")
SHUTDOWN = (
    textwrap.dedent(
        """
        READY
        EXPECT action
        SHUTDOWN
        """
    ).lstrip(),
    "",
)
STDOUT = ("STDOUT .*", "STDOUT [PATH]")
STDERR = ("STDERR .*", "STDERR [PATH]")


def new() -> subprocess.Popen:
    env = os.environ.copy()
    # Add the test backends to the Python path
    env["PYTHONPATH"] = PROJECT_DIR / "backends"

    return subprocess.Popen(
        [sys.executable, str(PROJECT_DIR / "hookd.py")],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=env,
    )


def send(process, lines):
    process.stdin.write("\n".join(lines) + "\n")


def assert_snapshot(value, expected, filters=None):
    filters = filters or []
    for pattern, replace in filters:
        value = re.sub(pattern, replace, value)
    print(value)
    assert value == textwrap.dedent(expected).lstrip()


def test_shutdown():
    daemon = new()
    daemon.communicate(input="shutdown\n")
    assert daemon.returncode == 0


def test_sigkill():
    daemon = new()
    daemon.kill()
    daemon.wait()
    assert daemon.returncode == -9


def test_sigterm():
    daemon = new()
    daemon.terminate()
    daemon.wait()
    assert daemon.returncode == -15


def test_run_invalid_backend():
    daemon = new()
    send(daemon, ["run", "backend_does_not_exist"])
    stdout, stderr = daemon.communicate(input="shutdown\n")
    assert_snapshot(
        stdout,
        """
        READY
        EXPECT action
        EXPECT build-backend
        ERROR MissingBackendModule Failed to import the backend 'backend_does_not_exist'
        READY
        EXPECT action
        SHUTDOWN
        """,
    )
    assert stderr == ""
    assert daemon.returncode == 0


def test_run_invalid_hook():
    daemon = new()
    send(daemon, ["run", "ok_backend", "hook_does_not_exist"])
    stdout, stderr = daemon.communicate(input="shutdown\n")
    assert_snapshot(
        stdout,
        """
        READY
        EXPECT action
        EXPECT build-backend
        EXPECT hook-name
        ERROR InvalidHookName The name 'hook_does_not_exist' is not valid hook. Expected one of: 'build_wheel', 'build_sdist', 'prepare_metadata_for_build_wheel', 'get_requires_for_build_wheel', 'get_requires_for_build_sdist'
        READY
        EXPECT action
        SHUTDOWN
        """,
    )
    assert stderr == ""
    assert daemon.returncode == 0


def test_run_build_wheel_ok():
    """
    Uses a mock backend to test the `build_wheel` hook.
    """
    daemon = new()
    send(daemon, ["run", "ok_backend", "build_wheel", "foo", "", ""])
    stdout, stderr = daemon.communicate(input="shutdown\n")
    assert_snapshot(
        stdout,
        """
        READY
        EXPECT action
        EXPECT build-backend
        EXPECT hook-name
        EXPECT wheel-directory
        EXPECT config-settings
        EXPECT metadata-directory
        DEBUG ok_backend build_wheel wheel_directory=foo config_settings=None metadata_directory=None
        DEBUG parsed hook inputs in [TIME]
        OK build_wheel_fake_path
        DEBUG ran hook in [TIME]
        READY
        EXPECT action
        SHUTDOWN
        """,
        filters=[TIME, STDOUT, STDERR],
    )
    assert stderr == ""
    assert daemon.returncode == 0


def test_run_build_sdist_ok():
    """
    Uses a mock backend to test the `build_sdist` hook.
    """
    daemon = new()
    send(daemon, ["run", "ok_backend", "build_sdist", "foo", ""])
    stdout, stderr = daemon.communicate(input="shutdown\n")
    assert_snapshot(
        stdout,
        """
        READY
        EXPECT action
        EXPECT build-backend
        EXPECT hook-name
        EXPECT sdist-directory
        EXPECT config-settings
        DEBUG ok_backend build_sdist sdist_directory=foo config_settings=None
        DEBUG parsed hook inputs in [TIME]
        OK build_sdist_fake_path
        DEBUG ran hook in [TIME]
        READY
        EXPECT action
        SHUTDOWN
        """,
        filters=[TIME, STDOUT, STDERR],
    )
    assert stderr == ""
    assert daemon.returncode == 0


def test_run_get_requires_for_build_wheel_ok():
    """
    Uses a mock backend to test the `get_requires_for_build_wheel` hook.
    """
    daemon = new()
    send(daemon, ["run", "ok_backend", "get_requires_for_build_wheel", ""])
    stdout, stderr = daemon.communicate(input="shutdown\n")
    assert_snapshot(
        stdout,
        """
        READY
        EXPECT action
        EXPECT build-backend
        EXPECT hook-name
        EXPECT config-settings
        DEBUG ok_backend get_requires_for_build_wheel config_settings=None
        DEBUG parsed hook inputs in [TIME]
        OK ['fake', 'build', 'wheel', 'requires']
        DEBUG ran hook in [TIME]
        READY
        EXPECT action
        SHUTDOWN
        """,
        filters=[TIME, STDOUT, STDERR],
    )
    assert stderr == ""
    assert daemon.returncode == 0


def test_run_prepare_metadata_for_build_wheel_ok():
    """
    Uses a mock backend to test the `prepare_metadata_for_build_wheel` hook.
    """
    daemon = new()
    send(daemon, ["run", "ok_backend", "prepare_metadata_for_build_wheel", "foo", ""])
    stdout, stderr = daemon.communicate(input="shutdown\n")
    assert_snapshot(
        stdout,
        """
        READY
        EXPECT action
        EXPECT build-backend
        EXPECT hook-name
        EXPECT metadata-directory
        EXPECT config-settings
        DEBUG ok_backend prepare_metadata_for_build_wheel metadata_directory=foo config_settings=None
        DEBUG parsed hook inputs in [TIME]
        OK prepare_metadata_fake_dist_info_path
        DEBUG ran hook in [TIME]
        READY
        EXPECT action
        SHUTDOWN
        """,
        filters=[TIME, STDOUT, STDERR],
    )
    assert stderr == ""
    assert daemon.returncode == 0


def test_run_invalid_config_settings():
    """
    Sends invalid JSON for the `config_settings` argument which should result in a non-fatal error.
    """
    daemon = new()
    send(
        daemon, ["run", "ok_backend", "get_requires_for_build_wheel", "not-valid-json"]
    )
    stdout, stderr = daemon.communicate(input="shutdown\n")
    assert_snapshot(
        stdout,
        """
        READY
        EXPECT action
        EXPECT build-backend
        EXPECT hook-name
        EXPECT config-settings
        ERROR MalformedHookArgument Malformed content for argument 'config_settings': 'not-valid-json'
        READY
        EXPECT action
        SHUTDOWN
        """,
        filters=[TIME, STDOUT, STDERR],
    )
    assert stderr == ""
    assert daemon.returncode == 0


def test_run_build_wheel_multiple_times():
    """
    Uses a mock backend to test running a hook repeatedly.
    """
    daemon = new()
    for _ in range(5):
        send(daemon, ["run", "ok_backend", "build_wheel", "foo", "", ""])
    stdout, stderr = daemon.communicate(input="shutdown\n")
    assert_snapshot(
        stdout,
        """
        READY
        EXPECT action
        EXPECT build-backend
        EXPECT hook-name
        EXPECT wheel-directory
        EXPECT config-settings
        EXPECT metadata-directory
        DEBUG ok_backend build_wheel wheel_directory=foo config_settings=None metadata_directory=None
        DEBUG parsed hook inputs in [TIME]
        OK build_wheel_fake_path
        DEBUG ran hook in [TIME]"""
        * 5
        + """
        READY
        EXPECT action
        SHUTDOWN
        """,
        filters=[TIME, STDOUT, STDERR],
    )
    assert stderr == ""
    assert daemon.returncode == 0


def test_run_build_wheel_error():
    """
    Uses a mock backend that throws an error to test error reporting.
    """
    daemon = new()
    send(daemon, ["run", "err_backend", "build_wheel", "foo", "", ""])
    stdout, stderr = daemon.communicate(input="shutdown\n")
    assert_snapshot(
        stdout,
        """
        READY
        EXPECT action
        EXPECT build-backend
        EXPECT hook-name
        EXPECT wheel-directory
        EXPECT config-settings
        EXPECT metadata-directory
        DEBUG err_backend build_wheel wheel_directory=foo config_settings=None metadata_directory=None
        DEBUG parsed hook inputs in [TIME]
        ERROR HookRuntimeError Oh no
        READY
        EXPECT action
        SHUTDOWN
        """,
        filters=[TIME, STDOUT, STDERR],
    )
    assert stderr == ""
    assert daemon.returncode == 0


def test_run_error_not_fatal():
    """
    Uses a mock backend that throws an error to ensure errors are not fatal and another hook can be run.
    """
    daemon = new()
    send(daemon, ["run", "err_backend", "build_wheel", "foo", "", ""])
    send(daemon, ["run", "err_backend", "build_wheel", "foo", "", ""])
    stdout, stderr = daemon.communicate(input="shutdown\n")
    assert_snapshot(
        stdout,
        """
        READY
        EXPECT action
        EXPECT build-backend
        EXPECT hook-name
        EXPECT wheel-directory
        EXPECT config-settings
        EXPECT metadata-directory
        DEBUG err_backend build_wheel wheel_directory=foo config_settings=None metadata_directory=None
        DEBUG parsed hook inputs in [TIME]
        ERROR HookRuntimeError Oh no
        READY
        EXPECT action
        EXPECT build-backend
        EXPECT hook-name
        EXPECT wheel-directory
        EXPECT config-settings
        EXPECT metadata-directory
        DEBUG err_backend build_wheel wheel_directory=foo config_settings=None metadata_directory=None
        DEBUG parsed hook inputs in [TIME]
        ERROR HookRuntimeError Oh no
        READY
        EXPECT action
        SHUTDOWN
        """,
        filters=[TIME, STDOUT, STDERR],
    )
    assert stderr == ""
    assert daemon.returncode == 0


def test_run_missing_hook():
    """
    Tests a backend without any hooks.
    """
    daemon = new()
    send(daemon, ["run", "empty_backend", "build_wheel", "foo", "", ""])
    stdout, stderr = daemon.communicate(input="shutdown\n")
    assert_snapshot(
        stdout,
        """
        READY
        EXPECT action
        EXPECT build-backend
        EXPECT hook-name
        EXPECT wheel-directory
        EXPECT config-settings
        EXPECT metadata-directory
        DEBUG empty_backend build_wheel wheel_directory=foo config_settings=None metadata_directory=None
        DEBUG parsed hook inputs in [TIME]
        OK build_wheel_fake_path
        DEBUG ran hook in [TIME]
        READY
        EXPECT action
        SHUTDOWN
        """,
        filters=[TIME, STDOUT, STDERR],
    )
    assert stderr == ""
    assert daemon.returncode == 0


@pytest.mark.parametrize("separator", [":", "."])
def test_run_cls_backend(separator):
    """
    Tests a backend namespaced to a class.
    """
    daemon = new()
    send(daemon, ["run", f"cls_backend{separator}Class", "build_wheel", "foo", "", ""])
    stdout, stderr = daemon.communicate(input="shutdown\n")
    assert_snapshot(
        stdout,
        f"""
        READY
        EXPECT action
        EXPECT build-backend
        EXPECT hook-name
        EXPECT wheel-directory
        EXPECT config-settings
        EXPECT metadata-directory
        DEBUG cls_backend{separator}Class build_wheel wheel_directory=foo config_settings=None metadata_directory=None
        DEBUG parsed hook inputs in [TIME]
        OK build_wheel_fake_path
        DEBUG ran hook in [TIME]
        READY
        EXPECT action
        SHUTDOWN
        """,
        filters=[TIME, STDOUT, STDERR],
    )
    assert stderr == ""
    assert daemon.returncode == 0


@pytest.mark.parametrize("separator", [":", "."])
def test_run_obj_backend(separator):
    """
    Tests a backend namespaced to an object.
    """
    daemon = new()
    send(daemon, ["run", f"obj_backend{separator}obj", "build_wheel", "foo", "", ""])
    stdout, stderr = daemon.communicate(input="shutdown\n")
    assert_snapshot(
        stdout,
        f"""
        READY
        EXPECT action
        EXPECT build-backend
        EXPECT hook-name
        EXPECT wheel-directory
        EXPECT config-settings
        EXPECT metadata-directory
        DEBUG obj_backend{separator}obj build_wheel wheel_directory=foo config_settings=None metadata_directory=None
        DEBUG parsed hook inputs in [TIME]
        OK build_wheel_fake_path
        DEBUG ran hook in [TIME]
        READY
        EXPECT action
        SHUTDOWN
        """,
        filters=[TIME, STDOUT, STDERR],
    )
    assert stderr == ""
    assert daemon.returncode == 0


def test_run_submodule_backend():
    """
    Tests a backend namespaced to an submodule.
    """
    daemon = new()
    send(daemon, ["run", "submodule_backend.submodule", "build_wheel", "foo", "", ""])
    stdout, stderr = daemon.communicate(input="shutdown\n")
    assert_snapshot(
        stdout,
        """
        READY
        EXPECT action
        EXPECT build-backend
        EXPECT hook-name
        EXPECT wheel-directory
        EXPECT config-settings
        EXPECT metadata-directory
        DEBUG submodule_backend.submodule build_wheel wheel_directory=foo config_settings=None metadata_directory=None
        DEBUG parsed hook inputs in [TIME]
        OK build_wheel_fake_path
        DEBUG ran hook in [TIME]
        READY
        EXPECT action
        SHUTDOWN
        """,
        filters=[TIME, STDOUT, STDERR],
    )
    assert stderr == ""
    assert daemon.returncode == 0


def test_run_submodule_backend_invalid_import():
    """
    Tests a backend namespaced to an submodule but imported as an attribute
    """
    daemon = new()
    send(daemon, ["run", "submodule_backend:submodule"])
    stdout, stderr = daemon.communicate(input="shutdown\n")
    assert_snapshot(
        stdout,
        """
        READY
        EXPECT action
        EXPECT build-backend
        ERROR MissingBackendAttribute Failed to find attribute 'submodule_backend:submodule' in the backend module 'submodule_backend'
        READY
        EXPECT action
        SHUTDOWN
        """,
        filters=[TIME, STDOUT, STDERR],
    )
    assert stderr == ""
    assert daemon.returncode == 0


def test_run_stdout_capture():
    """
    Tests capture of stdout from a backend.
    """
    daemon = new()
    send(daemon, ["run", "stdout_backend", "build_wheel", "foo", "", ""])
    stdout, stderr = daemon.communicate(input="shutdown\n")
    assert_snapshot(
        stdout,
        """
        READY
        EXPECT action
        EXPECT build-backend
        EXPECT hook-name
        EXPECT wheel-directory
        EXPECT config-settings
        EXPECT metadata-directory
        DEBUG stdout_backend build_wheel wheel_directory=foo config_settings=None metadata_directory=None
        DEBUG parsed hook inputs in [TIME]
        STDOUT [PATH]
        STDERR [PATH]
        OK build_wheel_fake_path
        DEBUG ran hook in [TIME]
        READY
        EXPECT action
        SHUTDOWN
        """,
        filters=[TIME, STDOUT, STDERR],
    )
    stdout_parts = stderr_parts = None
    for line in stdout.splitlines():
        parts = line.split()
        if parts[0] == "STDOUT":
            stdout_parts = parts
        elif parts[0] == "STDERR":
            stderr_parts = parts

    assert len(stdout_parts) == 2
    assert len(stderr_parts) == 2

    assert_snapshot(
        Path(stdout_parts[1]).read_text(),
        """
        hello
        world
        """,
    )
    assert Path(stderr_parts[1]).read_text() == ""

    assert stderr == ""
    assert daemon.returncode == 0


def test_run_stderr_capture():
    """
    Tests capture of stderr from a backend.
    """
    daemon = new()
    send(daemon, ["run", "stderr_backend", "build_wheel", "foo", "", ""])
    stdout, stderr = daemon.communicate(input="shutdown\n")
    assert_snapshot(
        stdout,
        """
        READY
        EXPECT action
        EXPECT build-backend
        EXPECT hook-name
        EXPECT wheel-directory
        EXPECT config-settings
        EXPECT metadata-directory
        DEBUG stderr_backend build_wheel wheel_directory=foo config_settings=None metadata_directory=None
        DEBUG parsed hook inputs in [TIME]
        STDOUT [PATH]
        STDERR [PATH]
        OK build_wheel_fake_path
        DEBUG ran hook in [TIME]
        READY
        EXPECT action
        SHUTDOWN
        """,
        filters=[TIME, STDOUT, STDERR],
    )
    stdout_parts = stderr_parts = None
    for line in stdout.splitlines():
        parts = line.split()
        if parts[0] == "STDOUT":
            stdout_parts = parts
        elif parts[0] == "STDERR":
            stderr_parts = parts

    assert len(stdout_parts) == 2
    assert len(stderr_parts) == 2
    assert Path(stdout_parts[1]).read_text() == ""
    assert_snapshot(
        Path(stderr_parts[1]).read_text(),
        """
        hello
        world
        """,
    )

    assert stderr == ""
    assert daemon.returncode == 0


def test_run_stdout_capture_multiple_hook_runs():
    """
    Tests capture of stdout from a backend, each hook run should get a unique file
    """
    COUNT = 2
    daemon = new()
    for i in range(COUNT):
        send(
            daemon,
            ["run", "stdout_backend", "build_wheel", "foo", f'{{"run": {i}}}', ""],
        )
    stdout, stderr = daemon.communicate(input="shutdown\n")
    print(stdout)

    all_stdout_parts = []
    all_stderr_parts = []
    for line in stdout.splitlines():
        parts = line.split()
        if parts[0] == "STDOUT":
            all_stdout_parts.append(parts)
        elif parts[0] == "STDERR":
            all_stderr_parts.append(parts)

    # We should have a result for each hook run
    assert len(all_stdout_parts) == COUNT
    assert len(all_stderr_parts) == COUNT

    # Each run should write unique output to their file
    for i, stdout_parts in enumerate(all_stdout_parts):
        assert_snapshot(
            Path(stdout_parts[1]).read_text(),
            f"""
            writing config_settings
            run = {i}
            """,
        )
    for i, stderr_parts in enumerate(all_stderr_parts):
        assert Path(stderr_parts[1]).read_text() == ""

    assert stderr == ""
    assert daemon.returncode == 0


@pytest.mark.parametrize("backend", ["hatchling.build", "poetry.core.masonry.api"])
def test_run_real_backend_build_wheel_error(backend: str):
    """
    Sends an path that does not exist to a real "build_wheel" hook.
    """
    try:
        importlib.import_module(backend)
    except ImportError:
        pytest.skip(f"build backend {backend!r} is not installed")

    daemon = new()
    send(daemon, ["run", backend, "build_wheel", "foo", "", ""])
    stdout, stderr = daemon.communicate(input="shutdown\n")
    assert_snapshot(
        stdout,
        f"""
        READY
        EXPECT action
        EXPECT build-backend
        EXPECT hook-name
        EXPECT wheel-directory
        EXPECT config-settings
        EXPECT metadata-directory
        DEBUG {backend} build_wheel wheel_directory=foo config_settings=None metadata_directory=None
        DEBUG parsed hook inputs in [TIME]
        ERROR HookRuntimeError [MESSAGE]
        READY
        EXPECT action
        SHUTDOWN
        """,
        filters=[TIME, ("HookRuntimeError .*", "HookRuntimeError [MESSAGE]")],
    )
    assert stderr == ""
    assert daemon.returncode == 0
