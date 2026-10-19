use std::path::Path;

use uv_cache_info::CacheInfo;
use uv_distribution_filename::WheelFilename;
use uv_distribution_types::{BuildInfo, BuiltDist, CachedDist, Dist, Hashed};
use uv_metadata::{find_flat_dist_info, read_flat_wheel_metadata, read_wheel_variant};
use uv_preview::PreviewFeature;
use uv_pypi_types::{HashDigest, HashDigests, ResolutionMetadata};

use crate::{Error, supported_variant_properties};

/// Allow variant wheels from an index or local path when the wheel variants preview is enabled.
///
/// Variant compatibility is checked separately by [`check_wheel_variant_compatibility`].
///
/// TODO: Support variant wheels from direct URLs and Git paths.
pub fn check_supported_wheel_variant(dist: &BuiltDist) -> Result<(), Error> {
    let filename = dist.wheel_filename();
    if filename.variant().is_some() {
        let supported_source = match dist {
            BuiltDist::Registry(_) | BuiltDist::Path(_) => true,
            BuiltDist::DirectUrl(_) | BuiltDist::GitPath(_) => false,
        };
        if !supported_source || !uv_preview::is_enabled(PreviewFeature::WheelVariants) {
            return Err(Error::UnsupportedWheelVariant(Box::new(filename.clone())));
        }
    }
    Ok(())
}

/// Check that an extracted wheel's variant properties are supported by the target system.
pub fn check_wheel_variant_compatibility(
    filename: &WheelFilename,
    archive: &Path,
) -> Result<(), Error> {
    let Some(label) = filename.variant() else {
        return Ok(());
    };
    if !uv_preview::is_enabled(PreviewFeature::WheelVariants) {
        return Err(Error::UnsupportedWheelVariant(Box::new(filename.clone())));
    }

    let dist_info = find_flat_dist_info(filename, archive)
        .map_err(|err| Error::WheelMetadata(archive.to_path_buf(), Box::new(err)))?;
    let path = archive.join(format!("{dist_info}.dist-info/variant.json"));
    let metadata = read_wheel_variant(&path, label)
        .map_err(|err| Error::WheelMetadata(path, Box::new(err)))?;
    if let Some(properties) =
        metadata.unsupported_variant_properties(label, &supported_variant_properties(&metadata))
    {
        return Err(Error::IncompatibleWheelVariant {
            label: label.clone(),
            properties,
        });
    }
    Ok(())
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
