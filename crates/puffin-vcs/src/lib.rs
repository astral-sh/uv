//! Home of the [`GitSource`].
//!
//! Apparently, the most important type in this module is [`GitSource`].
//! [`git`] provides libgit2 utilities like fetch and checkout, whereas
//! [`oxide`] is the counterpart for gitoxide integration. [`known_hosts`]
//! is the mitigation of [CVE-2022-46176].
//!
//! [CVE-2022-46176]: https://blog.rust-lang.org/2023/01/10/cve-2022-46176.html

pub use self::git::{fetch, GitCheckout, GitDatabase, GitRemote};

mod known_hosts;
mod git;
mod config;
mod utils;
mod util;

/// Information to find a specific commit in a Git repository.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GitReference {
    /// From a tag.
    Tag(String),
    /// From a branch.
    Branch(String),
    /// From a specific revision. Can be a commit hash (either short or full),
    /// or a named reference like `refs/pull/493/head`.
    Rev(String),
    /// The default branch of the repository, the reference named `HEAD`.
    DefaultBranch,
}
