use std::path::{Path, PathBuf};
use std::process::Command;

/// Information about the git repository where uv was built from.
#[derive(Default, Debug, Clone, Hash, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct CommitInfo {
    short_commit_hash: String,
    commit_hash: String,
    commit_date: String,
    last_tag: Option<String>,
    commits_since_last_tag: u32,
}

impl CommitInfo {
    /// Return the [`CommitInfo`] for the repository at the given path.
    pub fn from_repository(path: &Path) -> Option<Self> {
        // Find the `.git` directory, searching through parent directories if necessary.
        let git_dir = path
            .ancestors()
            .map(|ancestor| ancestor.join(".git"))
            .find(|git_dir| git_dir.exists())?;

        if let Some(git_head_path) = git_head(&git_dir) {
            println!("cargo:rerun-if-changed={}", git_head_path.display());

            let git_head_contents = fs_err::read_to_string(git_head_path);
            if let Ok(git_head_contents) = git_head_contents {
                // The contents are either a commit or a reference in the following formats
                // - "<commit>" when the head is detached
                // - "ref <ref>" when working on a branch
                // If a commit, checking if the HEAD file has changed is sufficient
                // If a ref, we need to add the head file for that ref to rebuild on commit
                let mut git_ref_parts = git_head_contents.split_whitespace();
                git_ref_parts.next();
                if let Some(git_ref) = git_ref_parts.next() {
                    let git_ref_path = git_dir.join(git_ref);
                    println!("cargo:rerun-if-changed={}", git_ref_path.display());
                }
            }
        }

        let output = match Command::new("git")
            .arg("log")
            .arg("-1")
            .arg("--date=short")
            .arg("--abbrev=9")
            .arg("--format=%H %h %cd %(describe)")
            .output()
        {
            Ok(output) if output.status.success() => output,
            _ => return None,
        };
        let stdout = String::from_utf8(output.stdout).unwrap();
        let mut parts = stdout.split_whitespace();
        let mut next = || parts.next().unwrap();

        let commit_hash = next().to_string();
        let short_commit_hash = next().to_string();
        let commit_date = next().to_string();

        // Describe can fail for some commits
        // https://git-scm.com/docs/pretty-formats#Documentation/pretty-formats.txt-emdescribeoptionsem
        let mut last_tag = None;
        let mut commits_since_last_tag = 0;
        if let Some(describe) = parts.next() {
            let mut describe_parts = describe.split('-');
            last_tag = Some(describe_parts.next().unwrap().to_string());
            commits_since_last_tag = describe_parts.next().unwrap_or("0").parse().unwrap_or(0);
        }

        Some(CommitInfo {
            short_commit_hash,
            commit_hash,
            commit_date,
            last_tag: None,
            commits_since_last_tag: 0,
        })
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
