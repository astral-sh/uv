pub use crate::credentials::{GIT_STORE, store_credentials_from_url};
pub use crate::git::GIT;
pub use crate::resolver::{
    GitResolver, GitResolverError, RepositoryReference, ResolvedRepositoryReference,
};
pub use crate::source::{Fetch, GitSource, Reporter};

mod credentials;
mod git;
mod resolver;
mod source;
