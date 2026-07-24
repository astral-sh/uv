from __future__ import annotations

import subprocess
import unittest
from pathlib import Path

RUN_SCRIPT = Path(__file__).with_name("run.sh")


def run_bash(statement: str, *arguments: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            "bash",
            "-c",
            f'set -euo pipefail\nsource "$1"\nshift\n{statement}',
            "bash",
            str(RUN_SCRIPT),
            *arguments,
        ],
        check=False,
        capture_output=True,
        text=True,
    )


class CodexCgroupTests(unittest.TestCase):
    def test_accepts_generated_dedicated_user(self) -> None:
        result = run_bash('validate_codex_user "$1"', "uvcodex0123456789abcdef")

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_rejects_runner_and_invalid_user_names(self) -> None:
        for username in (
            "runner",
            "root",
            "uvcodex",
            "uvcodex0123456789abcde",
            "uvcodex0123456789abcdef0",
            "uvcodex0123456789abcdeg",
            "uvcodex01234567;runner",
        ):
            with self.subTest(username=username):
                result = run_bash('validate_codex_user "$1"', username)

                self.assertNotEqual(result.returncode, 0)

    def test_accepts_named_permission_profiles(self) -> None:
        for profile in ("create-bug-test", ":workspace", "issue-triage"):
            with self.subTest(profile=profile):
                result = run_bash('validate_permission_profile "$1"', profile)

                self.assertEqual(result.returncode, 0, result.stderr)

    def test_rejects_permission_profile_injection(self) -> None:
        for profile in (
            "",
            "create bug test",
            "create-bug-test\nGH_TOKEN=secret",
            "create-bug-test;runner",
            "../create-bug-test",
        ):
            with self.subTest(profile=profile):
                result = run_bash('validate_permission_profile "$1"', profile)

                self.assertNotEqual(result.returncode, 0)

    def test_accepts_generated_transient_service(self) -> None:
        result = run_bash(
            'validate_service_unit "$1"',
            "uv-codex-123456-2-0123456789abcdef",
        )

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_rejects_service_name_injection(self) -> None:
        for service in (
            "",
            "runner.service",
            "uv-codex-123456-2",
            "uv-codex-123456-2-0123456789abcdeg",
            "uv-codex-123456-2-0123456789abcdef.service",
            "uv-codex-123456-2-0123456789abcdef\nrunner.service",
            "../uv-codex-123456-2-0123456789abcdef",
        ):
            with self.subTest(service=service):
                result = run_bash('validate_service_unit "$1"', service)

                self.assertNotEqual(result.returncode, 0)

    def test_rejects_credentials_and_runner_command_files(self) -> None:
        result = run_bash(
            """
            SERVICE_ENVIRONMENT=()
            for name in "$@"; do
                if append_service_environment "$name" secret; then
                    printf 'Unexpectedly accepted %s\\n' "$name" >&2
                    exit 1
                fi
            done
            """,
            "GH_TOKEN",
            "GITHUB_TOKEN",
            "ACTIONS_ID_TOKEN_REQUEST_TOKEN",
            "ACTIONS_ID_TOKEN_REQUEST_URL",
            "ACTIONS_RUNTIME_TOKEN",
            "GITHUB_OUTPUT",
            "GITHUB_ENV",
            "GITHUB_PATH",
            "LD_PRELOAD",
            "BASH_ENV",
            "HOME",
        )

        self.assertEqual(result.returncode, 0, result.stderr)

    def test_rejects_environment_value_injection(self) -> None:
        for value in ("safe\nGH_TOKEN=secret", "safe\rGH_TOKEN=secret"):
            with self.subTest(value=value):
                result = run_bash(
                    'SERVICE_ENVIRONMENT=(); append_service_environment PATH "$1"',
                    value,
                )

                self.assertNotEqual(result.returncode, 0)

    def test_builds_an_explicit_service_environment(self) -> None:
        result = run_bash(
            """
            unset RUSTUP_HOME UV_PYTHON_INSTALL_DIR INSTA_UPDATE
            SERVICE_ENVIRONMENT=()
            build_service_environment "$1" "$2" "$3"
            printf '%s\\n' "${SERVICE_ENVIRONMENT[@]}"
            """,
            "/usr/bin:/bin",
            "/workspace/agents/codex",
            "/runner-temp/uv-codex-123456-2-0123456789abcdef",
        )

        self.assertEqual(result.returncode, 0, result.stderr)

        arguments = result.stdout.splitlines()
        self.assertIn("--setenv=PATH=/usr/bin:/bin", arguments)
        self.assertIn("--setenv=CODEX_HOME=/workspace/agents/codex", arguments)
        self.assertIn(
            "--setenv=RUNNER_TEMP=/runner-temp/uv-codex-123456-2-0123456789abcdef",
            arguments,
        )
        self.assertFalse(
            any(
                "TOKEN" in argument or "GITHUB_OUTPUT" in argument
                for argument in arguments
            ),
        )

    def test_builds_a_killable_unprivileged_transient_service(self) -> None:
        result = run_bash(
            """
            SERVICE_ENVIRONMENT=()
            build_service_arguments "$1" "$2"
            printf '%s\\n' "${SERVICE_ARGUMENTS[@]}"
            """,
            "uv-codex-123456-2-0123456789abcdef",
            "uvcodex0123456789abcdef",
        )

        self.assertEqual(result.returncode, 0, result.stderr)

        arguments = result.stdout.splitlines()
        self.assertIn("--pipe", arguments)
        self.assertIn("--wait", arguments)
        self.assertIn("--uid=uvcodex0123456789abcdef", arguments)
        self.assertIn("--property=KillMode=control-group", arguments)
        self.assertIn("--property=SendSIGKILL=yes", arguments)
        self.assertIn("--property=NoNewPrivileges=yes", arguments)
        self.assertIn("--property=ProtectControlGroups=yes", arguments)
        self.assertNotIn("--scope", arguments)


if __name__ == "__main__":
    unittest.main()
