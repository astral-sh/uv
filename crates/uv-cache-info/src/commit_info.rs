use std::path::{Path, PathBuf};

/// The current commit for a repository.
#[derive(Default, Debug, Clone, Hash, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct Commit(String);

impl Commit {
    /// Return the [`Commit`] for the repository at the given path.
    pub(crate) fn from_repository(path: &Path) -> Option<Self> {
        // Find the `.git` directory, searching through parent directories if necessary.
        let git_dir = path
            .ancestors()
            .map(|ancestor| ancestor.join(".git"))
            .find(|git_dir| git_dir.exists())?;

        let git_head_path = git_head(&git_dir)?;
        let git_head_contents = fs_err::read_to_string(git_head_path).ok()?;

        // The contents are either a commit or a reference in the following formats
        // - "<commit>" when the head is detached
        // - "ref <ref>" when working on a branch
        // If a commit, checking if the HEAD file has changed is sufficient
        // If a ref, we need to add the head file for that ref to rebuild on commit
        let mut git_ref_parts = git_head_contents.split_whitespace();
        let commit_or_ref = git_ref_parts.next()?;
        if let Some(git_ref) = git_ref_parts.next() {
            let git_ref_path = git_dir.join(git_ref);
            let commit = fs_err::read_to_string(git_ref_path).ok()?;
            Some(Self(commit))
        } else {
            Some(Self(commit_or_ref.to_string()))
        }
    }
}

/// Return the path to the `HEAD` file of a Git repository, taking worktrees into account.
fn git_head(git_dir: &Path) -> Option<PathBuf> {
    // The typical case is a standard git repository.
    let git_head_path = git_dir.join("HEAD");
    if git_head_path.exists() {
        return Some(git_head_path);
    }
    if !git_dir.is_file() {
        return None;
    }
    // If `.git/HEAD` doesn't exist and `.git` is actually a file,
    // then let's try to attempt to read it as a worktree. If it's
    // a worktree, then its contents will look like this, e.g.:
    //
    //     gitdir: /home/andrew/astral/uv/main/.git/worktrees/pr2
    //
    // And the HEAD file we want to watch will be at:
    //
    //     /home/andrew/astral/uv/main/.git/worktrees/pr2/HEAD
    let contents = fs_err::read_to_string(git_dir).ok()?;
    let (label, worktree_path) = contents.split_once(':')?;
    if label != "gitdir" {
        return None;
    }
    let worktree_path = worktree_path.trim();
    Some(PathBuf::from(worktree_path))
}
