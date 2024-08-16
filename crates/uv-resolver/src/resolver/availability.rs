use std::fmt::{Display, Formatter};

use distribution_types::IncompatibleDist;
use pep440_rs::Version;

/// The reason why a package or a version cannot be used.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum UnavailableReason {
    /// The entire package cannot be used.
    Package(UnavailablePackage),
    /// A single version cannot be used.
    Version(UnavailableVersion),
}

impl Display for UnavailableReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Version(version) => Display::fmt(version, f),
            Self::Package(package) => Display::fmt(package, f),
        }
    }
}

/// The package version is unavailable and cannot be used. Unlike [`PackageUnavailable`], this
/// applies to a single version of the package.
///
/// Most variant are from [`MetadataResponse`] without the error source (since we don't format
/// the source).
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum UnavailableVersion {
    /// Version is incompatible because it has no usable distributions
    IncompatibleDist(IncompatibleDist),
    /// The wheel metadata was not found.
    MissingMetadata,
    /// The wheel metadata was found, but could not be parsed.
    InvalidMetadata,
    /// The wheel metadata was found, but the metadata was inconsistent.
    InconsistentMetadata,
    /// The wheel has an invalid structure.
    InvalidStructure,
    /// The wheel metadata was not found in the cache and the network is not available.
    Offline,
}

impl Display for UnavailableVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            UnavailableVersion::IncompatibleDist(invalid_dist) => Display::fmt(invalid_dist, f),
            UnavailableVersion::MissingMetadata => {
                f.write_str("does not include a `METADATA` file")
            }
            UnavailableVersion::InvalidMetadata => f.write_str("has invalid metadata"),
            UnavailableVersion::InconsistentMetadata => f.write_str("has inconsistent metadata"),
            UnavailableVersion::InvalidStructure => f.write_str("has an invalid package format"),
            UnavailableVersion::Offline => f.write_str(
                "network connectivity is disabled, but the metadata wasn't found in the cache",
            ),
        }
    }
}

/// The package is unavailable and cannot be used.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum UnavailablePackage {
    /// Index lookups were disabled (i.e., `--no-index`) and the package was not found in a flat index (i.e. from `--find-links`).
    NoIndex,
    /// Network requests were disabled (i.e., `--offline`), and the package was not found in the cache.
    Offline,
    /// The package was not found in the registry.
    NotFound,
    /// The package metadata was not found.
    MissingMetadata,
    /// The package metadata was found, but could not be parsed.
    InvalidMetadata(String),
    /// The package has an invalid structure.
    InvalidStructure(String),
}

impl UnavailablePackage {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            UnavailablePackage::NoIndex => "was not found in the provided package locations",
            UnavailablePackage::Offline => "was not found in the cache",
            UnavailablePackage::NotFound => "was not found in the package registry",
            UnavailablePackage::MissingMetadata => "does not include a `METADATA` file",
            UnavailablePackage::InvalidMetadata(_) => "has invalid metadata",
            UnavailablePackage::InvalidStructure(_) => "has an invalid package format",
        }
    }
}

impl Display for UnavailablePackage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The package is unavailable at specific versions.
#[derive(Debug, Clone)]
pub(crate) enum IncompletePackage {
    /// Network requests were disabled (i.e., `--offline`), and the wheel metadata was not found in the cache.
    Offline,
    /// The wheel metadata was not found.
    MissingMetadata,
    /// The wheel metadata was found, but could not be parsed.
    InvalidMetadata(String),
    /// The wheel metadata was found, but the metadata was inconsistent.
    InconsistentMetadata(String),
    /// The wheel has an invalid structure.
    InvalidStructure(String),
}

#[derive(Debug, Clone)]
pub(crate) enum ResolverVersion {
    /// A usable version
    Available(Version),
    /// A version that is not usable for some reason
    Unavailable(Version, UnavailableVersion),
}
