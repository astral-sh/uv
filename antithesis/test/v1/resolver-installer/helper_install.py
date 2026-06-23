import json
import os
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from antithesis.assertions import always, reachable
from antithesis.lifecycle import send_event

EXPECTED_VERSIONS = {
    "antithesis-left": "1.0.0",
    "antithesis-right": "1.0.0",
    "antithesis-root": "1.0.0",
}
EXPECTED_INSTALL_FILE_COUNT = 10_000
EXPECTED_PAYLOAD_SIZE = 2 * 1024 * 1024
INDEX_URL = os.environ.get(
    "UV_ANTITHESIS_INDEX_URL",
    "http://index:8000/simple",
)
STATE_DIRECTORY = Path(os.environ.get("UV_ANTITHESIS_STATE_DIR", "/state"))
SHARED_ENVIRONMENT = STATE_DIRECTORY / "shared-environment"
INTERRUPTED_UNINSTALL_ENVIRONMENT = (
    STATE_DIRECTORY / "interrupted-uninstall-environment"
)


@dataclass(frozen=True)
class CommandResult:
    returncode: int | None
    timed_out: bool = False

    @property
    def succeeded(self) -> bool:
        return self.returncode == 0

    def details(self) -> dict[str, Any]:
        return {
            "returncode": self.returncode,
            "timed_out": self.timed_out,
        }


@dataclass(frozen=True)
class InstallResult:
    succeeded: bool
    phase: str
    command: CommandResult


