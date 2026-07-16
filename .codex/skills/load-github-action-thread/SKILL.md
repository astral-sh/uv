---
name: load-github-action-thread
description:
  Download retained Codex GitHub Action thread artifacts and load their rollout history into the
  local Codex app. Use when asked to open, load, import, resume, or inspect a Codex automation
  thread from a GitHub Actions run or a related GitHub issue.
---

# Load GitHub Action Thread

When given an issue instead of a run, find the related issue-triage run first. Resolve the issue
title and number, then match the run display title:

```bash
repository=astral-sh/uv
issue=20477
issue_title="$(gh issue view "$issue" --repo "$repository" --json title --jq '.title')"
issue_number="$(gh issue view "$issue" --repo "$repository" --json number --jq '.number')"

gh run list --repo "$repository" --workflow issue-triage.yml --limit 500 \
  --json databaseId,displayTitle,event,status,conclusion,createdAt,url \
  | jq -c --arg title "$issue_title" '.[] | select(.displayTitle == $title)'
```

Use the newest matching run. An issue-triage run can contain both the triage and bug-reproduction
threads. Also check for directly dispatched bug-reproduction runs; their display title does not
include the issue title. If the issue title changed or a run was manually dispatched, list recent
candidates and confirm them from their logs:

```bash
gh run list --repo "$repository" --workflow reproduce-bug.yml --limit 100 \
  --json databaseId,event,status,conclusion,createdAt,url
gh run view <run-id> --repo "$repository" --log | rg -m 2 "ISSUE: $issue_number|issue: $issue_number"
```

Run the repository utility with the GitHub Actions run URL:

```bash
./agent/scripts/load-github-action-thread.sh https://github.com/astral-sh/uv/actions/runs/123456789
```

The utility downloads every matching `codex-thread-*` artifact, finds its saved rollouts, and forks
each rollout into an interactive local Codex thread. Forking is required because `codex-action` uses
`codex exec`; copying an `exec` rollout into `~/.codex/sessions` alone does not make it visible in
the Codex app.

For a numeric run ID, pass the repository when it cannot be inferred from the current checkout:

```bash
./agent/scripts/load-github-action-thread.sh 123456789 --repo astral-sh/uv
```

Use `--artifact PATTERN` to select a particular retained thread and `--cwd PATH` to attach the
imported history to a different local checkout:

```bash
./agent/scripts/load-github-action-thread.sh 123456789 \
  --repo astral-sh/uv \
  --artifact 'codex-thread-reproduce-bug-*' \
  --cwd /path/to/uv
```

The utility requires `gh`, `jq`, and `codex`. It prints a `codex://threads/<id>` link for every
imported task and automatically opens the Codex app when exactly one task is imported on macOS. For
multiple tasks, return a Markdown list with a clickable link labeled by the corresponding artifact
type without repeatedly switching the app:

```markdown
- [Issue triage](codex://threads/<id>)
- [Bug reproduction](codex://threads/<id>)
```

Use **Pull request security review** for `codex-thread-pull-request-security-review-*` and **Release
preparation** for `codex-thread-release-prepare-*`.
