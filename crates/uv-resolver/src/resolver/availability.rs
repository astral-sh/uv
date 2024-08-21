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

impl UnavailableVersion {
    pub(crate) fn message(&self) -> String {
        match self {
            UnavailableVersion::IncompatibleDist(invalid_dist) => format!("{invalid_dist}"),
            UnavailableVersion::MissingMetadata => "not include a `METADATA` file".into(),
            UnavailableVersion::InvalidMetadata => "invalid metadata".into(),
            UnavailableVersion::InconsistentMetadata => "inconsistent metadata".into(),
            UnavailableVersion::InvalidStructure => "an invalid package format".into(),
            UnavailableVersion::Offline => "to be downloaded from a registry".into(),
        }
    }

    pub(crate) fn singular_message(&self) -> String {
        match self {
            UnavailableVersion::IncompatibleDist(invalid_dist) => invalid_dist.singular_message(),
            UnavailableVersion::MissingMetadata => format!("does {self}"),
            UnavailableVersion::InvalidMetadata => format!("has {self}"),
            UnavailableVersion::InconsistentMetadata => format!("has {self}"),
            UnavailableVersion::InvalidStructure => format!("has {self}"),
            UnavailableVersion::Offline => format!("needs {self}"),
        }
    }

    pub(crate) fn plural_message(&self) -> String {
        match self {
            UnavailableVersion::IncompatibleDist(invalid_dist) => invalid_dist.plural_message(),
            UnavailableVersion::MissingMetadata => format!("do {self}"),
            UnavailableVersion::InvalidMetadata => format!("have {self}"),
            UnavailableVersion::InconsistentMetadata => format!("have {self}"),
            UnavailableVersion::InvalidStructure => format!("have {self}"),
            UnavailableVersion::Offline => format!("need {self}"),
        }
    }
}

impl Display for UnavailableVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
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
    pub(crate) fn message(&self) -> &'static str {
        match self {
            UnavailablePackage::NoIndex => "not found in the provided package locations",
            UnavailablePackage::Offline => "not found in the cache",
            UnavailablePackage::NotFound => "not found in the package registry",
            UnavailablePackage::MissingMetadata => "not include a `METADATA` file",
            UnavailablePackage::InvalidMetadata(_) => "invalid metadata",
            UnavailablePackage::InvalidStructure(_) => "an invalid package format",
        }
    }

    pub(crate) fn singular_message(&self) -> String {
        match self {
            UnavailablePackage::NoIndex => format!("was {self}"),
            UnavailablePackage::Offline => format!("was {self}"),
            UnavailablePackage::NotFound => format!("was {self}"),
            UnavailablePackage::MissingMetadata => format!("does {self}"),
            UnavailablePackage::InvalidMetadata(_) => format!("has {self}"),
            UnavailablePackage::InvalidStructure(_) => format!("has {self}"),
        }
    }
}

impl Display for UnavailablePackage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
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
