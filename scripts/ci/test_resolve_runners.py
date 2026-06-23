from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path
from types import ModuleType


def load_resolver() -> ModuleType:
    path = Path(__file__).with_name("resolve-ci-runners.py")
    spec = importlib.util.spec_from_file_location("resolve_ci_runners", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"Failed to load {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


resolver = load_resolver()
ConfigError = resolver.ConfigError
load_config = resolver.load_config
resolve_runners = resolver.resolve_runners
validate_config = resolver.validate_config


class ResolveRunnersTest(unittest.TestCase):
    def test_uses_first_candidate(self) -> None:
        config = validate_config(
            {
                "linux-x86_64": [
                    {"label": "fast-runner", "free": False},
                    {"label": "free-runner", "free": True},
                ]
            }
        )

        self.assertEqual(
            resolve_runners(config, free_only=False),
            {"linux-x86_64": "fast-runner"},
        )

    def test_omits_roles_without_a_free_candidate(self) -> None:
        config = validate_config(
            {
                "available": [
                    {"label": "paid-runner", "free": False},
                    {"label": "free-runner", "free": True},
                ],
                "unavailable": [{"label": "paid-only-runner", "free": False}],
            }
        )

        self.assertEqual(
            resolve_runners(config, free_only=True),
            {"available": "free-runner"},
        )

    def test_validates_repository_config(self) -> None:
        root = Path(__file__).parents[2]
        config = load_config(root / ".github" / "runners.json")

        self.assertNotIn(
            "ubuntu-22-04-aarch64-4",
            resolve_runners(config, free_only=True),
        )

    def test_build_dev_workflow_has_no_hardcoded_private_runners(self) -> None:
        root = Path(__file__).parents[2]
        config = load_config(root / ".github" / "runners.json")
        private_labels = {
            candidate["label"]
            for candidates in config.values()
            for candidate in candidates
            if not candidate["free"]
        }
        workflow = (
            root / ".github" / "workflows" / "build-dev-binaries.yml"
        ).read_text()
        hardcoded = [
            line.strip().removeprefix("runs-on: ")
            for line in workflow.splitlines()
            if line.strip().startswith("runs-on: ")
            and line.strip().removeprefix("runs-on: ") in private_labels
        ]

        self.assertEqual(hardcoded, [])

    def test_rejects_missing_fields(self) -> None:
        with self.assertRaisesRegex(ConfigError, "missing: free"):
            validate_config({"linux": [{"label": "runner"}]})


if __name__ == "__main__":
    unittest.main()
