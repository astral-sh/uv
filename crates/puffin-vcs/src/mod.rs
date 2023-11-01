//! Home of the [`GitSource`].
//!
//! Apparently, the most important type in this module is [`GitSource`].
//! [`utils`] provides libgit2 utilities like fetch and checkout, whereas
//! [`oxide`] is the counterpart for gitoxide integration. [`known_hosts`]
//! is the mitigation of [CVE-2022-46176].
//!
//! [CVE-2022-46176]: https://blog.rust-lang.org/2023/01/10/cve-2022-46176.html

pub use self::source::GitSource;
pub use self::utils::{fetch, GitCheckout, GitDatabase, GitRemote};
mod known_hosts;
mod oxide;
mod source;
mod utils;

/// For `-Zgitoxide` integration.
pub mod fetch {
    use crate::core::features::GitoxideFeatures;
    use crate::Config;

    /// The kind remote repository to fetch.
    #[derive(Debug, Copy, Clone)]
    pub enum RemoteKind {
        /// A repository belongs to a git dependency.
        GitDependency,
        /// A repository belongs to a Cargo registry.
        Registry,
    }

    impl RemoteKind {
        /// Obtain the kind of history we would want for a fetch from our remote knowing if the target repo is already shallow
        /// via `repo_is_shallow` along with gitoxide-specific feature configuration via `config`.
        /// `rev_and_ref` is additional information that affects whether or not we may be shallow.
        pub(crate) fn to_shallow_setting(
            &self,
            repo_is_shallow: bool,
            config: &Config,
        ) -> gix::remote::fetch::Shallow {
            let has_feature = |cb: &dyn Fn(GitoxideFeatures) -> bool| {
                config
                    .cli_unstable()
                    .gitoxide
                    .map_or(false, |features| cb(features))
            };

            // maintain shallow-ness and keep downloading single commits, or see if we can do shallow clones
            if !repo_is_shallow {
                match self {
                    RemoteKind::GitDependency if has_feature(&|git| git.shallow_deps) => {}
                    RemoteKind::Registry if has_feature(&|git| git.shallow_index) => {}
                    _ => return gix::remote::fetch::Shallow::NoChange,
                }
            };

            gix::remote::fetch::Shallow::DepthAtRemote(1.try_into().expect("non-zero"))
        }
    }

    pub type Error = gix::env::collate::fetch::Error<gix::refspec::parse::Error>;
}
