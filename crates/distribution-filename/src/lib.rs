use pep440_rs::Version;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use uv_normalize::PackageName;

pub use build_tag::{BuildTag, BuildTagError};
pub use egg::{EggInfoFilename, EggInfoFilenameError};
pub use source_dist::{SourceDistExtension, SourceDistFilename, SourceDistFilenameError};
pub use wheel::{WheelFilename, WheelFilenameError};

mod build_tag;
mod egg;
mod source_dist;
mod wheel;

#[derive(Debug, Clone)]
pub enum DistFilename {
    SourceDistFilename(SourceDistFilename),
    WheelFilename(WheelFilename),
}

impl DistFilename {
    /// Parse a filename as wheel or source dist name.
    pub fn try_from_filename(filename: &str, package_name: &PackageName) -> Option<Self> {
        if let Ok(filename) = WheelFilename::from_str(filename) {
            Some(Self::WheelFilename(filename))
        } else if let Ok(filename) = SourceDistFilename::parse(filename, package_name) {
            Some(Self::SourceDistFilename(filename))
        } else {
            None
        }
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
}

impl Display for DistFilename {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceDistFilename(filename) => Display::fmt(filename, f),
            Self::WheelFilename(filename) => Display::fmt(filename, f),
        }
    }
}
