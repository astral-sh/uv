use serde::{Deserialize, Serialize};

/// The [`Revision`] is a thin wrapper around a unique identifier for the source distribution.
///
/// A revision represents a unique version of a source distribution, at a level more granular than
/// (e.g.) the version number of the distribution itself. For example, a source distribution hosted
/// at a URL or a local file path may have multiple revisions, each representing a unique state of
/// the distribution, despite the reported version number remaining the same.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Revision(String);

impl Revision {
    /// Initialize a new [`Revision`] with a random UUID.
    pub(crate) fn new() -> Self {
        Self(nanoid::nanoid!())
    }

    /// Return the unique ID of the revision.
    pub(crate) fn id(&self) -> &str {
        &self.0
    }
}
