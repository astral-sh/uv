#!/usr/bin/env python3

import sys

from antithesis.assertions import always
from helper_install import (
    SHARED_ENVIRONMENT,
    inspect_payload,
    install_command,
    run,
)


def main() -> None:
    environment_result = run(
        [
            "uv",
            "venv",
            "--clear",
            "--python",
            sys.executable,
            SHARED_ENVIRONMENT,
        ],
        timeout=60,
    )
    install_result = run(install_command(SHARED_ENVIRONMENT), timeout=60)
    payload_valid, payload = inspect_payload(SHARED_ENVIRONMENT)
    initialized = (
        environment_result.succeeded and install_result.succeeded and payload_valid
    )
    always(
        initialized,
        "The interrupted reinstall environment is initialized",
        {
            "environment": str(SHARED_ENVIRONMENT),
            "environment_command": environment_result.details(),
            "install_command": install_result.details(),
            "payload": payload,
        },
    )
    if not initialized:
        raise RuntimeError("failed to initialize the interrupted reinstall environment")


if __name__ == "__main__":
    main()
