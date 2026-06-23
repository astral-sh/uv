#!/usr/bin/env python3

import time

from antithesis.assertions import always, always_or_unreachable, sometimes
from antithesis.lifecycle import send_event
from helper_install import (
    create_operation,
    install_and_verify,
    read_operation_journals,
    remove_environment,
)


def main() -> None:
    journals = read_operation_journals()
    invalid_journals = [
        journal for journal in journals if journal["status"] == "invalid"
    ]
    completed_journals = [
        journal for journal in journals if journal["status"] == "completed"
    ]
    incomplete_journals = [
        journal
        for journal in journals
        if journal["status"] not in {"completed", "invalid"}
    ]

    always(
        not invalid_journals,
        "Operation journals remain valid after faults",
        {"invalid_journals": invalid_journals},
    )
    sometimes(
        bool(incomplete_journals),
        "Persisted journals expose interrupted operations",
        {"incomplete_operations": len(incomplete_journals)},
    )
    send_event(
        "uv_recovery_started",
        {
            "completed_operations": len(completed_journals),
            "incomplete_operations": len(incomplete_journals),
        },
    )

    if completed_journals:
        offline_operation = create_operation("offline-before-recovery")
        offline_result = install_and_verify(
            offline_operation,
            offline=True,
            timeout=60,
        )
        always_or_unreachable(
            offline_result.succeeded,
            "A previously successful cache remains usable offline after faults",
            {
                "operation": offline_operation.identifier,
                "completed_operations": len(completed_journals),
                "incomplete_operations": len(incomplete_journals),
                "phase": offline_result.phase,
            },
        )
        if not offline_result.succeeded:
            raise RuntimeError("the pre-fault cache is not usable offline")
        remove_environment(offline_operation)

    deadline = time.monotonic() + 120
    attempts = 0
    recovery_operation = None
    recovery_result = None

    while time.monotonic() < deadline:
        attempts += 1
        recovery_operation = create_operation("online-recovery")
        recovery_result = install_and_verify(recovery_operation, timeout=60)
        if recovery_result.succeeded:
            remove_environment(recovery_operation)
            break
        time.sleep(1)

    recovered = recovery_result is not None and recovery_result.succeeded
    always(
        recovered,
        "uv recovers after fault injection stops",
        {
            "attempts": attempts,
            "operation": (
                recovery_operation.identifier
                if recovery_operation is not None
                else None
            ),
            "phase": recovery_result.phase if recovery_result is not None else None,
        },
    )
    if not recovered:
        raise RuntimeError("uv did not recover after fault injection stopped")

    offline_operation = create_operation("offline-after-recovery")
    offline_result = install_and_verify(
        offline_operation,
        offline=True,
        timeout=60,
    )
    always(
        offline_result.succeeded,
        "A recovered cache supports a fresh offline installation",
        {
            "operation": offline_operation.identifier,
            "phase": offline_result.phase,
        },
    )
    if not offline_result.succeeded:
        raise RuntimeError("uv recovered online but its cache is not usable offline")
    remove_environment(offline_operation)

    print(
        "verified journal integrity, online recovery, and offline installation "
        f"after {attempts} online attempt(s)"
    )


if __name__ == "__main__":
    main()
