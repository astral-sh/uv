#!/usr/bin/env python3

import subprocess
import time
from pathlib import Path

from antithesis.assertions import always
from helper_install import (
    SHARED_ENVIRONMENT,
    inspect_payload,
    install_command,
    run,
)


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


def interrupt_reinstall(environment: Path) -> bool:
    packages = site_packages(environment)
    record = packages / "antithesis_root-1.0.0.dist-info" / "RECORD"
    last_payload_file = packages / "antithesis_root" / "generated" / "module_09999.py"

    process = subprocess.Popen(install_command(environment, reinstall=True, copy=True))
    saw_removal = False
    deadline = time.monotonic() + 60
    while process.poll() is None and time.monotonic() < deadline:
        record_exists = record.exists()
        payload_exists = last_payload_file.exists()
        if not record_exists and not payload_exists:
            saw_removal = True
        if saw_removal and record_exists and not payload_exists:
            process.kill()
            process.wait()
            return True
        time.sleep(0.0001)

    if process.poll() is None:
        process.kill()
        process.wait()
    return False


def main() -> None:
    interrupted = interrupt_reinstall(SHARED_ENVIRONMENT)
    always(
        interrupted,
        "The test interrupts a reinstall after metadata becomes visible",
        {"environment": str(SHARED_ENVIRONMENT)},
    )
    if not interrupted:
        raise RuntimeError("failed to interrupt uv during the vulnerable install phase")

    payload_valid_before, payload_before = inspect_payload(SHARED_ENVIRONMENT)
    recovery_result = run(install_command(SHARED_ENVIRONMENT), timeout=60)
    payload_valid_after, payload_after = inspect_payload(SHARED_ENVIRONMENT)
    recovered = recovery_result.succeeded and payload_valid_after
    always(
        recovered,
        "An ordinary install repairs an interrupted reinstall",
        {
            "environment": str(SHARED_ENVIRONMENT),
            "payload_valid_before": payload_valid_before,
            "payload_before": payload_before,
            "recovery_command": recovery_result.details(),
            "payload_after": payload_after,
        },
    )
    if not recovered:
        raise RuntimeError("uv reported success without restoring the wheel payload")


if __name__ == "__main__":
    main()
