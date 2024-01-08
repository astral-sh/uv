use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use distribution_filename::WheelFilename;
use platform_tags::Tags;
use pypi_types::Metadata21;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub(crate) struct Manifest {
    /// The metadata for the distribution, as returned by `prepare_metadata_for_build_wheel`.
    metadata: Option<Metadata21>,
    /// The wheels built for the distribution, as returned by `build_wheel`.
    built_wheels: FxHashMap<WheelFilename, DiskFilenameAndMetadata>,
}

impl Manifest {
    /// Set the prepared metadata.
    pub(crate) fn set_metadata(&mut self, metadata: Metadata21) {
        self.metadata = Some(metadata);
    }

    /// Insert a built wheel into the manifest.
    pub(crate) fn insert_wheel(
        &mut self,
        filename: WheelFilename,
        disk_filename_and_metadata: DiskFilenameAndMetadata,
    ) {
        self.built_wheels
            .insert(filename, disk_filename_and_metadata);
    }

    /// Find a compatible wheel in the manifest.
    pub(crate) fn find_wheel(
        &self,
        tags: &Tags,
    ) -> Option<(&WheelFilename, &DiskFilenameAndMetadata)> {
        self.built_wheels
            .iter()
            .find(|(filename, _)| filename.is_compatible(tags))
    }

    /// Find a metadata in the manifest.
    pub(crate) fn find_metadata(&self) -> Option<&Metadata21> {
        // If we already have a prepared metadata, return it.
        if let Some(metadata) = &self.metadata {
            return Some(metadata);
        }

        // Otherwise, return the metadata from any of the built wheels.
        let wheel = self.built_wheels.values().next()?;
        Some(&wheel.metadata)
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
