#!/usr/bin/env python3

import os
import signal
import subprocess
import time
from pathlib import Path
from typing import Any

from antithesis.assertions import always
from helper_install import INTERRUPTED_UNINSTALL_ENVIRONMENT, run


def site_packages(environment: Path) -> Path:
    completed = subprocess.run(
        [
            environment / "bin" / "python",
            "-c",
            "import sysconfig; print(sysconfig.get_path('purelib'))",
        ],
        check=True,
        capture_output=True,
        text=True,
        timeout=30,
    )
    return Path(completed.stdout.strip())


def uninstall_command(environment: Path) -> list[str]:
    return [
        "uv",
        "pip",
        "uninstall",
        "--python",
        environment,
        "antithesis-root",
    ]


def interrupt_uninstall(environment: Path) -> bool:
    packages = site_packages(environment)
    record = packages / "antithesis_root-1.0.0.dist-info" / "RECORD"
    last_payload_file = packages / "antithesis_root" / "generated" / "module_09999.py"

    process = subprocess.Popen(uninstall_command(environment))
    deadline = time.monotonic() + 60
    while process.poll() is None and time.monotonic() < deadline:
        # Freeze the process so the filesystem state cannot change between the checks and kill.
        process.send_signal(signal.SIGSTOP)
        _, status = os.waitpid(process.pid, os.WUNTRACED)
        if not os.WIFSTOPPED(status):
            process.returncode = os.waitstatus_to_exitcode(status)
            break
        if not record.exists() and last_payload_file.exists():
            process.kill()
            process.wait()
            return True
        process.send_signal(signal.SIGCONT)
        time.sleep(0.0001)

    if process.poll() is None:
        process.kill()
        process.wait()
    return False


def inspect_environment(environment: Path) -> dict[str, Any]:
    packages = site_packages(environment)
    generated = packages / "antithesis_root" / "generated"
    return {
        "generated_files": sum(1 for _ in generated.glob("module_*.py")),
        "dist_info": sorted(
            path.name for path in packages.glob("antithesis_root-*.dist-info")
        ),
    }


def main() -> None:
    interrupted = interrupt_uninstall(INTERRUPTED_UNINSTALL_ENVIRONMENT)
    state_before = inspect_environment(INTERRUPTED_UNINSTALL_ENVIRONMENT)
    always(
        interrupted,
        "The test interrupts an uninstall after RECORD removal",
        {
            "environment": str(INTERRUPTED_UNINSTALL_ENVIRONMENT),
            "state": state_before,
        },
    )
    if not interrupted:
        raise RuntimeError("failed to interrupt uninstall after RECORD removal")

    recovery_result = run(
        uninstall_command(INTERRUPTED_UNINSTALL_ENVIRONMENT),
        timeout=60,
    )
    state_after = inspect_environment(INTERRUPTED_UNINSTALL_ENVIRONMENT)
    recovered = (
        recovery_result.succeeded
        and state_after["generated_files"] == 0
        and not state_after["dist_info"]
    )
    always(
        recovered,
        "Retrying an interrupted uninstall completes package removal",
        {
            "environment": str(INTERRUPTED_UNINSTALL_ENVIRONMENT),
            "state_before": state_before,
            "recovery_command": recovery_result.details(),
            "state_after": state_after,
        },
    )
    if not recovered:
        raise RuntimeError("retrying uv pip uninstall did not complete removal")


if __name__ == "__main__":
    main()
