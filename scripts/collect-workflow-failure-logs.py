# /// script
# requires-python = ">=3.10"
# dependencies = []
#
# [tool.uv]
# no-build = true
# exclude-newer = "P7D"
# ///
"""Index the jobs from a validated Actions run and download only failed job logs."""

import json
import os
import re
import subprocess
import tempfile
from pathlib import Path

FAILED = {"action_required", "failure", "startup_failure", "timed_out"}
EVENT = Path(".workflow-failure-event.json")
LOGS = Path(".workflow-failure-logs")

# Strip ANSI colors, hyperlinks and other terminal controls, keeping tabs and newlines.
CONTROLS = re.compile(
    r"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b\n]*(?:\x07|\x1b\\)|[\x00-\x08\x0b-\x1f\x7f-\x9f]"
)


def download_log(repository: str, job_id: int) -> Path:
    path = LOGS / f"{job_id}.txt"
    with tempfile.TemporaryFile() as raw:
        # The jobs endpoint returns plain text. Never print it to the workflow log.
        result = subprocess.run(
            [
                "gh",
                "api",
                "--allow-escape-sequences",
                f"repos/{repository}/actions/jobs/{job_id}/logs",
            ],
            stdout=raw,
            stderr=subprocess.PIPE,
            check=False,
        )
        if result.returncode:
            # Keep the HTTP status or transport error without logging signed URLs.
            error = CONTROLS.sub("", result.stderr.decode("utf-8", errors="replace"))
            error = re.sub(r'https?://[^\s"]+', "<url>", error, flags=re.IGNORECASE)
            raise RuntimeError(
                f"gh api failed for job {job_id} (exit {result.returncode}): "
                + " ".join(error.split())
            )
        raw.seek(0)
        has_content = False
        with path.open("x", encoding="utf-8") as output:
            for line in raw:
                text = CONTROLS.sub("", line.decode("utf-8", errors="replace"))
                output.write(text)
                has_content = has_content or bool(text.strip())
    if not has_content:
        raise ValueError(f"Empty log for job {job_id}")
    return path


def main() -> None:
    repository = os.environ["GITHUB_REPOSITORY"]
    event = json.loads(EVENT.read_text(encoding="utf-8"))
    run_id, attempt = event["databaseId"], event["attempt"]
    # Pin the listing to the attempt we validated, even if someone retries the run.
    # Large pages of jobs can return HTTP 502 on retries.
    endpoint = (
        f"repos/{repository}/actions/runs/{run_id}/attempts/{attempt}/jobs?per_page=30"
    )
    pages = json.loads(
        subprocess.check_output(["gh", "api", "--paginate", "--slurp", endpoint])
    )
    jobs = [job for page in pages for job in page["jobs"]]
    if (
        not pages
        or any(page["total_count"] != len(jobs) for page in pages)
        or len({job["id"] for job in jobs}) != len(jobs)
    ):
        raise ValueError("Incomplete workflow job listing")

    LOGS.mkdir()
    event["jobs"] = []
    for job in jobs:
        job_id = job["id"]
        if (
            not isinstance(job_id, int)
            or job_id <= 0
            or job["run_id"] != run_id
            or job.get("run_attempt", attempt) != attempt
            or job["head_sha"] != event["headSha"]
        ):
            raise ValueError("Unexpected job in workflow run")
        entry = {
            "databaseId": job_id,
            "name": job["name"],
            "conclusion": job["conclusion"],
            "url": job["html_url"],
        }
        if job["conclusion"] in FAILED:
            entry["steps"] = [
                step for step in job.get("steps", []) if step["conclusion"] in FAILED
            ]
            entry["log"] = str(download_log(repository, job_id))
        elif job["conclusion"] == "cancelled":
            entry["steps"] = job.get("steps", [])
        event["jobs"].append(entry)

    # The classifier must not run with a partial set of failure logs.
    EVENT.write_text(json.dumps(event, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
