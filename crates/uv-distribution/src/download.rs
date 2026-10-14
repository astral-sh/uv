use std::collections::HashSet;
use std::path::Path;

use uv_cache_info::CacheInfo;
use uv_distribution_filename::WheelFilename;
use uv_distribution_types::{BuildInfo, CachedDist, Dist, Hashed, VariantsJsonContent};
use uv_metadata::read_flat_wheel_metadata;
use uv_preview::PreviewFeature;
use uv_pypi_types::{HashDigest, HashDigests, ResolutionMetadata};

use crate::Error;

/// Whether an extracted wheel's variant properties are supported by the target system.
pub(crate) fn variant_compatible(filename: &WheelFilename, archive: &Path) -> bool {
    let Some(label) = filename.variant() else {
        return true;
    };
    if !uv_preview::is_enabled(PreviewFeature::WheelVariants) {
        return false;
    }

    let Ok(dist_info) = uv_metadata::find_flat_dist_info(filename, archive) else {
        return false;
    };
    let path = archive.join(format!("{dist_info}.dist-info/variant.json"));
    let Ok(contents) = fs_err::read(path) else {
        return false;
    };
    let Ok(metadata) = VariantsJsonContent::from_slice(&contents) else {
        return false;
    };
    metadata.is_compatible(label, &HashSet::new())
}

/// A locally available wheel.
#[derive(Debug, Clone)]
pub struct LocalWheel {
    /// The remote distribution from which this wheel was downloaded.
    pub(crate) dist: Dist,
    /// The parsed filename.
    pub(crate) filename: WheelFilename,
    /// The canonicalized path in the cache directory to which the wheel was downloaded.
    /// Typically, a directory within the archive bucket.
    pub(crate) archive: Box<Path>,
    /// The cache info of the wheel.
    pub(crate) cache: CacheInfo,
    /// The build info, if available.
    pub(crate) build: Option<BuildInfo>,
    /// The computed hashes of the wheel.
    pub(crate) hashes: HashDigests,
}

impl LocalWheel {
    /// Check whether the wheel's variant properties are supported by the target system.
    pub fn check_variant_compatibility(&self) -> Result<(), Error> {
        if variant_compatible(&self.filename, &self.archive) {
            Ok(())
        } else {
            Err(Error::IncompatibleWheelVariant(Box::new(
                self.filename.clone(),
            )))
        }
    }

    /// Return the [`Dist`] from which this wheel was downloaded.
    fn remote(&self) -> &Dist {
        &self.dist
    }

    /// Read the [`ResolutionMetadata`] from a wheel.
    pub(crate) fn metadata(&self) -> Result<ResolutionMetadata, Error> {
        read_flat_wheel_metadata(&self.filename, &self.archive)
            .map_err(|err| Error::WheelMetadata(self.archive.to_path_buf(), Box::new(err)))
    }
}

impl Hashed for LocalWheel {
    fn hashes(&self) -> &[HashDigest] {
        self.hashes.as_slice()
    }
}

/// Convert a [`LocalWheel`] into a [`CachedDist`].
impl From<LocalWheel> for CachedDist {
    fn from(wheel: LocalWheel) -> Self {
        Self::from_remote(
            wheel.dist,
            wheel.filename,
            wheel.hashes,
            wheel.cache,
            wheel.build,
            wheel.archive,
        )
    }
}

impl std::fmt::Display for LocalWheel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.remote())
    }
}
