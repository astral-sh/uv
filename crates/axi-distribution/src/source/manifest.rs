use serde::{Deserialize, Serialize};

/// The [`Manifest`] is a thin wrapper around a unique identifier for the source distribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Manifest(String);

impl Manifest {
    /// Initialize a new [`Manifest`] with a random UUID.
    pub(crate) fn new() -> Self {
        Self(nanoid::nanoid!())
    }

    /// Return the unique ID of the manifest.
    pub(crate) fn id(&self) -> &str {
        &self.0
    }
}
