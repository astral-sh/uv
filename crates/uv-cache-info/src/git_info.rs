use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use tracing::warn;
use walkdir::WalkDir;

#[derive(Debug, thiserror::Error)]
pub(crate) enum GitInfoError {
    #[error("The repository at {0} is missing a `.git` directory")]
    MissingGitDir(PathBuf),
    #[error("The repository at {0} is missing a `HEAD` file")]
    MissingHead(PathBuf),
    #[error("The repository at {0} is missing the reference `{1}`")]
    MissingRef(PathBuf, String),
    #[error("The repository at {0} has an invalid reference: `{1}`")]
    InvalidRef(PathBuf, String),
    #[error("The discovered commit has an invalid length (expected 40 characters): `{0}`")]
    WrongLength(String),
    #[error("The discovered commit has an invalid character (expected hexadecimal): `{0}`")]
    WrongDigit(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// The current commit for a repository (i.e., a 40-character hexadecimal string).
#[derive(Default, Debug, Clone, Hash, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct Commit(String);

impl Commit {
    /// Return the [`Commit`] for the repository at the given path.
    pub(crate) fn from_repository(path: &Path) -> Result<Self, GitInfoError> {
        let repository = GitRepository::find(path)?;

        let git_head_path = repository.git_dir.join("HEAD");
        if !git_head_path.exists() {
            return Err(GitInfoError::MissingHead(repository.git_dir));
        }
        let git_head_contents = fs_err::read_to_string(git_head_path)?;

        // The contents are either a commit or a reference in the following formats
        // - "<commit>" when the head is detached
        // - "ref: <ref>" when working on a branch
        // If a commit, checking if the HEAD file has changed is sufficient
        // If a ref, we need to add the head file for that ref to rebuild on commit
        let mut git_ref_parts = git_head_contents.split_whitespace();
        let commit_or_ref = git_ref_parts.next().ok_or_else(|| {
            GitInfoError::InvalidRef(repository.git_dir.clone(), git_head_contents.clone())
        })?;
        let commit = if let Some(git_ref) = git_ref_parts.next() {
            repository.read_ref(git_ref)?
        } else {
            commit_or_ref.to_string()
        };

        validate_commit(&commit)?;

        Ok(Self(commit))
    }
}

/// The set of tags visible in a repository.
#[derive(Default, Debug, Clone, Hash, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub(crate) struct Tags(BTreeMap<String, String>);

impl Tags {
    /// Return the [`Tags`] for the repository at the given path.
    pub(crate) fn from_repository(path: &Path) -> Result<Self, GitInfoError> {
        let repository = GitRepository::find(path)?;
        let git_tags_path = repository.common_dir.join("refs").join("tags");

        let mut tags = BTreeMap::new();

        for (git_ref, commit) in read_packed_refs(&repository.common_dir)? {
            if let Some(tag) = git_ref.strip_prefix("refs/tags/") {
                tags.insert(tag.to_string(), commit);
            }
        }

        // Map each tag to its commit.
        if git_tags_path.exists() {
            for entry in WalkDir::new(&git_tags_path).contents_first(true) {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(err) => {
                        warn!("Failed to read Git tags: {err}");
                        continue;
                    }
                };
                let path = entry.path();
                if !entry.file_type().is_file() {
                    continue;
                }
                if let Ok(Some(tag)) = path.strip_prefix(&git_tags_path).map(|name| name.to_str())
                    && let Some(commit) = read_ref_file(path)?
                {
                    tags.insert(tag.to_string(), commit);
                }
            }
        }

        Ok(Self(tags))
    }
}

struct GitRepository {
    git_dir: PathBuf,
    common_dir: PathBuf,
}

impl GitRepository {
    /// Find the Git repository for a path, searching parent directories if necessary.
    fn find(path: &Path) -> Result<Self, GitInfoError> {
        let dot_git_path = path
            .ancestors()
            .map(|ancestor| ancestor.join(".git"))
            .find(|dot_git_path| dot_git_path.exists())
            .ok_or_else(|| GitInfoError::MissingGitDir(path.to_path_buf()))?;
        let git_dir = read_git_dir(&dot_git_path)
            .ok_or_else(|| GitInfoError::MissingGitDir(path.to_path_buf()))?;
        let common_dir = read_common_dir(&git_dir)?;

        Ok(Self {
            git_dir,
            common_dir,
        })
    }

