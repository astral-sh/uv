use serde::{Deserialize, Serialize};

/// The [`Manifest`] is a thin wrapper around a unique identifier for the source distribution.
#[derive(Debug, Default, Copy, Clone, Serialize, Deserialize)]
pub(crate) struct Manifest(uuid::Uuid);

impl Manifest {
    /// Return the digest of the manifest. At present, the digest is the first 8 bytes of the
    /// [`uuid::Uuid`] as a string.
    pub(crate) fn digest(&self) -> String {
        self.0.to_string()[..8].to_string()
    }
}
