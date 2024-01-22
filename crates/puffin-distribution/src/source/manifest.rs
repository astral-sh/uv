use serde::{Deserialize, Serialize};

/// The [`Manifest`] exists as an empty serializable struct we can use to test for cache freshness.
///
/// TODO(charlie): Store a unique ID, rather than an empty struct.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub(crate) struct Manifest;
