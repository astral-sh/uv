use std::fmt::Display;
use std::str;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};

/// Percent-encode Git revisions for use after the `@` in VCS URLs.
///
/// This follows Python's `urllib.parse.quote(rev, safe="/")`.
/// See: <https://docs.python.org/3/library/urllib.parse.html#urllib.parse.quote>
const GIT_REFERENCE_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'/')
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// A reference to commit or commit-ish.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GitReference {
    /// A specific branch.
    Branch(String),
    /// A specific tag.
    Tag(String),
    /// From a reference that's ambiguously a branch or tag.
    BranchOrTag(String),
    /// From a reference that's ambiguously a commit, branch, or tag.
    BranchOrTagOrCommit(String),
    /// From a named reference, like `refs/pull/493/head`.
    NamedRef(String),
    /// The default branch of the repository, the reference named `HEAD`.
    DefaultBranch,
}

impl GitReference {
    /// Creates a [`GitReference`] from an arbitrary revision string, which could represent a
    /// branch, tag, commit, or named ref.
    pub fn from_rev(rev: String) -> Self {
        if rev.starts_with("refs/") {
            Self::NamedRef(rev)
        } else if looks_like_commit_hash(&rev) {
            Self::BranchOrTagOrCommit(rev)
        } else {
            Self::BranchOrTag(rev)
        }
    }

    /// Converts the [`GitReference`] to a `str`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Tag(rev) => Some(rev),
            Self::Branch(rev) => Some(rev),
            Self::BranchOrTag(rev) => Some(rev),
            Self::BranchOrTagOrCommit(rev) => Some(rev),
            Self::NamedRef(rev) => Some(rev),
            Self::DefaultBranch => None,
        }
    }

    /// Converts the [`GitReference`] to a `str` that can be used as a revision.
    pub fn as_rev(&self) -> &str {
        match self {
            Self::Tag(rev) => rev,
            Self::Branch(rev) => rev,
            Self::BranchOrTag(rev) => rev,
            Self::BranchOrTagOrCommit(rev) => rev,
            Self::NamedRef(rev) => rev,
            Self::DefaultBranch => "HEAD",
        }
    }

    /// Converts the [`GitReference`] to a percent-encoded revision string for use in a URL.
    pub fn as_url_rev(&self) -> Option<String> {
        self.as_str().map(Self::encode_rev)
    }

    /// Percent-encode a revision string for use in a URL.
    pub fn encode_rev(rev: &str) -> String {
        utf8_percent_encode(rev, GIT_REFERENCE_ENCODE_SET).to_string()
    }

    /// Returns the kind of this reference.
    pub fn kind_str(&self) -> &str {
        match self {
            Self::Branch(_) => "branch",
            Self::Tag(_) => "tag",
            Self::BranchOrTag(_) => "branch or tag",
            Self::BranchOrTagOrCommit(_) => "branch, tag, or commit",
            Self::NamedRef(_) => "ref",
            Self::DefaultBranch => "default branch",
        }
    }
}

impl Display for GitReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str().unwrap_or("HEAD"))
    }
}

/// Whether a `rev` looks like a commit hash (ASCII hex digits).
fn looks_like_commit_hash(rev: &str) -> bool {
    rev.len() >= 7 && rev.chars().all(|ch| ch.is_ascii_hexdigit())
}
