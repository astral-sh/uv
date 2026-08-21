"""Keep CI's stable required check connected to every blocking job.

GitHub Actions requires a static `needs` list. Check it before planning any work,
so a new job cannot run outside the merge gate just because that list was missed.
Reusable workflow results already include their jobs and matrix expansions.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).parent.parent
AGGREGATE = "required-checks-passed"

# These jobs provide advisory results, not a condition for merging. Keep the
# exceptions explicit: other jobs, including opt-in tests and builds, are required
# whenever their existing conditions select them.
NONBLOCKING_JOBS = {
    "bench": "Performance measurements are reviewed separately from correctness checks.",
    "review": "Automated review findings are advisory and need human review.",
}
NONBLOCKING_OUTPUTS = {"run-bench", "review-security"}

PLAN_OUTPUT = re.compile(r"needs\.plan\.outputs(?:\.([\w-]+)|\[['\"]([\w-]+)['\"]\])")


def load_workflow(path: Path) -> dict:
    # BaseLoader keeps YAML's `on` key as a string instead of a YAML 1.1 boolean.
    return yaml.load(path.read_text(encoding="utf-8"), Loader=yaml.BaseLoader)


def plan_references(value: object) -> set[str]:
    if isinstance(value, str):
        return {name or bracketed for name, bracketed in PLAN_OUTPUT.findall(value)}
    if isinstance(value, dict):
        return {name for item in value.values() for name in plan_references(item)}
    if isinstance(value, list):
        return {name for item in value for name in plan_references(item)}
    return set()


def check_workflows(ci: dict, plan: dict) -> list[str]:
    jobs = ci["jobs"]
    aggregate = jobs[AGGREGATE]
    needs = aggregate["needs"]
    dependencies = {needs} if isinstance(needs, str) else set(needs)
    required = set(jobs) - {AGGREGATE} - NONBLOCKING_JOBS.keys()
    errors = []

    if missing := required - dependencies:
        errors.append(
            f"Required CI jobs missing from {AGGREGATE}.needs: {sorted(missing)}"
        )
    if extra := dependencies - required:
        errors.append(f"Unexpected {AGGREGATE}.needs: {sorted(extra)}")
    if stale := NONBLOCKING_JOBS.keys() - jobs.keys():
        errors.append(f"Unknown nonblocking CI jobs: {sorted(stale)}")

    outputs = set(plan["on"]["workflow_call"]["outputs"])
    job_outputs = set(plan["jobs"]["plan"]["outputs"])
    if outputs != job_outputs:
        errors.append("Plan job outputs and workflow outputs must match")
    if stale := NONBLOCKING_OUTPUTS - outputs:
        errors.append(f"Unknown nonblocking plan outputs: {sorted(stale)}")

    references = set()
    required_references = set()
    for name, job in jobs.items():
        if name == AGGREGATE:
            continue
        consumed = plan_references(job)
        references.update(consumed)
        if name in required:
            required_references.update(consumed)

    if unknown := references - outputs:
        errors.append(f"Unknown plan outputs referenced by CI: {sorted(unknown)}")
    if unused := outputs - references:
        errors.append(f"Plan outputs without a CI consumer: {sorted(unused)}")
    if unprotected := outputs - NONBLOCKING_OUTPUTS - required_references:
        errors.append(
            f"Plan outputs without a required CI consumer: {sorted(unprotected)}"
        )

    return errors


def main() -> int:
    workflows = ROOT / ".github" / "workflows"
    errors = check_workflows(
        load_workflow(workflows / "ci.yml"), load_workflow(workflows / "plan.yml")
    )
    for error in errors:
        print(f"::error::{error}", file=sys.stderr)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
