pub use crate::credentials::{store_credentials, store_credentials_from_url};
pub use crate::git::{GIT, GIT_LFS, GitError};
pub use crate::resolver::{
    GitHttpSettings, GitResolver, GitResolverError, RepositoryReference,
    ResolvedRepositoryReference,
};
pub use crate::source::{Fetch, GitSource, Reporter};

mod credentials;
mod git;
mod rate_limit;
mod resolver;
mod source;
