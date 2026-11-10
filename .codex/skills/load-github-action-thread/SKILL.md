---
name: load-github-action-thread
description:
  Download retained Codex GitHub Action thread artifacts and load their rollout history into the
  local Codex app. Use when asked to open, load, import, resume, or inspect a Codex automation
  thread from a GitHub Actions run or a related GitHub issue or pull request.
---

# Load GitHub Action Thread

When given an issue instead of a run, find the related issue-triage run first. Resolve the issue
number and search for it in the run name.

```bash
repository=astral-sh/uv
issue=20477
issue_number="$(gh issue view "$issue" --repo "$repository" --json number --jq '.number')"

gh run list --repo "$repository" --workflow issue-triage.yml --limit 500 \
  --json databaseId,displayTitle,event,status,conclusion,createdAt,url \
  | jaq -c --arg number "$issue_number" --arg url "https://github.com/$repository/issues/$issue_number" \
      '.[] | select(
        .displayTitle == "Issue context for \($number)" or
        .displayTitle == "Issue context for \($url)"
      )'
```

Confirm the issue number in the newest matching run's logs before importing the thread. An
issue-triage run can contain both the triage and bug-reproduction threads. For older triage runs,
use the run URL or ID. For manual dispatches with a different URL spelling, check recent triage
runs. Also check for directly dispatched bug-reproduction runs.

```bash
gh run list --repo "$repository" --workflow reproduce-bug.yml --limit 100 \
  --json databaseId,event,status,conclusion,createdAt,url
gh run view <run-id> --repo "$repository" --log | rg -m 2 "ISSUE_NUMBER: ${issue_number}([[:space:]]|$)"
```

When given a pull request, find its security-review run from the pull request checks. The check link
contains the Actions run URL and can be passed directly to the loader:

```bash
pull_request=20482
gh pr checks "$pull_request" --repo "$repository" --json name,workflow,state,link \
  --jq '.[] | select(.name | test("review"; "i"))'
```

The security-review job only runs for eligible same-repository pull requests; a skipped review has
no thread artifact to load.

Run the repository utility with the GitHub Actions run URL:

```bash
./agents/scripts/load-github-action-thread.sh https://github.com/astral-sh/uv/actions/runs/123456789
```

The utility downloads every matching `codex-thread-*` artifact, finds the root `codex exec` rollout
for each step, and forks it into an interactive local Codex thread. Subagent rollouts are ignored;
they inherit the root prompt and would otherwise produce duplicate-looking tasks. The first line of
the root preview becomes the task title, including the step, source number, and issue or pull
request title. Forking is required because `codex-action` uses `codex exec`; copying an `exec`
rollout into `~/.codex/sessions` alone does not make it visible in the Codex app.

For a numeric run ID, pass the repository when it cannot be inferred from the current checkout:

```bash
./agents/scripts/load-github-action-thread.sh 123456789 --repo astral-sh/uv
```

Use `--artifact PATTERN` to select a particular retained thread and `--cwd PATH` to attach the
imported history to a different local checkout:

```bash
./agents/scripts/load-github-action-thread.sh 123456789 \
  --repo astral-sh/uv \
  --artifact 'codex-thread-reproduce-bug-*' \
  --cwd /path/to/uv
```

The utility requires `gh`, `jaq`, and `codex`, plus `zstd` for compressed rollouts. It prints the
resolved title and a `codex://threads/<id>` link for every imported task and automatically opens the
Codex app when exactly one task is imported on macOS. For multiple steps, return a Markdown list
with a clickable link labeled by the corresponding task title without repeatedly switching the app:

```markdown
- [Issue triage for #20477: Example issue](codex://threads/<id>)
- [Bug reproduction for #20477: Example issue](codex://threads/<id>)
```

Use the matching step title for `codex-thread-pull-request-security-review-*` and
`codex-thread-release-prepare-*`.