    /// Resolve a Git ref to a commit, taking worktrees and packed refs into account.
    fn read_ref(&self, git_ref: &str) -> Result<String, GitInfoError> {
        if let Some(commit) = read_ref_file(&self.git_dir.join(git_ref))? {
            return Ok(commit);
        }
        if let Some(commit) = read_ref_file(&self.common_dir.join(git_ref))? {
            return Ok(commit);
        }
        if let Some(commit) = read_packed_refs(&self.common_dir)?.remove(git_ref) {
            return Ok(commit);
        }
        Err(GitInfoError::MissingRef(
            self.common_dir.clone(),
            git_ref.to_string(),
        ))
    }
}

/// Resolve `.git` to the repository's Git directory, including linked-worktree files.
fn read_git_dir(dot_git_path: &Path) -> Option<PathBuf> {
    if dot_git_path.is_dir() {
        return Some(dot_git_path.to_path_buf());
    }
    if !dot_git_path.is_file() {
        return None;
    }

    let contents = fs_err::read_to_string(dot_git_path).ok()?;
    let git_dir = contents.strip_prefix("gitdir:")?.trim();
    Some(resolve_relative_path(dot_git_path.parent()?, git_dir))
}

/// Return the common Git directory, following a linked worktree's `commondir` file.
fn read_common_dir(git_dir: &Path) -> Result<PathBuf, GitInfoError> {
    let commondir_path = git_dir.join("commondir");
    let contents = match fs_err::read_to_string(commondir_path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(git_dir.to_path_buf()),
        Err(err) => return Err(err.into()),
    };
    Ok(resolve_relative_path(git_dir, contents.trim()))
}

/// Read and validate a loose ref, returning [`None`] when it does not exist.
fn read_ref_file(path: &Path) -> Result<Option<String>, GitInfoError> {
    let contents = match fs_err::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    let commit = contents.trim().to_string();
    validate_commit(&commit)?;
    Ok(Some(commit))
}

/// Read the direct refs from `packed-refs`, ignoring comments and peeled tag lines.
fn read_packed_refs(git_dir: &Path) -> Result<BTreeMap<String, String>, GitInfoError> {
    let path = git_dir.join("packed-refs");
    let contents = match fs_err::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(err) => return Err(err.into()),
    };

    let mut refs = BTreeMap::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('^') {
            continue;
        }

        let (commit, git_ref) = line
            .split_once(' ')
            .ok_or_else(|| GitInfoError::InvalidRef(git_dir.to_path_buf(), line.to_string()))?;
        validate_commit(commit)?;
        refs.insert(git_ref.to_string(), commit.to_string());
    }

    Ok(refs)
}

fn resolve_relative_path(base: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

fn validate_commit(commit: &str) -> Result<(), GitInfoError> {
    // The commit should be 40 hexadecimal characters.
    if commit.len() != 40 {
        return Err(GitInfoError::WrongLength(commit.to_string()));
    }
    if commit.chars().any(|c| !c.is_ascii_hexdigit()) {
        return Err(GitInfoError::WrongDigit(commit.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use anyhow::Result;

    use super::{Commit, Tags};

    const COMMIT_1: &str = "1b6638fdb424e993d8354e75c55a3e524050c857";
    const COMMIT_2: &str = "a1a42cbd10d83bafd8600ba81f72bbef6c579385";

    #[test]
    fn commit_and_tags_from_linked_worktree() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let worktree = temp_dir.path().join("worktree");
        let common_git_dir = temp_dir.path().join("common.git");
        let worktree_git_dir = common_git_dir.join("worktrees").join("worktree");

        fs_err::create_dir_all(&worktree)?;
        fs_err::create_dir_all(&worktree_git_dir)?;
        fs_err::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", worktree_git_dir.display()),
        )?;
        fs_err::write(worktree_git_dir.join("HEAD"), "ref: refs/heads/main\n")?;
        fs_err::write(worktree_git_dir.join("commondir"), "../..\n")?;
        fs_err::write(
            common_git_dir.join("packed-refs"),
            format!(
                "\
# pack-refs with: peeled fully-peeled sorted
{COMMIT_1} refs/heads/main
{COMMIT_2} refs/tags/v0.1.0
^{COMMIT_1}
"
            ),
        )?;

        let mut expected_tags = BTreeMap::new();
        expected_tags.insert("v0.1.0".to_string(), COMMIT_2.to_string());

        assert_eq!(
            Commit::from_repository(&worktree)?,
            Commit(COMMIT_1.to_string())
        );
        assert_eq!(
            Tags::from_repository(&worktree)?,
            Tags(expected_tags.clone())
        );

        let refs_dir = common_git_dir.join("refs");
        let heads_dir = refs_dir.join("heads");
        let tags_dir = refs_dir.join("tags");
        fs_err::create_dir_all(&heads_dir)?;
        fs_err::create_dir_all(&tags_dir)?;
        fs_err::write(heads_dir.join("main"), COMMIT_2)?;
        fs_err::write(tags_dir.join("v0.1.0"), COMMIT_1)?;

        expected_tags.insert("v0.1.0".to_string(), COMMIT_1.to_string());

        assert_eq!(
            Commit::from_repository(&worktree)?,
            Commit(COMMIT_2.to_string())
        );
        assert_eq!(Tags::from_repository(&worktree)?, Tags(expected_tags));

        Ok(())
    }
}
