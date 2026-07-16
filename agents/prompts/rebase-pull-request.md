Resolve the conflicts in the pull request rebase that is already in progress.

- Inspect the conflicted files and preserve the intent of both the pull request and its updated
  base.
- Resolve every conflict, stage the resolved files, and run `GIT_EDITOR=true git rebase --continue`.
  Repeat until the rebase completes; later commits may introduce additional conflicts.
- Keep the changes narrowly scoped to the pull request. Do not add dependencies, run release builds,
  or make unrelated cleanups.
- Do not abort the rebase or push the branch. The workflow will verify and push the completed
  rebase.
