use std::fmt::{Display, Formatter};

use crate::resolver::{MetadataUnavailable, VersionFork};
use uv_distribution_types::IncompatibleDist;
use uv_pep440::{Version, VersionSpecifiers};
use uv_platform_tags::{AbiTag, Tags};

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

/// The package version is unavailable and cannot be used. Unlike [`MetadataUnavailable`], this
/// applies to a single version of the package.
///
/// Most variant are from [`MetadataResponse`] without the error source, since we don't format
/// the source and we want to merge unavailable messages across versions.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum UnavailableVersion {
    /// Version is incompatible because it has no usable distributions
    IncompatibleDist(IncompatibleDist),
    /// The wheel metadata was found, but could not be parsed.
    InvalidMetadata,
    /// The wheel metadata was found, but the metadata was inconsistent.
    InconsistentMetadata,
    /// The wheel has an invalid structure.
    InvalidStructure,
    /// The wheel metadata was not found in the cache and the network is not available.
    Offline,
    /// The source distribution has a `requires-python` requirement that is not met by the installed
    /// Python version (and static metadata is not available).
    RequiresPython(VersionSpecifiers),
}

impl UnavailableVersion {
    pub(crate) fn message(&self) -> String {
        match self {
            UnavailableVersion::IncompatibleDist(invalid_dist) => format!("{invalid_dist}"),
            UnavailableVersion::InvalidMetadata => "invalid metadata".into(),
            UnavailableVersion::InconsistentMetadata => "inconsistent metadata".into(),
            UnavailableVersion::InvalidStructure => "an invalid package format".into(),
            UnavailableVersion::Offline => "to be downloaded from a registry".into(),
            UnavailableVersion::RequiresPython(requires_python) => {
                format!("Python {requires_python}")
            }
        }
    }

    pub(crate) fn singular_message(&self) -> String {
        match self {
            UnavailableVersion::IncompatibleDist(invalid_dist) => invalid_dist.singular_message(),
            UnavailableVersion::InvalidMetadata => format!("has {self}"),
            UnavailableVersion::InconsistentMetadata => format!("has {self}"),
            UnavailableVersion::InvalidStructure => format!("has {self}"),
            UnavailableVersion::Offline => format!("needs {self}"),
            UnavailableVersion::RequiresPython(..) => format!("requires {self}"),
        }
    }

    pub(crate) fn plural_message(&self) -> String {
        match self {
            UnavailableVersion::IncompatibleDist(invalid_dist) => invalid_dist.plural_message(),
            UnavailableVersion::InvalidMetadata => format!("have {self}"),
            UnavailableVersion::InconsistentMetadata => format!("have {self}"),
            UnavailableVersion::InvalidStructure => format!("have {self}"),
            UnavailableVersion::Offline => format!("need {self}"),
            UnavailableVersion::RequiresPython(..) => format!("require {self}"),
        }
    }

    pub(crate) fn context_message(
        &self,
        tags: Option<&Tags>,
        requires_python: Option<AbiTag>,
    ) -> Option<String> {
        match self {
            UnavailableVersion::IncompatibleDist(invalid_dist) => {
                invalid_dist.context_message(tags, requires_python)
            }
            UnavailableVersion::InvalidMetadata => None,
            UnavailableVersion::InconsistentMetadata => None,
            UnavailableVersion::InvalidStructure => None,
            UnavailableVersion::Offline => None,
            UnavailableVersion::RequiresPython(..) => None,
        }
    }
}

impl Display for UnavailableVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl From<&MetadataUnavailable> for UnavailableVersion {
    fn from(reason: &MetadataUnavailable) -> Self {
        match reason {
            MetadataUnavailable::Offline => UnavailableVersion::Offline,
            MetadataUnavailable::InvalidMetadata(_) => UnavailableVersion::InvalidMetadata,
            MetadataUnavailable::InconsistentMetadata(_) => {
                UnavailableVersion::InconsistentMetadata
            }
            MetadataUnavailable::InvalidStructure(_) => UnavailableVersion::InvalidStructure,
            MetadataUnavailable::RequiresPython(requires_python, _python_version) => {
                UnavailableVersion::RequiresPython(requires_python.clone())
            }
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
            UnavailablePackage::InvalidMetadata(_) => "invalid metadata",
            UnavailablePackage::InvalidStructure(_) => "an invalid package format",
        }
    }

    pub(crate) fn singular_message(&self) -> String {
        match self {
            UnavailablePackage::NoIndex => format!("was {self}"),
            UnavailablePackage::Offline => format!("was {self}"),
            UnavailablePackage::NotFound => format!("was {self}"),
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

impl From<&MetadataUnavailable> for UnavailablePackage {
    fn from(reason: &MetadataUnavailable) -> Self {
        match reason {
            MetadataUnavailable::Offline => Self::Offline,
            MetadataUnavailable::InvalidMetadata(err) => Self::InvalidMetadata(err.to_string()),
            MetadataUnavailable::InconsistentMetadata(err) => {
                Self::InvalidMetadata(err.to_string())
            }
            MetadataUnavailable::InvalidStructure(err) => Self::InvalidStructure(err.to_string()),
            MetadataUnavailable::RequiresPython(..) => {
                unreachable!("`requires-python` is only known upfront for registry distributions")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ResolverVersion {
    /// A version that is not usable for some reason
    Unavailable(Version, UnavailableVersion),
    /// A usable version
    Unforked(Version),
    /// A set of forks, optionally with resolved versions
    Forked(Vec<VersionFork>),
}
