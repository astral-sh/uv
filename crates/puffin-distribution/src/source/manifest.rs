use serde::{Deserialize, Serialize};

/// The [`Manifest`] is a thin wrapper around a unique identifier for the source distribution.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub(crate) struct Manifest(uuid::Uuid);

impl Manifest {
    /// Initialize a new [`Manifest`] with a random UUID.
    pub(crate) fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }

    /// Return the digest of the manifest. At present, the digest is the first 8 bytes of the
    /// [`uuid::Uuid`] as a string.
    pub(crate) fn digest(&self) -> String {
        self.0.to_string()[..8].to_string()
    }
}
