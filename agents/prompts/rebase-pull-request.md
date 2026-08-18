The checked-out pull request's rebase onto `$BASE_SHA` is already in progress and has conflicts.

If `$BASE_PREVIOUS_SHA` is set, the workflow started
`git rebase --onto "$BASE_SHA" "$BASE_PREVIOUS_SHA"` to replay only the stacked pull request's
commits. Otherwise it started `git rebase "$BASE_SHA"`. Continue that rebase; do not restart it or
widen its commit range.

In any GitHub-facing output, write issue and pull request references in the canonical
owner/repository#number form, such as astral-sh/uv#123 or astral-sh/uv-dev#123. This preserves
cross-repository closing keywords and lets GitHub render the references as links. Do not use bare
numbers, repository-name shorthand, Markdown link syntax, or backticks around references.

- Inspect the conflicted files and preserve the intent of both the pull request and its updated
  base.
- Resolve every conflict, stage the resolved files, and run `GIT_EDITOR=true git rebase --continue`.
  Repeat until the rebase completes; later commits may introduce additional conflicts.
- Run the relevant formatting and lint checks, fix any failures introduced by the rebase, and prefer
  focused checks over the full suite.
- Keep the changes narrowly scoped to the pull request. Do not add dependencies, run release builds,
  or make unrelated cleanups.
- Do not abort the rebase or push the branch. The workflow will verify and push the completed
  rebase.
