//! Lockfile discovery, validation, and resolution for project and workspace workflows.

mod discovery;
mod error;
mod lock;
mod lock_target;
mod lockfile;

pub use discovery::DiscoveredProject;
pub use error::{LockError, MissingLockfileSource};
pub use lock::{LockMode, LockOperation, LockResult};
pub use lock_target::LockTarget;
pub use lockfile::FrozenWorkspace;
