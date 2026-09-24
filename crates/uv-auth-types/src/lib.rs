//! Shared authentication values and URL matching without HTTP clients.

mod credentials;
mod index;
mod realm;

pub use credentials::{Credentials, CredentialsFromUrlError, Password, Token, Username};
pub use index::{AuthPolicy, Index, Indexes, is_path_prefix};
pub use realm::{Realm, RealmRef};
