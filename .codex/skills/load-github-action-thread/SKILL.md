---
name: load-github-action-thread
description:
  Download retained Codex GitHub Action thread artifacts and load their rollout history into the
  local Codex app. Use when asked to open, load, import, resume, or inspect a Codex automation
  thread from a GitHub Actions run URL or run ID.
---

# Load GitHub Action Thread

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
multiple tasks, return a clickable link for each task without repeatedly switching the app:

```markdown
[Open imported task](codex://threads/<id>)
```

Treat downloaded rollouts as sensitive because they can contain prompts, tool inputs, and tool
output; do not paste their contents into a chat or post them to GitHub.
