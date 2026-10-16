use std::collections::HashSet;
use std::path::Path;

use tracing::debug;
use uv_cache_info::CacheInfo;
use uv_distribution_filename::{VariantLabel, WheelFilename};
use uv_distribution_types::{BuildInfo, BuiltDist, CachedDist, Dist, Hashed, VariantsJsonContent};
use uv_metadata::{find_flat_dist_info, read_flat_wheel_metadata};
use uv_preview::PreviewFeature;
use uv_pypi_types::{HashDigest, HashDigests, ResolutionMetadata};

use crate::Error;

/// Reject wheel variants unless the preview is enabled and the source supports index-level metadata.
pub fn check_supported_wheel_variant(dist: &Dist) -> Result<(), Error> {
    if let Dist::Built(built) = dist
        && built.wheel_filename().variant().is_some()
        && match built {
            BuiltDist::Registry(_) => !uv_preview::is_enabled(PreviewFeature::WheelVariants),
            BuiltDist::DirectUrl(_) | BuiltDist::Path(_) | BuiltDist::GitPath(_) => true,
        }
    {
        return Err(Error::UnsupportedWheelVariant(Box::new(
            built.wheel_filename().clone(),
        )));
    }
    Ok(())
}

/// Whether an extracted wheel's variant properties are supported by the target system.
pub(crate) fn variant_compatible(filename: &WheelFilename, archive: &Path) -> bool {
    let Some(label) = filename.variant() else {
        return true;
    };
    if !uv_preview::is_enabled(PreviewFeature::WheelVariants) {
        return false;
    }

    let Ok(dist_info) = find_flat_dist_info(filename, archive) else {
        return false;
    };
    let path = archive.join(format!("{dist_info}.dist-info/variant.json"));
    wheel_variant_matches(&path, Some(label))
}

/// Whether `variant.json` describes the expected, compatible variant.
///
/// An ordinary wheel (no variant label) must not have a `variant.json` file.
pub fn wheel_variant_matches(path: &Path, label: Option<&VariantLabel>) -> bool {
    match fs_err::read(path) {
        Ok(contents) => label.is_some_and(|label| {
            VariantsJsonContent::from_slice(&contents)
                .is_ok_and(|metadata| metadata.is_compatible_wheel(label, &HashSet::new()))
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => label.is_none(),
        Err(err) => {
            debug!("Failed to read wheel variant metadata: {err}");
            false
        }
    }
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
