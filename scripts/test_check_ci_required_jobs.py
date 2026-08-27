"""Regression tests for the required CI job contract and aggregate result check."""

from __future__ import annotations

import copy
import json
import os
import subprocess
import unittest

from check_ci_required_jobs import AGGREGATE, ROOT, check_workflows, load_workflow


class RequiredCIJobs(unittest.TestCase):
    def setUp(self) -> None:
        workflows = ROOT / ".github" / "workflows"
        self.ci = load_workflow(workflows / "ci.yml")
        self.plan = load_workflow(workflows / "plan.yml")

    def add_output(self, name: str) -> None:
        self.plan["on"]["workflow_call"]["outputs"][name] = {
            "value": f"${{{{ jobs.plan.outputs.{name} }}}}"
        }
        self.plan["jobs"]["plan"]["outputs"][name] = (
            f"${{{{ steps.plan.outputs.{name} }}}}"
        )

    def test_current_workflows(self) -> None:
        self.assertEqual(check_workflows(self.ci, self.plan), [])

    def test_every_required_job_is_protected(self) -> None:
        for name in self.ci["jobs"][AGGREGATE]["needs"]:
            with self.subTest(job=name):
                ci = copy.deepcopy(self.ci)
                ci["jobs"][AGGREGATE]["needs"].remove(name)
                self.assertEqual(
                    check_workflows(ci, self.plan),
                    [f"Required CI jobs missing from {AGGREGATE}.needs: {[name]}"],
                )

    def test_new_planned_job_requires_aggregate_dependency(self) -> None:
        self.add_output("test-new")
        self.ci["jobs"]["test-new"] = {
            "needs": "plan",
            "if": "${{ needs.plan.outputs.test-new == 'true' }}",
            "uses": "./.github/workflows/test-new.yml",
        }
        self.assertEqual(
            check_workflows(self.ci, self.plan),
            [f"Required CI jobs missing from {AGGREGATE}.needs: ['test-new']"],
        )
        self.ci["jobs"][AGGREGATE]["needs"].append("test-new")
        self.assertEqual(check_workflows(self.ci, self.plan), [])

    def test_new_output_requires_consumer(self) -> None:
        self.add_output("test-new")
        self.assertEqual(
            check_workflows(self.ci, self.plan),
            [
                "Plan outputs without a CI consumer: ['test-new']",
                "Plan outputs without a required CI consumer: ['test-new']",
            ],
        )
        self.ci["jobs"]["test"]["with"]["test-new"] = (
            "${{ needs.plan.outputs['test-new'] }}"
        )
        self.assertEqual(check_workflows(self.ci, self.plan), [])

    def test_new_output_is_not_implicitly_nonblocking(self) -> None:
        self.add_output("new-benchmark")
        self.ci["jobs"]["bench"]["with"]["new"] = (
            "${{ needs.plan.outputs.new-benchmark }}"
        )
        self.assertEqual(
            check_workflows(self.ci, self.plan),
            ["Plan outputs without a required CI consumer: ['new-benchmark']"],
        )

    def test_unknown_output(self) -> None:
        self.ci["jobs"]["test"]["with"]["test-new"] = (
            "${{ needs.plan.outputs.test-new }}"
        )
        self.assertEqual(
            check_workflows(self.ci, self.plan),
            ["Unknown plan outputs referenced by CI: ['test-new']"],
        )

    def test_plan_output_wiring(self) -> None:
        del self.plan["jobs"]["plan"]["outputs"]["test-code"]
        self.assertEqual(
            check_workflows(self.ci, self.plan),
            ["Plan job outputs and workflow outputs must match"],
        )

    def test_nonblocking_jobs_stay_outside_aggregate(self) -> None:
        self.ci["jobs"][AGGREGATE]["needs"].append("bench")
        self.assertEqual(
            check_workflows(self.ci, self.plan),
            [f"Unexpected {AGGREGATE}.needs: ['bench']"],
        )

    def run_aggregate(
        self, results: dict[str, dict[str, str]]
    ) -> subprocess.CompletedProcess:
        aggregate = self.ci["jobs"][AGGREGATE]
        return subprocess.run(
            ["bash", "-e", "-o", "pipefail", "-c", aggregate["steps"][0]["run"]],
            env={**os.environ, "NEEDS_JSON": json.dumps(results)},
            capture_output=True,
            text=True,
            check=False,
        )

    def test_aggregate_allows_success_and_conditional_skips(self) -> None:
        results = {
            name: {"result": "success" if index % 2 else "skipped"}
            for index, name in enumerate(self.ci["jobs"][AGGREGATE]["needs"])
        }
        result = self.run_aggregate(results)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_aggregate_rejects_each_required_job_failure(self) -> None:
        for name in self.ci["jobs"][AGGREGATE]["needs"]:
            for conclusion in ("failure", "cancelled"):
                with self.subTest(job=name, conclusion=conclusion):
                    results = {
                        job: {"result": "success"}
                        for job in self.ci["jobs"][AGGREGATE]["needs"]
                    }
                    results[name]["result"] = conclusion
                    result = self.run_aggregate(results)
                    self.assertEqual(
                        result.returncode, 1, result.stdout + result.stderr
                    )
                    self.assertEqual(result.stdout.strip(), f"{name}: {conclusion}")


if __name__ == "__main__":
    unittest.main()
