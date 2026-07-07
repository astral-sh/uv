use std::borrow::Cow;
use std::fmt::{Display, Formatter};
use std::iter;
use std::sync::Arc;

use reqwest::StatusCode;

use uv_distribution_types::IncompatibleDist;
use uv_pep440::{Version, VersionSpecifiers};
use uv_platform_tags::{AbiTag, Tags};

use crate::resolver::{MetadataUnavailable, VersionFork};

/// The reason why a package or a version cannot be used.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum UnavailableReason {
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
pub enum UnavailableVersion {
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
    /// The network request failed with the given status code.
    Network(StatusCode),
}

impl UnavailableVersion {
    fn message(&self) -> Cow<'static, str> {
        match self {
            Self::IncompatibleDist(invalid_dist) => Cow::Owned(format!("{invalid_dist}")),
            Self::InvalidMetadata => Cow::Borrowed("invalid metadata"),
            Self::InconsistentMetadata => Cow::Borrowed("inconsistent metadata"),
            Self::InvalidStructure => Cow::Borrowed("an invalid package format"),
            Self::Offline => Cow::Borrowed("to be downloaded from a registry"),
            Self::RequiresPython(requires_python) => {
                Cow::Owned(format!("Python {requires_python}"))
            }
            Self::Network(status) => Cow::Owned(status.to_string()),
        }
    }

    pub(crate) fn singular_message(&self) -> String {
        match self {
            Self::IncompatibleDist(invalid_dist) => invalid_dist.singular_message(),
            Self::InvalidMetadata => format!("has {self}"),
            Self::InconsistentMetadata => format!("has {self}"),
            Self::InvalidStructure => format!("has {self}"),
            Self::Offline => format!("needs {self}"),
            Self::RequiresPython(..) => format!("requires {self}"),
            Self::Network(..) => format!("could not be fetched from the network (`{self}`)"),
        }
    }

    pub(crate) fn plural_message(&self) -> String {
        match self {
            Self::IncompatibleDist(invalid_dist) => invalid_dist.plural_message(),
            Self::InvalidMetadata => format!("have {self}"),
            Self::InconsistentMetadata => format!("have {self}"),
            Self::InvalidStructure => format!("have {self}"),
            Self::Offline => format!("need {self}"),
            Self::RequiresPython(..) => format!("require {self}"),
            Self::Network(..) => format!("could not be fetched from the network (`{self}`)"),
        }
    }

    pub(crate) fn context_message(
        &self,
        tags: Option<&Tags>,
        requires_python: Option<AbiTag>,
    ) -> Option<String> {
        match self {
            Self::IncompatibleDist(invalid_dist) => {
                invalid_dist.context_message(tags, requires_python)
            }
            Self::InvalidMetadata => None,
            Self::InconsistentMetadata => None,
            Self::InvalidStructure => None,
            Self::Offline => None,
            Self::RequiresPython(..) => None,
            Self::Network(..) => None,
        }
    }
}

impl Display for UnavailableVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message().as_ref())
    }
}

impl From<&MetadataUnavailable> for UnavailableVersion {
    fn from(reason: &MetadataUnavailable) -> Self {
        match reason {
            MetadataUnavailable::Offline => Self::Offline,
            MetadataUnavailable::InvalidMetadata(_) => Self::InvalidMetadata,
            MetadataUnavailable::InconsistentMetadata(_) => Self::InconsistentMetadata,
            MetadataUnavailable::InvalidStructure(_) => Self::InvalidStructure,
            MetadataUnavailable::RequiresPython(requires_python, _python_version) => {
                Self::RequiresPython(requires_python.clone())
            }
            MetadataUnavailable::Network(status) => Self::Network(*status),
        }
    }
}

/// Display the error chain for unavailable packages.
#[derive(Debug, Clone)]
pub struct UnavailableErrorChain(Arc<dyn std::error::Error + Send + Sync + 'static>);

impl Display for UnavailableErrorChain {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for source in iter::successors(Some(&self.0 as &dyn std::error::Error), |&err| err.source())
        {
            writeln!(f, "Caused by: {}", source.to_string().trim())?;
        }
        Ok(())
    }
}

impl PartialEq for UnavailableErrorChain {
    /// Whether we can collapse two reasons into one because they would be rendered the same.
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}

impl Eq for UnavailableErrorChain {}

/// The package is unavailable and cannot be used.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum UnavailablePackage {
    /// Index lookups were disabled (i.e., `--no-index`) and the package was not found in a flat index (i.e. from `--find-links`).
    NoIndex,
    /// Network requests were disabled (i.e., `--offline`), and the package was not found in the cache.
    Offline,
    /// The package was not found in the registry.
    NotFound,
    /// The package metadata was found, but could not be parsed.
    InvalidMetadata(UnavailableErrorChain),
    /// The package has an invalid structure.
    InvalidStructure(UnavailableErrorChain),
    /// The network request failed with the given status code.
    Network(StatusCode),
}

impl UnavailablePackage {
    fn message(&self) -> Cow<'static, str> {
        match self {
            Self::NoIndex => Cow::Borrowed("not found in the provided package locations"),
            Self::Offline => Cow::Borrowed("not found in the cache"),
            Self::NotFound => Cow::Borrowed("not found in the package registry"),
            Self::InvalidMetadata(_) => Cow::Borrowed("invalid metadata"),
            Self::InvalidStructure(_) => Cow::Borrowed("an invalid package format"),
            Self::Network(status) => Cow::Owned(status.to_string()),
        }
    }

    pub(crate) fn singular_message(&self) -> String {
        match self {
            Self::NoIndex => format!("was {self}"),
            Self::Offline => format!("was {self}"),
            Self::NotFound => format!("was {self}"),
            Self::InvalidMetadata(_) => format!("has {self}"),
            Self::InvalidStructure(_) => format!("has {self}"),
            Self::Network(_) => format!("could not be fetched from the network (`{self}`)"),
        }
    }
}

impl Display for UnavailablePackage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message().as_ref())
    }
}

impl From<&MetadataUnavailable> for UnavailablePackage {
    fn from(reason: &MetadataUnavailable) -> Self {
        match reason {
            MetadataUnavailable::Offline => Self::Offline,
            MetadataUnavailable::InvalidMetadata(err) => {
                Self::InvalidMetadata(UnavailableErrorChain(err.clone()))
            }
            MetadataUnavailable::InconsistentMetadata(err) => {
                Self::InvalidMetadata(UnavailableErrorChain(err.clone()))
            }
            MetadataUnavailable::InvalidStructure(err) => {
                Self::InvalidStructure(UnavailableErrorChain(err.clone()))
            }
            MetadataUnavailable::RequiresPython(..) => {
                unreachable!("`requires-python` is only known upfront for registry distributions")
            }
            MetadataUnavailable::Network(status) => Self::Network(*status),
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

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use reqwest::StatusCode;

    use super::UnavailableVersion;

    #[test]
    fn unavailable_version_borrows_static_messages() {
        assert!(matches!(
            UnavailableVersion::InvalidMetadata.message(),
            Cow::Borrowed("invalid metadata")
        ));
        assert!(matches!(
            UnavailableVersion::Network(StatusCode::NOT_FOUND).message(),
            Cow::Owned(_)
        ));
    }
}
