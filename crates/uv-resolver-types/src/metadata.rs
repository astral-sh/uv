use reqwest::StatusCode;
use rustc_hash::FxHasher;
use std::hash::BuildHasherDefault;
use std::sync::Arc;
use uv_distribution::ArchiveMetadata;
use uv_distribution_types::{DistributionId, RequestedDist};
use uv_once_map::RegisteredOnceMap;
use uv_pep440::{Version, VersionSpecifiers};

/// Metadata shared between resolution and lockfile validation.
pub type DistributionMetadataIndex =
    RegisteredOnceMap<DistributionId, Arc<MetadataResponse>, BuildHasherDefault<FxHasher>>;

#[derive(Debug)]
pub enum MetadataResponse {
    /// The wheel metadata was found and parsed successfully.
    Found(ArchiveMetadata),
    /// A non-fatal error.
    Unavailable(MetadataUnavailable),
    /// The distribution could not be built or downloaded, a fatal error.
    Error(Box<RequestedDist>, Arc<uv_distribution::Error>),
}

/// Non-fatal metadata fetching error.
///
/// This is also the unavailability reasons for a package, while version unavailability is separate
/// in [`UnavailableVersion`].
#[derive(Debug, Clone)]
pub enum MetadataUnavailable {
    /// The wheel metadata was not found in the cache and the network is not available.
    Offline,
    /// The wheel metadata was found, but could not be parsed.
    InvalidMetadata(Arc<uv_pypi_types::MetadataError>),
    /// The wheel metadata was found, but the metadata was inconsistent.
    InconsistentMetadata(Arc<uv_distribution::Error>),
    /// The wheel has an invalid structure.
    InvalidStructure(Arc<uv_metadata::Error>),
    /// The source distribution has a `requires-python` requirement that is not met by the installed
    /// Python version (and static metadata is not available).
    RequiresPython(VersionSpecifiers, Version),
    /// The wheel metadata could not be fetched due to a network error.
    Network(StatusCode),
}

impl MetadataUnavailable {
    /// Like [`std::error::Error::source`], but we don't want to derive the std error since our
    /// formatting system is more custom.
    pub fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Offline => None,
            Self::InvalidMetadata(err) => Some(err),
            Self::InconsistentMetadata(err) => Some(err),
            Self::InvalidStructure(err) => Some(err),
            Self::RequiresPython(..) | Self::Network(..) => None,
        }
    }
}
