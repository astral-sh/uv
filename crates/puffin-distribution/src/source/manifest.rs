use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use distribution_filename::WheelFilename;
use platform_tags::Tags;
use pypi_types::Metadata21;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub(crate) struct Manifest(FxHashMap<WheelFilename, DiskFilenameAndMetadata>);

impl Manifest {
    /// Find a compatible wheel in the cache.
    pub(crate) fn find_compatible(
        &self,
        tags: &Tags,
    ) -> Option<(&WheelFilename, &DiskFilenameAndMetadata)> {
        self.0
            .iter()
            .find(|(filename, _metadata)| filename.is_compatible(tags))
    }
}

impl std::ops::Deref for Manifest {
    type Target = FxHashMap<WheelFilename, DiskFilenameAndMetadata>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Manifest {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DiskFilenameAndMetadata {
    /// Relative, un-normalized wheel filename in the cache, which can be different than
    /// `WheelFilename::to_string`.
    pub(crate) disk_filename: String,
    /// The [`Metadata21`] of the wheel.
    pub(crate) metadata: Metadata21,
}
