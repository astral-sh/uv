use std::borrow::Cow;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::Arc;

use reqwest::StatusCode;

use uv_distribution::Error as DistributionError;
use uv_distribution_types::{MinimumLibcVersion, Requirement, RequirementSource};
use uv_metadata::Error as WheelMetadataError;
use uv_normalize::{ExtraName, PackageName};
use uv_pep440::{Version, VersionSpecifiers};
use uv_pep508::MarkerTree;
use uv_platform_tags::{AbiTag, Tags};
use uv_pypi_types::MetadataError;
use uv_resolver_types::VariantsJsonMetadataError;

use crate::known_platform::KnownPlatform;
use crate::prioritized_distribution::IncompatibleDist;
use crate::pubgrub::Range;
use crate::resolver::{MetadataUnavailable, VersionFork};

/// The reason why a package or a version cannot be used.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum UnavailableReason {
    /// The entire package cannot be used.
    Package(UnavailablePackage),
    /// A single version cannot be used.
    Version(UnavailableVersion),
}

impl UnavailableReason {
    /// Return a reason that describes both sets of unavailable versions, if possible.
    pub(crate) fn merge(&self, other: &Self) -> Option<Self> {
        // Releases can fail the same requirement with different wheel tags. Report the oldest
        // advertised libc version across the range that PubGrub displays.
        if let (
            Self::Version(UnavailableVersion::MissingPlatform(left)),
            Self::Version(UnavailableVersion::MissingPlatform(right)),
        ) = (self, other)
            && left.marker == right.marker
            && left.required_libc == right.required_libc
        {
            return Some(Self::Version(UnavailableVersion::MissingPlatform(
                MissingPlatform {
                    available_libc: left.available_libc.union(right.available_libc),
                    ..*left
                },
            )));
        }
        (self == other).then(|| self.clone())
    }
}

impl Display for UnavailableReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Version(version) => Display::fmt(version, f),
            Self::Package(package) => Display::fmt(package, f),
        }
    }
}

/// A requirement whose version specifiers resolve to an empty range.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnsatisfiableRequirement {
    name: PackageName,
    extras: Box<[ExtraName]>,
    version_specifiers: VersionSpecifiers,
}

impl UnsatisfiableRequirement {
    /// Preserve a requirement whose specifiers collapse to an empty PubGrub range.
    ///
    /// PubGrub ranges do not retain the specifiers that produced them, but the original
    /// requirement is needed to explain the conflict in the resolution report.
    pub(crate) fn from_requirement(requirement: &Requirement) -> Option<Self> {
        let RequirementSource::Registry { specifier, .. } = &requirement.source else {
            return None;
        };
        (Range::from(specifier.clone()) == Range::empty()).then(|| Self {
            name: requirement.name.clone(),
            extras: requirement.extras.clone(),
            version_specifiers: specifier.clone(),
        })
    }

    fn fmt_package(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.name, f)?;
        if !self.extras.is_empty() {
            f.write_str("[")?;
            for (index, extra) in self.extras.iter().enumerate() {
                if index > 0 {
                    f.write_str(",")?;
                }
                Display::fmt(extra, f)?;
            }
            f.write_str("]")?;
        }
        Ok(())
    }
}

impl Display for UnsatisfiableRequirement {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for (index, specifier) in self.version_specifiers.iter().enumerate() {
            if index > 0 {
                f.write_str(" and ")?;
            }
            self.fmt_package(f)?;
            Display::fmt(specifier, f)?;
        }
        if self.version_specifiers.len() > 1 {
            f.write_str(", which are incompatible")
        } else {
            f.write_str(", which does not allow any versions")
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
    /// The version has a dependency whose version specifiers resolve to an empty range.
    UnsatisfiableDependency(UnsatisfiableRequirement),
    /// Version is incompatible because it has no usable distributions
    IncompatibleDist(IncompatibleDist),
    /// The version lacks wheels for a required environment.
    MissingPlatform(MissingPlatform),
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
            Self::UnsatisfiableDependency(requirement) => Cow::Owned(requirement.to_string()),
            Self::MissingPlatform(platform) => Cow::Owned(platform.to_string()),
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
            Self::UnsatisfiableDependency(requirement) => {
                format!("depends on {requirement}")
            }
            Self::IncompatibleDist(invalid_dist) => invalid_dist.singular_message(),
            Self::MissingPlatform(_) => format!("has {self}"),
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
            Self::UnsatisfiableDependency(requirement) => format!("depend on {requirement}"),
            Self::IncompatibleDist(invalid_dist) => invalid_dist.plural_message(),
            Self::MissingPlatform(_) => format!("have {self}"),
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
            Self::UnsatisfiableDependency(_) => None,
            Self::IncompatibleDist(invalid_dist) => {
                invalid_dist.context_message(tags, requires_python)
            }
            Self::MissingPlatform(_) => None,
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

/// A required environment that the available wheels do not cover.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct MissingPlatform {
    pub(super) marker: MarkerTree,
    pub(super) required_libc: MinimumLibcVersion,
    /// The oldest release of each libc advertised by otherwise matching wheels.
    pub(super) available_libc: MinimumLibcVersion,
}

impl Display for MissingPlatform {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if let Some(platform) = KnownPlatform::from_marker(self.marker) {
            write!(f, "no {platform}-compatible wheels")?;
        } else if let Some(marker) = self.marker.try_to_string() {
            write!(f, "no `{marker}`-compatible wheels")?;
        } else {
            f.write_str("no compatible wheels")?;
        }

