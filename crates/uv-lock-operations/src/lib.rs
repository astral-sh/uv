//! Lockfile discovery, validation, and resolution for project and workspace workflows.

mod discovery;
mod error;
mod lock;
mod lock_target;
mod lockfile;
mod validated_lock;

pub use discovery::DiscoveredProject;
pub use error::{LockError, LockValidationError, MissingLockfileSource};
pub use lock::{LockCommand, LockMode, LockOperation, LockResult};
pub use lock_target::LockTarget;
pub use lockfile::FrozenWorkspace;
pub use validated_lock::ValidatedLock;
