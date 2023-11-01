//! Home of the [`GitSource`].
//!
//! Apparently, the most important type in this module is [`GitSource`].
//! [`git`] provides libgit2 utilities like fetch and checkout, whereas
//! [`oxide`] is the counterpart for gitoxide integration. [`known_hosts`]
//! is the mitigation of [CVE-2022-46176].
//!
//! [CVE-2022-46176]: https://blog.rust-lang.org/2023/01/10/cve-2022-46176.html

use std::str::FromStr;

use anyhow::Context;
use url::Url;

pub use self::git::{fetch, GitCheckout, GitDatabase, GitRemote};

mod config;
mod git;
mod source;
mod util;
mod utils;

#[derive(Debug, Clone)]
pub struct GitDependencyReference {
    url: Url,
    reference: GitReference,
    precise: Option<git2::Oid>,
}

impl FromStr for GitDependencyReference {
    type Err = anyhow::Error;

    fn from_str(string: &str) -> Result<Self, Self::Err> {
        let (kind, url) = string
            .split_once('+')
            .ok_or_else(|| anyhow::format_err!("Invalid source: `{string}`"))?;

        match kind {
            "git" => {
                let mut url = Url::parse(url)
                    .with_context(|| format!("Failed to parse Git source: {url}"))?;
                let mut reference = GitReference::DefaultBranch;
                for (k, v) in url.query_pairs() {
                    match &k[..] {
                        // Map older 'ref' to branch.
                        "branch" | "ref" => reference = GitReference::Branch(v.into_owned()),

                        "rev" => reference = GitReference::Rev(v.into_owned()),
                        "tag" => reference = GitReference::Tag(v.into_owned()),
                        _ => {}
                    }
                }
                let precise = url.fragment().map(|s| git2::Oid::from_str(s)).transpose()?;
                url.set_fragment(None);
                url.set_query(None);

                Ok(Self {
                    url,
                    reference,
                    precise,
                })
            }
            kind => Err(anyhow::format_err!("unsupported source protocol: {}", kind)),
        }
    }
}

/// Information to find a specific commit in a Git repository.
#[derive(Debug, Clone)]
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

#[cfg(test)]
mod test {
    use std::path::Path;
    use std::str::FromStr;

    use crate::config::Config;
    use crate::util::CargoResult;
    use crate::{GitDependencyReference, GitReference, GitRemote};

    #[test]
    fn run() -> CargoResult<()> {
        let source = GitDependencyReference::from_str("ssh://git@github.com/pallets/flask.git")?;

        let remote = GitRemote::new(&source.url);

        let db_path = Path::new("db");

        // let db = remote.db_at(&db_path)?;

        let checkout_path = Path::new("checkout");
        remote.checkout(
            checkout_path,
            None,
            &GitReference::Branch("main".to_string()),
            source.precise,
            &Config::new(),
        )?;

        Ok(())
    }
}