@dataclass(frozen=True)
class Operation:
    identifier: str
    environment: Path
    journal: Path

    def update(self, status: str, **details: Any) -> None:
        if self.journal.exists():
            payload = json.loads(self.journal.read_text(encoding="utf-8"))
        else:
            payload = {
                "id": self.identifier,
                "environment": str(self.environment),
                "history": [],
            }

        payload["status"] = status
        payload["history"].append({"status": status, "details": details})

        temporary_journal = self.journal.with_suffix(".tmp")
        temporary_journal.write_text(
            json.dumps(payload, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        temporary_journal.replace(self.journal)


def create_operation(kind: str) -> Operation:
    operations_directory = STATE_DIRECTORY / "operations"
    environments_directory = STATE_DIRECTORY / "environments"
    operations_directory.mkdir(parents=True, exist_ok=True)
    environments_directory.mkdir(parents=True, exist_ok=True)

    operation_directory = Path(
        tempfile.mkdtemp(prefix=f"{kind}-", dir=operations_directory)
    )
    identifier = operation_directory.name
    operation = Operation(
        identifier=identifier,
        environment=environments_directory / identifier,
        journal=operation_directory / "journal.json",
    )
    operation.update(
        "created",
        kind=kind,
        container=os.environ.get("HOSTNAME", "unknown"),
    )
    return operation


def read_operation_journals() -> list[dict[str, Any]]:
    operations_directory = STATE_DIRECTORY / "operations"
    if not operations_directory.exists():
        return []

    journals = []
    for journal in sorted(operations_directory.glob("*/journal.json")):
        try:
            journals.append(json.loads(journal.read_text(encoding="utf-8")))
        except (json.JSONDecodeError, OSError) as error:
            journals.append(
                {
                    "id": journal.parent.name,
                    "status": "invalid",
                    "error": str(error),
                }
            )
    return journals


def remove_environment(operation: Operation) -> None:
    shutil.rmtree(operation.environment, ignore_errors=True)


def run(command: list[str], timeout: int) -> CommandResult:
    try:
        completed = subprocess.run(command, check=False, timeout=timeout)
    except subprocess.TimeoutExpired:
        print(f"command timed out after {timeout}s: {command!r}")
        return CommandResult(returncode=None, timed_out=True)
    return CommandResult(returncode=completed.returncode)


def inspect_versions(environment: Path) -> tuple[bool, dict[str, str] | str]:
    python = environment / "bin" / "python"
    verification = f"""
import json
from importlib.metadata import version

expected = {EXPECTED_VERSIONS!r}
actual = {{name: version(name) for name in expected}}
print(json.dumps(actual, sort_keys=True))
"""
    completed = subprocess.run(
        [python, "-c", verification],
        check=False,
        capture_output=True,
        text=True,
        timeout=30,
    )
    if completed.returncode != 0:
        return False, completed.stderr.strip()

    try:
        actual_versions = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        return False, f"invalid version output: {error}"
    return actual_versions == EXPECTED_VERSIONS, actual_versions


def inspect_payload(environment: Path) -> tuple[bool, dict[str, int] | str]:
    python = environment / "bin" / "python"
    verification = """
import json
from importlib.util import find_spec
from pathlib import Path

spec = find_spec("antithesis_root")
if spec is None or spec.origin is None:
    raise RuntimeError("antithesis_root is not importable")
package = Path(spec.origin).parent
generated = package / "generated"
payload = package / "payload.bin"
print(json.dumps({
    "generated_files": sum(1 for _ in generated.glob("module_*.py")),
    "payload_size": payload.stat().st_size if payload.exists() else 0,
}, sort_keys=True))
"""
    try:
        completed = subprocess.run(
            [python, "-c", verification],
            check=False,
            capture_output=True,
            text=True,
            timeout=30,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        return False, str(error)
    if completed.returncode != 0:
        return False, completed.stderr.strip()

    try:
        payload = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        return False, f"invalid payload output: {error}"
    return (
        payload
        == {
            "generated_files": EXPECTED_INSTALL_FILE_COUNT,
            "payload_size": EXPECTED_PAYLOAD_SIZE,
        },
        payload,
    )


def install_command(
    environment: Path,
    *,
    offline: bool = False,
    reinstall: bool = False,
    copy: bool = False,
) -> list[str]:
    command = [
        "uv",
        "pip",
        "install",
        "--python",
        environment,
        "--default-index",
        INDEX_URL,
    ]
    if offline:
        command.append("--offline")
    if reinstall:
        command.append("--reinstall")
    if copy:
        command.extend(["--link-mode", "copy"])
    command.extend(["--no-build", "antithesis-root"])
    return command


def verify_environment(operation: Operation, offline: bool) -> None:
    versions_valid, versions = inspect_versions(operation.environment)
    assertion_details = {
        "operation": operation.identifier,
        "offline": offline,
        "versions": versions,
    }
    always(
        versions_valid,
        "Successful installations contain exactly the expected package versions",
        assertion_details,
    )

    payload_valid, payload = inspect_payload(operation.environment)
    always(
        payload_valid,
        "Successful installations contain the complete wheel payload",
        {
            **assertion_details,
            "payload": payload,
        },
    )

    python = operation.environment / "bin" / "python"
    dependency_check = run(["uv", "pip", "check", "--python", python], timeout=30)
    always(
        dependency_check.succeeded,
        "Successful installations pass uv pip check",
        {
            **assertion_details,
            **dependency_check.details(),
        },
    )

    operation.update(
        "verified",
        offline=offline,
        versions=versions,
        dependencies_valid=dependency_check.succeeded,
    )
    if not versions_valid or not payload_valid or not dependency_check.succeeded:
        operation.update("invalid", offline=offline)
        raise RuntimeError("uv reported success but produced an invalid environment")

    reachable(
        "A resolver and installer operation completes",
        assertion_details,
    )
    send_event("uv_install_completed", assertion_details)


def install_and_verify(
    operation: Operation,
    *,
    offline: bool = False,
    timeout: int,
) -> InstallResult:
    operation.update("creating_environment", offline=offline)
    environment_result = run(
        ["uv", "venv", "--python", sys.executable, operation.environment],
        timeout=timeout,
    )
    if not environment_result.succeeded:
        operation.update(
            "interrupted",
            phase="venv",
            offline=offline,
            command=environment_result.details(),
        )
        return InstallResult(False, "venv", environment_result)

    command = install_command(operation.environment, offline=offline)

    operation.update("installing", offline=offline)
    install_result = run(command, timeout=timeout)
    if not install_result.succeeded:
        operation.update(
            "interrupted",
            phase="install",
            offline=offline,
            command=install_result.details(),
        )
        send_event(
            "uv_install_interrupted",
            {
                "operation": operation.identifier,
                "offline": offline,
                **install_result.details(),
            },
        )
        return InstallResult(False, "install", install_result)

    operation.update("verifying", offline=offline)
    verify_environment(operation, offline)
    operation.update("completed", offline=offline)
    return InstallResult(True, "completed", install_result)
