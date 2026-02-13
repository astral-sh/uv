use std::fmt::{Display, Formatter};
use std::str::FromStr;

use either::Either;
use serde::{Deserialize, Serialize};
use uv_normalize::PackageName;
use uv_pep440::Version;
use uv_platform_tags::{AbiTag, LanguageTag, PlatformTag, TagCompatibility, Tags};

pub use build_tag::{BuildTag, BuildTagError};
pub use egg::{EggInfoFilename, EggInfoFilenameError};
pub use expanded_tags::{ExpandedTagError, ExpandedTags};
pub use extension::{DistExtension, ExtensionError, SourceDistExtension};
pub use source_dist::{SourceDistFilename, SourceDistFilenameError};
pub use wheel::{WheelFilename, WheelFilenameError};

/// Metadata about a wheel, stored in `uv_wheel.json` in the `.dist-info` directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WheelWireMetadata {
    /// The wheel filename (e.g., `torch-2.10.0-cp313-none-macosx_11_0_arm64.whl`).
    pub filename: WheelFilename,
}

/// Wheel tags from either a [`WheelFilename`] or an [`ExpandedTags`] (from the `WHEEL` file).
#[derive(Debug, Clone)]
pub enum WheelTags {
    /// Tags derived from the serialized wheel filename.
    Filename(WheelFilename),
    /// Tags read from the `WHEEL` file.
    Expanded(ExpandedTags),
}

impl WheelTags {
    /// Returns `true` if the wheel is compatible with the given tags.
    pub fn is_compatible(&self, tags: &Tags) -> bool {
        match self {
            Self::Filename(filename) => filename.is_compatible(tags),
            Self::Expanded(expanded) => expanded.is_compatible(tags),
        }
    }

    /// Return the [`TagCompatibility`] of the wheel with the given tags.
    pub fn compatibility(&self, tags: &Tags) -> TagCompatibility {
        match self {
            Self::Filename(filename) => filename.compatibility(tags),
            Self::Expanded(expanded) => expanded.compatibility(tags),
        }
    }

    /// Return the Python tags.
    pub fn python_tags(&self) -> impl Iterator<Item = LanguageTag> + '_ {
        match self {
            Self::Filename(filename) => Either::Left(filename.python_tags().iter().copied()),
            Self::Expanded(expanded) => Either::Right(expanded.python_tags().copied()),
        }
    }

    /// Return the ABI tags.
    pub fn abi_tags(&self) -> impl Iterator<Item = AbiTag> + '_ {
        match self {
            Self::Filename(filename) => Either::Left(filename.abi_tags().iter().copied()),
            Self::Expanded(expanded) => Either::Right(expanded.abi_tags().copied()),
        }
    }

    /// Return the platform tags.
    pub fn platform_tags(&self) -> impl Iterator<Item = PlatformTag> + '_ {
        match self {
            Self::Filename(filename) => Either::Left(filename.platform_tags().iter().cloned()),
            Self::Expanded(expanded) => Either::Right(expanded.platform_tags().cloned()),
        }
    }
}

mod build_tag;
mod egg;
mod expanded_tags;
mod extension;
mod source_dist;
mod splitter;
mod wheel;
mod wheel_tag;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DistFilename {
    SourceDistFilename(SourceDistFilename),
    WheelFilename(WheelFilename),
}

impl DistFilename {
    /// Parse a filename as wheel or source dist name.
    pub fn try_from_filename(filename: &str, package_name: &PackageName) -> Option<Self> {
        match DistExtension::from_path(filename) {
            Ok(DistExtension::Wheel) => {
                if let Ok(filename) = WheelFilename::from_str(filename) {
                    return Some(Self::WheelFilename(filename));
                }
            }
            Ok(DistExtension::Source(extension)) => {
                if let Ok(filename) = SourceDistFilename::parse(filename, extension, package_name) {
                    return Some(Self::SourceDistFilename(filename));
                }
            }
            Err(_) => {}
        }
        None
    }

    /// Like [`DistFilename::try_from_normalized_filename`], but without knowing the package name.
    ///
    /// Source dist filenames can be ambiguous, e.g. `a-1-1.tar.gz`. Without knowing the package name, we assume that
    /// source dist filename version doesn't contain minus (the version is normalized).
    pub fn try_from_normalized_filename(filename: &str) -> Option<Self> {
        if let Ok(filename) = WheelFilename::from_str(filename) {
            Some(Self::WheelFilename(filename))
        } else if let Ok(filename) = SourceDistFilename::parsed_normalized_filename(filename) {
            Some(Self::SourceDistFilename(filename))
        } else {
            None
        }
    }

    pub fn name(&self) -> &PackageName {
        match self {
            Self::SourceDistFilename(filename) => &filename.name,
            Self::WheelFilename(filename) => &filename.name,
        }
    }

    pub fn version(&self) -> &Version {
        match self {
            Self::SourceDistFilename(filename) => &filename.version,
            Self::WheelFilename(filename) => &filename.version,
        }
    }

    pub fn into_version(self) -> Version {
        match self {
            Self::SourceDistFilename(filename) => filename.version,
            Self::WheelFilename(filename) => filename.version,
        }
    }

    /// Whether the file is a `bdist_wheel` or an `sdist`.
    pub fn filetype(&self) -> &'static str {
        match self {
            Self::SourceDistFilename(_) => "sdist",
            Self::WheelFilename(_) => "bdist_wheel",
        }
    }
}

impl Display for DistFilename {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceDistFilename(filename) => Display::fmt(filename, f),
            Self::WheelFilename(filename) => Display::fmt(filename, f),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::WheelFilename;

    #[test]
    fn wheel_filename_size() {
        assert_eq!(size_of::<WheelFilename>(), 48);
    }
}
