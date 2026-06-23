#!/usr/bin/env python3

from antithesis.assertions import sometimes
from helper_install import create_operation, install_and_verify, remove_environment


def main() -> None:
    operation = create_operation("driver")
    result = install_and_verify(operation, timeout=60)
    sometimes(
        not result.succeeded,
        "Install operations are sometimes interrupted by faults",
        {
            "operation": operation.identifier,
            "phase": result.phase,
            **result.command.details(),
        },
    )

    if result.succeeded:
        remove_environment(operation)
    else:
        print(
            "installation was interrupted; preserved "
            f"{operation.environment} and {operation.journal}"
        )


if __name__ == "__main__":
    main()