        if self.available_libc == MinimumLibcVersion::default() {
            return Ok(());
        }
        let (family, required, minimum) = match (self.required_libc.glibc, self.required_libc.musl)
        {
            // The oldest glibc and musl tags can come from different package releases
            // or target disjoint Python versions within a release.
            (Some(glibc), Some(musl)) => {
                return write!(
                    f,
                    " that satisfy `minimum-libc-version` for both glibc {glibc} and musl {musl}"
                );
            }
            (Some(glibc), None) => ("glibc", glibc, self.available_libc.glibc),
            (None, Some(musl)) => ("musl", musl, self.available_libc.musl),
            (None, None) => return Ok(()),
        };
        write!(
            f,
            " that satisfy `minimum-libc-version` for {family} {required}"
        )?;
        if let Some(minimum) = minimum
            && minimum > required
        {
            write!(f, " (the minimum supported is {family} {minimum})")?;
        }
        Ok(())
    }
}

impl From<&MetadataUnavailable> for UnavailableVersion {
    fn from(reason: &MetadataUnavailable) -> Self {
        match reason {
            MetadataUnavailable::Offline => Self::Offline,
            MetadataUnavailable::InvalidMetadata(_)
            | MetadataUnavailable::InvalidVariantMetadata(_) => Self::InvalidMetadata,
            MetadataUnavailable::InconsistentMetadata(_) => Self::InconsistentMetadata,
            MetadataUnavailable::InvalidStructure(_) => Self::InvalidStructure,
            MetadataUnavailable::RequiresPython(requires_python, _python_version) => {
                Self::RequiresPython(requires_python.clone())
            }
            MetadataUnavailable::Network(status) => Self::Network(*status),
        }
    }
}

/// The package is unavailable and cannot be used.
#[derive(Debug, Clone, thiserror::Error)]
pub enum UnavailablePackage {
    /// Index lookups were disabled (i.e., `--no-index`) and the package was not found in a flat index (i.e. from `--find-links`).
    NoIndex,
    /// Network requests were disabled (i.e., `--offline`), and the package was not found in the cache.
    Offline,
    /// The package was not found in the registry.
    NotFound,
    /// The package metadata was found, but could not be parsed.
    InvalidMetadata(#[source] Arc<MetadataError>),
    /// The variant metadata was found, but could not be parsed.
    InvalidVariantMetadata(#[source] Arc<VariantsJsonMetadataError>),
    /// The package metadata was found, but the metadata was inconsistent.
    InconsistentMetadata(#[source] Arc<DistributionError>),
    /// The package has an invalid structure.
    InvalidStructure(#[source] Arc<WheelMetadataError>),
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
            Self::InvalidVariantMetadata(_) => Cow::Borrowed("invalid metadata"),
            Self::InconsistentMetadata(_) => Cow::Borrowed("invalid metadata"),
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
            Self::InvalidVariantMetadata(_) => format!("has {self}"),
            Self::InconsistentMetadata(_) => format!("has {self}"),
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

impl PartialEq for UnavailablePackage {
    /// Collapse reasons with the same category and cause messages.
    fn eq(&self, other: &Self) -> bool {
        if self.message() != other.message() {
            return false;
        }

        let mut source = self.source();
        let mut other_source = other.source();
        while let (Some(error), Some(other_error)) = (source, other_source) {
            if error.to_string().trim() != other_error.to_string().trim() {
                return false;
            }
            source = error.source();
            other_source = other_error.source();
        }
        source.is_none() && other_source.is_none()
    }
}

impl Eq for UnavailablePackage {}

impl From<&MetadataUnavailable> for UnavailablePackage {
    fn from(reason: &MetadataUnavailable) -> Self {
        match reason {
            MetadataUnavailable::Offline => Self::Offline,
            MetadataUnavailable::InvalidMetadata(err) => Self::InvalidMetadata(err.clone()),
            MetadataUnavailable::InvalidVariantMetadata(err) => {
                Self::InvalidVariantMetadata(err.clone())
            }
            MetadataUnavailable::InconsistentMetadata(err) => {
                Self::InconsistentMetadata(err.clone())
            }
            MetadataUnavailable::InvalidStructure(err) => Self::InvalidStructure(err.clone()),
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
