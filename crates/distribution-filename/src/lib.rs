use pep440_rs::Version;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use uv_normalize::PackageName;

pub use build_tag::{BuildTag, BuildTagError};
pub use egg::{EggInfoFilename, EggInfoFilenameError};
pub use extension::{DistExtension, ExtensionError, SourceDistExtension};
pub use source_dist::SourceDistFilename;
pub use wheel::{WheelFilename, WheelFilenameError};

mod build_tag;
mod egg;
mod extension;
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
}

impl Display for DistFilename {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SourceDistFilename(filename) => Display::fmt(filename, f),
            Self::WheelFilename(filename) => Display::fmt(filename, f),
        }
    }
}
